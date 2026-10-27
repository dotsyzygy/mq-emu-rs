//! Selftest checks for EverQuest's own UI windows: every top-level window
//! and the controls of the open ones against MacroQuest's `${Window[...]}`,
//! then opening, moving, clicking, tab switching, typing into and right
//! clicking real windows (the inventory window, the chat input), each put
//! back as it was.

use std::time::Duration;

use mq_emu::{Click, GameState, Mq, Plugin, Task, Window, WindowKind, WindowRect};

use crate::RustTest;
use crate::checks::{Check, Report, same_text};
use crate::game_checks::all_match;

/// How long a click may take to show.
const CLICK_TIMEOUT: Duration = Duration::from_secs(2);
/// The window the actions are tried on, and its done button (the default UI's names).
const INVENTORY: &str = "InventoryWindow";
const INVENTORY_DONE: &str = "IW_DoneButton";
/// The main chat window and its input, for typing.
const CHAT_INPUT: &str = "ChatWindow/CW_ChatInput";
/// Texts longer than this don't fit `${...}`'s result (MAX_STRING), so
/// they aren't compared.
const MAX_COMPARED_TEXT: usize = 1500;
/// How many rows of each list are compared.
const LIST_ROWS: usize = 5;

/// A boolean as MacroQuest shows it.
fn tf(value: bool) -> String {
    (if value { "TRUE" } else { "FALSE" }).to_owned()
}

/// Something MacroQuest shows as NULL when there's none.
fn or_null(value: Option<String>) -> String {
    value.unwrap_or_else(|| "NULL".into())
}

/// What's compared for any window, as `(what, ours, path)` rows; `at` is
/// the `${...}` expression for the window.
fn window_rows<P: Plugin>(mq: &Mq<P>, window: Window<'_>, label: &str, at: &str) -> Vec<(String, String, String)> {
    let location = window.location();
    let mut rows = vec![
        (format!("{label} open"), tf(window.is_open()), format!("${{{at}.Open}}")),
        (format!("{label} enabled"), tf(window.is_enabled()), format!("${{{at}.Enabled}}")),
        (format!("{label} minimized"), tf(window.is_minimized()), format!("${{{at}.Minimized}}")),
        (format!("{label} x"), location.left.to_string(), format!("${{{at}.X}}")),
        (format!("{label} y"), location.top.to_string(), format!("${{{at}.Y}}")),
        (format!("{label} width"), location.width().to_string(), format!("${{{at}.Width}}")),
        (format!("{label} height"), location.height().to_string(), format!("${{{at}.Height}}")),
        (format!("{label} style"), window.style().to_string(), format!("${{{at}.Style}}")),
        (format!("{label} bg color"), format!("{:x}", window.bg_color()), format!("${{{at}.BGColor}}")),
        (format!("{label} type"), or_null(window.type_name()), format!("${{{at}.Type}}")),
        (format!("{label} name"), or_null(window.name()), format!("${{{at}.Name}}")),
        (format!("{label} screen id"), or_null(window.screen_id()), format!("${{{at}.ScreenID}}")),
        (format!("{label} children"), tf(window.first_child().is_some()), format!("${{{at}.Children}}")),
        (format!("{label} focused"), tf(mq.focused_window() == Some(window)), format!("${{{at}.Highlighted}}")),
        (format!("{label} vscroll"), window.vertical_scroll().position.to_string(), format!("${{{at}.VScrollPos}}")),
    ];
    let text = window.text();
    if text.len() < MAX_COMPARED_TEXT {
        rows.push((format!("{label} text"), text, format!("${{{at}.Text}}")));
    }
    let tooltip = window.tooltip();
    if tooltip.len() < MAX_COMPARED_TEXT {
        rows.push((format!("{label} tooltip"), tooltip, format!("${{{at}.Tooltip}}")));
    }
    if let Some(checked) = window.is_checked() {
        rows.push((format!("{label} checked"), tf(checked), format!("${{{at}.Checked}}")));
    }
    if let Some(value) = window.gauge_value() {
        rows.push((format!("{label} value"), format!("{value:.2}"), format!("${{{at}.Value}}")));
    }
    if let Some(list) = window.as_list() {
        rows.push((format!("{label} items"), list.len().to_string(), format!("${{{at}.Items}}")));
        rows.push((format!("{label} selection"), list.selected().map_or(0, |row| row + 1).to_string(), format!("${{{at}.GetCurSel}}")));
        for row in 0..list.len().min(LIST_ROWS) {
            for column in 0..list.columns() {
                // A cell holding a window reads as that window (TRUE or FALSE).
                if list.cell_window(row, column).is_some() {
                    continue;
                }
                // A cell a short row lacks is NULL.
                let ours = or_null(list.text(row, column));
                rows.push((format!("{label} cell {row},{column}"), ours, format!("${{{at}.List[{},{}]}}", row + 1, column + 1)));
            }
        }
    }
    if let Some(tabs) = window.as_tabs() {
        rows.push((format!("{label} tabs"), tabs.len().to_string(), format!("${{{at}.TabCount}}")));
        rows.push((format!("{label} tab"), tabs.current().map_or(0, |page| page + 1).to_string(), format!("${{{at}.CurrentTabIndex}}")));
    }
    rows
}

pub async fn run(task: &Task<RustTest>, report: &mut Report) {
    let in_game = task.with(|mq| {
        if mq.game_state() != GameState::InGame {
            report.print(mq, "EQ UI windows", Check::Skip("needs to be in game".into()));
            return false;
        }
        reading(mq, report);
        true
    });
    if in_game {
        acting(task, report).await;
        typing(task, report).await;
        context_menu(task, report).await;
    }
}

fn reading<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let windows: Vec<Window<'_>> = mq.windows().collect();
    report.print(mq, &format!("windows() lists the game's top-level windows ({})", windows.len()), Check::that(windows.len() > 20, || {
        format!("only {} windows", windows.len())
    }));
    // Several windows can share a name; ${Window[name]} is a shown one.
    let found: Vec<(String, Window<'_>)> =
        windows.iter().filter_map(|w| w.name().map(|name| (name.clone(), w))).filter(|(name, w)| mq.window(name) == Some(**w)).map(|(n, w)| (n, *w)).collect();
    let missed: Vec<String> = windows.iter().filter_map(|w| w.name()).filter(|name| mq.window(name).is_none()).collect();
    report.print(mq, "window(name) finds every listed window by its name", Check::that(missed.is_empty(), || format!("not found: {missed:?}")));
    let rows = found.iter().flat_map(|(name, window)| window_rows(mq, *window, name, &format!("Window[{name}]")));
    report.print(mq, &format!("{} top-level windows match ${{Window[name]}} (open, place, style, color, names, text, focus)", found.len()), all_match(mq, rows));

    // The controls of the open windows, each by the first screen ID match,
    // as ${Window[top].Child[id]} finds it.
    let mut controls = 0;
    let mut kinds = std::collections::BTreeSet::new();
    let rows: Vec<_> = found
        .iter()
        .filter(|(_, top)| top.is_open())
        .flat_map(|(name, top)| {
            let top = *top;
            top.descendants()
                .filter_map(move |control| {
                    let id = control.screen_id()?;
                    // MacroQuest's Child[] checks the window itself first.
                    let shadowed = top.name().is_some_and(|n| n.eq_ignore_ascii_case(&id)) || top.screen_id().is_some_and(|s| s.eq_ignore_ascii_case(&id));
                    (!shadowed && top.child(&id) == Some(control)).then_some((id, control))
                })
                .map(move |(id, control)| (name.clone(), id, control))
        })
        .inspect(|(_, _, control)| {
            controls += 1;
            kinds.insert(control.kind().name());
        })
        .flat_map(|(name, id, control)| window_rows(mq, control, &format!("{name}/{id}"), &format!("Window[{name}].Child[{id}]")))
        .collect();
    report.print(mq, &format!("{controls} controls of open windows match ${{Window[...].Child[id]}} ({} kinds: lists, tabs, gauges, buttons...)", kinds.len()), all_match(mq, rows));

    let path = found.iter().filter(|(_, top)| top.is_open()).find_map(|(name, top)| {
        let control = top.descendants().find(|c| c.parent() != Some(*top) && c.screen_id().is_some())?;
        Some((format!("{name}/{}", control.screen_id()?), top.child(&control.screen_id()?)?))
    });
    report.print(mq, "window(\"Top/Control\") finds a control inside a window", match path {
        Some((path, control)) => Check::that(mq.window(&path) == Some(control), || format!("window({path:?}) is {:?}", mq.window(&path))),
        None => Check::Skip("no open window with nested controls".into()),
    });
}

/// The inventory window, opened, moved, its tabs switched and closed with
/// its done button; left as it was.
async fn acting(task: &Task<RustTest>, report: &mut Report) {
    let Some(was_open) = task.with(|mq| {
        let window = mq.window(INVENTORY);
        if window.is_none() {
            report.print(mq, "acting on the inventory window", Check::Skip(format!("no {INVENTORY}")));
        }
        window.map(|w| w.is_open())
    }) else {
        return;
    };
    task.with(|mq| {
        let window = mq.window(INVENTORY).expect("found above");
        window.open();
        report.print(mq, "open() shows the inventory window (${Window[InventoryWindow].Open})", same_text("TRUE", mq.parse("${Window[InventoryWindow].Open}")));

        let location = window.location();
        let moved = WindowRect::new(location.left + 1, location.top + 1, location.width(), location.height());
        window.set_location(moved);
        let now = mq.parse("${Window[InventoryWindow].X},${Window[InventoryWindow].Y}");
        report.print(mq, "set_location moves it by a pixel", same_text(format!("{},{}", moved.left, moved.top), now));
        window.set_location(location);
        let back = window.location();
        report.print(mq, "and back", Check::that(back == location, || format!("{back:?}, was {location:?}")));

        match window.descendants().find_map(|w| w.as_tabs()).filter(|tabs| tabs.len() > 1) {
            Some(tabs) => {
                let before = tabs.current().unwrap_or(0);
                let other = (before + 1) % tabs.len();
                tabs.select(other);
                let id = tabs.window().screen_id().unwrap_or_default();
                report.print(mq, &format!("tabs select page {} of {id}", other + 1), same_text(other + 1, mq.parse(&format!("${{Window[{INVENTORY}].Child[{id}].CurrentTabIndex}}"))));
                tabs.select(before);
                report.print(mq, "and back", Check::that(tabs.current() == Some(before), || format!("current is {:?}", tabs.current())));
            }
            None => report.print(mq, "selecting a tab", Check::Skip(format!("no tab box in {INVENTORY}"))),
        }
    });
    let clicked = task.with(|mq| {
        let done = mq.window(&format!("{INVENTORY}/{INVENTORY_DONE}"));
        match done {
            Some(done) => done.click(Click::Left),
            None => {
                report.print(mq, "clicking the done button", Check::Skip(format!("no {INVENTORY_DONE}")));
                false
            }
        }
    });
    if clicked {
        let closed = task.wait_until_timeout(|mq| mq.window(INVENTORY).is_some_and(|w| !w.is_open()), CLICK_TIMEOUT).await;
        task.with(|mq| {
            report.print(mq, &format!("click(Left) on {INVENTORY_DONE} closes the window"), Check::that(closed, || {
                format!("still open after {CLICK_TIMEOUT:?}: {}", mq.parse("${Window[InventoryWindow].Open}"))
            }))
        });
    }
    task.with(|mq| {
        let window = mq.window(INVENTORY).expect("found above");
        if was_open { window.open() } else { window.close() };
        report.print(mq, "the inventory window is left as it was (close() if it was closed)", same_text(tf(was_open), mq.parse("${Window[InventoryWindow].Open}")));
    });
}

/// Text typed into the chat input with set_text, then put back.
async fn typing(task: &Task<RustTest>, report: &mut Report) {
    task.with(|mq| {
        let Some(input) = mq.window(CHAT_INPUT).filter(|w| w.kind() == WindowKind::EditBox) else {
            report.print(mq, "set_text in an edit box", Check::Skip(format!("no edit box {CHAT_INPUT}")));
            return;
        };
        let before = input.text();
        let typed = "mq-emu selftest";
        let set = input.set_text(typed);
        let theirs = mq.parse("${Window[ChatWindow].Child[CW_ChatInput].Text}");
        report.print(mq, "set_text types into the chat input (${Window[ChatWindow].Child[CW_ChatInput].Text})", Check::that(set && theirs == typed, || {
            format!("set_text gave {set}, MacroQuest reads {theirs:?}")
        }));
        input.set_text(&before);
        report.print(mq, "and back", same_text(before, input.text()));
        let button = mq.windows().find_map(|w| w.descendants().find(|c| c.kind() == WindowKind::Button));
        report.print(mq, "set_text refuses windows that aren't edit boxes", Check::that(button.is_some_and(|b| !b.set_text("x")), || "a button took text".into()));
    });
}

/// The inventory window's context menu, opened with a right click, read
/// against ${Menu} and closed.
async fn context_menu(task: &Task<RustTest>, report: &mut Report) {
    let opened = task.with(|mq| {
        if mq.context_menu().is_some() {
            report.print(mq, "context menus", Check::Skip("a context menu is already open".into()));
            return None;
        }
        let window = mq.window(INVENTORY)?;
        let was_open = window.is_open();
        window.open();
        window.click(Click::Right);
        Some(was_open)
    });
    let Some(was_open) = opened else { return };
    let shown = task.wait_until_timeout(|mq| mq.context_menu().is_some(), CLICK_TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, "click(Right) on a window opens its context menu", Check::that(shown, || format!("${{Menu.NumVisibleMenus}} is {}", mq.parse("${Menu.NumVisibleMenus}"))));
        if let Some(menu) = mq.context_menu() {
            // MacroQuest can't read most of a menu on emu: ${Menu.NumItems} and
            // ${Menu.Items[n]} go by CContextMenu::NumItems, which on this client
            // isn't the menu's count (0 or garbage), and ${Menu.List} by the
            // window's XML type, which a menu
            // made in code doesn't have. ${Menu.Name} reads the first item
            // directly (GetItemText(0, 1)).
            report.note(mq, &format!("${{Menu.NumItems}} says {} (unused on emu); the menu has {} rows: {:?}", mq.parse("${Menu.NumItems}"), menu.len(), menu.items().collect::<Vec<_>>()));
            report.print(mq, "one context menu is open (${Menu.NumVisibleMenus})", same_text(1, mq.parse("${Menu.NumVisibleMenus}")));
            report.print(mq, "its first item is ${Menu.Name}", same_text(menu.item(0).unwrap_or_default(), mq.parse("${Menu.Name}")));
            let filled = menu.items().filter(|item| !item.is_empty()).count();
            report.print(mq, "its items have text (separators aside)", Check::that(menu.len() > 1 && filled > menu.len() / 2, || {
                format!("{filled} of {} items have text", menu.len())
            }));
            let closed = menu.close();
            report.print(mq, "close() closes it", Check::that(closed && mq.context_menu().is_none(), || {
                format!("close() gave {closed}; ${{Menu.NumVisibleMenus}} is {}", mq.parse("${Menu.NumVisibleMenus}"))
            }));
        }
        let window = mq.window(INVENTORY).expect("found above");
        if was_open { window.open() } else { window.close() };
    });
}
