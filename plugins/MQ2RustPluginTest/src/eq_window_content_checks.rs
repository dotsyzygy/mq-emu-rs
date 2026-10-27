//! Selftest checks for what EverQuest's UI windows hold: every item slot
//! window and hot button against `${Window[...].InvSlot}` and `.HotButton`,
//! the trade window against `${Window.MyTradeReady}`; item slots found as
//! `/itemnotify` finds them and right clicked to open a bag; and a link sent
//! to the chat window, which opens the item it names.

use std::time::Duration;

use mq_emu::{Click, GameState, InvSlot, ItemContainerKind, Mq, Plugin, SlotLocation, Task, Window};

use crate::RustTest;
use crate::checks::{Check, Report, same_text};
use crate::game_checks::all_match;

/// How long a click or link may take to show.
const CLICK_TIMEOUT: Duration = Duration::from_secs(2);
/// The main chat window and its output, which handles links.
const CHAT: &str = "ChatWindow";
const CHAT_OUTPUT: &str = "CW_ChatOutput";
/// The start of the item display windows' names.
const ITEM_DISPLAY: &str = "ItemDisplayWindow";

fn tf(value: bool) -> String {
    (if value { "TRUE" } else { "FALSE" }).to_owned()
}

fn or_null(value: Option<String>) -> String {
    value.unwrap_or_else(|| "NULL".into())
}

/// `Window[top].Child[id]` for a control `${Window[...]}` reaches: its
/// top-level window by name, and it the first control by its screen ID.
fn tlo_path<P: Plugin>(mq: &Mq<P>, control: Window<'_>) -> Option<String> {
    let top = std::iter::successors(Some(control), Window::parent).last()?;
    let name = top.name()?;
    let id = control.screen_id()?;
    // MacroQuest's Child[] checks the window itself first.
    let shadowed = name.eq_ignore_ascii_case(&id) || top.screen_id().is_some_and(|s| s.eq_ignore_ascii_case(&id));
    (!shadowed && mq.window(&name) == Some(top) && top.child(&id) == Some(control)).then(|| format!("Window[{name}].Child[{id}]"))
}

pub async fn run(task: &Task<RustTest>, report: &mut Report) {
    let in_game = task.with(|mq| {
        if mq.game_state() != GameState::InGame {
            report.print(mq, "item slots, hot buttons, the trade window and links", Check::Skip("needs to be in game".into()));
            return false;
        }
        item_slots(mq, report);
        hot_buttons(mq, report);
        trade(mq, report);
        true
    });
    if in_game {
        bag_click(task, report).await;
        link(task, report).await;
    }
}

fn item_slots<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let slots: Vec<_> = mq.item_slot_windows().collect();
    report.print(mq, &format!("item_slot_windows() lists the game's item slots ({})", slots.len()), Check::that(slots.len() > 30, || {
        format!("only {} item slots", slots.len())
    }));
    let mut compared = 0;
    let rows: Vec<_> = slots
        .iter()
        .filter_map(|slot| Some((tlo_path(mq, slot.window())?, slot)))
        .inspect(|_| compared += 1)
        .flat_map(|(at, slot)| {
            let raw = slot.raw();
            let (x, y) = slot.item_offset();
            let label = at.clone();
            let mut rows = vec![
                ("location", raw.ItemLocation().Location().0.to_string(), "ItemLocation"),
                ("quantity", slot.quantity().to_string(), "Quantity"),
                ("selected", tf(slot.is_selected()), "Selected"),
                ("find selected", tf(slot.is_find_selected()), "FindSelected"),
                ("hot button", tf(slot.is_hot_button()), "HotButton"),
                ("linked", tf(slot.is_inventory_slot_linked()), "InventorySlotLinked"),
                ("mode", slot.mode().to_string(), "Mode"),
                ("offset x", x.to_string(), "ItemOffsetX"),
                ("offset y", y.to_string(), "ItemOffsetY"),
                ("item", or_null(slot.item().map(|item| item.name())), "Item"),
            ];
            // MacroQuest reads these without checking for none, which crashes.
            if !raw.pTextObject().is_null() {
                rows.push(("text", slot.text(), "Text"));
            }
            if !raw.pBackground().is_null() {
                rows.push(("background", slot.background().unwrap_or_default(), "Background"));
            }
            rows.into_iter().map(move |(what, ours, member)| (format!("{label} {what}"), ours, format!("${{{at}.InvSlot.{member}}}")))
        })
        .collect();
    report.print(mq, &format!("{compared} item slots match ${{Window[...].InvSlot}} (place, item, quantity, flags, text)"), all_match(mq, rows));

    // /itemnotify <slot> finds what you wear.
    let Some(inventory) = mq.inventory() else { return };
    let worn: Vec<_> = inventory.worn().collect();
    let wrong: Vec<String> = worn
        .iter()
        .filter(|(slot, item)| {
            let found = mq.item_slot_window(SlotLocation::inv_slot(*slot));
            found.and_then(|w| w.item()).map(|i| i.id()) != Some(item.id())
        })
        .map(|(slot, _)| slot.to_string())
        .collect();
    report.print(mq, &format!("item_slot_window finds the slot of each worn item, showing it ({})", worn.len()), match worn.len() {
        0 => Check::Skip("wearing nothing".into()),
        _ => Check::that(wrong.is_empty(), || format!("wrong or missing: {wrong:?}")),
    });
    let named = SlotLocation::parse("mainhand").and_then(|location| mq.item_slot_window(location)).and_then(|w| w.location());
    report.print(mq, "SlotLocation::parse(\"mainhand\") is the main hand slot, as /itemnotify reads it", Check::that(
        named == Some(SlotLocation::inv_slot(InvSlot::PRIMARY)),
        || format!("found {named:?}"),
    ));
}

fn hot_buttons<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let bars: Vec<_> = (0..10).filter_map(|bar| mq.hot_bar(bar)).collect();
    report.print(mq, &format!("hot_bar finds the hot bars ({})", bars.len()), Check::that(!bars.is_empty(), || "no HotButtonWnd".into()));
    let mut compared = 0;
    let rows: Vec<_> = bars
        .iter()
        .flat_map(|bar| bar.buttons().map(move |button| (bar.page(), button)))
        .filter_map(|(page, button)| Some((tlo_path(mq, button.window())?, page, button)))
        .inspect(|_| compared += 1)
        .flat_map(|(at, page, button)| {
            let mut rows = vec![
                ("bar", (button.bar() + 1).to_string(), "BarIndex"),
                ("button", (button.index() + 1).to_string(), "ButtonIndex"),
                ("page", (page + 1).to_string(), "PageIndex"),
                ("type", button.button_type().raw().to_string(), "Type"),
                ("type name", button.button_type().name().to_owned(), "TypeName"),
                ("slot", button.slot().to_string(), "Slot"),
                ("label", button.label(), "Label"),
                ("item id", button.item_id().to_string(), "ItemId"),
                ("item guid", button.item_guid(), "ItemGuid"),
                ("icon type", button.icon_type().to_string(), "IconType"),
                ("icon slot", button.icon_slot().to_string(), "IconSlot"),
                ("item", or_null(button.item().map(|item| item.name())), "Item"),
            ];
            if let Some(gem) = button.gem() {
                rows.push(("spell", or_null(mq.gem(gem).map(|spell| spell.name())), "Spell.Name"));
            }
            if let Some(id) = button.alt_ability_id() {
                rows.push(("alt ability", or_null(mq.alt_ability(id).map(|ability| ability.name())), "AltAbility.Name"));
            }
            let label = at.clone();
            rows.into_iter().map(move |(what, ours, member)| (format!("{label} {what}"), ours, format!("${{{at}.HotButton.{member}}}")))
        })
        .collect();
    report.print(mq, &format!("{compared} hot buttons match ${{Window[...].HotButton}} (bar, page, type, slot, label, item, spell)"), all_match(mq, rows));

    // Pressing an empty button does nothing, so it's safe to try.
    let empty = bars.iter().flat_map(|bar| bar.buttons()).find(|b| b.button_type() == mq_emu::HotButtonType::None);
    report.print(mq, "activate() presses an (empty) hot button through its bar", match empty {
        Some(button) => Check::that(button.activate(), || "its bar wasn't found".into()),
        None => Check::Skip("no empty hot button".into()),
    });
}

fn trade<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let Some(trade) = mq.trade_window() else {
        report.print(mq, "the trade window", Check::Skip("the game has no trade window".into()));
        return;
    };
    let name = trade.window().name().unwrap_or_else(|| "TradeWnd".into());
    let rows = [
        ("mine ready", tf(trade.my_ready()), format!("${{Window[{name}].MyTradeReady}}")),
        ("theirs ready", tf(trade.their_ready()), format!("${{Window[{name}].HisTradeReady}}")),
    ]
    .map(|(what, ours, path)| (what.to_owned(), ours, path));
    report.print(mq, "the trade window's ready flags match ${Window.MyTradeReady} and .HisTradeReady", all_match(mq, rows));
    let wrong: Vec<usize> = (0..16)
        .filter(|slot| trade.slot_window(*slot).and_then(|w| w.location()) != Some(SlotLocation::new(ItemContainerKind::Trade, *slot, None)))
        .collect();
    report.print(mq, "its 16 item slots show trade slots 1 to 16", Check::that(wrong.is_empty(), || format!("wrong: {wrong:?}")));
    if !trade.is_trading() {
        let offered = trade.items().iter().count();
        report.print(mq, "nothing is offered while not trading", Check::that(offered == 0, || format!("{offered} items")));
    }
}

/// `/itemnotify pack<n> rightmouseup`: right clicking a bag's slot opens it,
/// and again closes it.
async fn bag_click(task: &Task<RustTest>, report: &mut Report) {
    let bag = task.with(|mq| {
        let inventory = mq.inventory()?;
        let found = inventory.bags().find(|(_, bag)| bag.is_container() && bag.container_slots() > 0);
        if found.is_none() {
            report.print(mq, "right clicking a bag's slot", Check::Skip("no bag in a bag slot".into()));
        }
        found.map(|(slot, _)| slot)
    });
    let Some(bag) = bag else { return };
    // The bag's window is found by the slot's name ("pack1").
    let name = SlotLocation::inv_slot(bag).to_string();
    let is_open = |mq: &Mq<RustTest>, name: &str| mq.window(name).is_some_and(|w| w.is_open());
    let was_open = task.with(|mq| is_open(mq, &name));
    // Open (or close) it, then put it back.
    for want_open in [!was_open, was_open] {
        let clicked = task.with(|mq| mq.item_slot_window(SlotLocation::inv_slot(bag)).is_some_and(|slot| slot.window().click(Click::Right)));
        let waited_for = name.clone();
        let toggled = clicked && task.wait_until_timeout(move |mq| is_open(mq, &waited_for) == want_open, CLICK_TIMEOUT).await;
        task.with(|mq| {
            let what = if want_open { "opens" } else { "closes" };
            report.print(mq, &format!("click(Right) on {name}'s item slot {what} the bag, as /itemnotify {name} rightmouseup does"), Check::that(toggled, || {
                format!("clicked: {clicked}; ${{Window[{name}].Open}} is {}", mq.parse(&format!("${{Window[{name}].Open}}")))
            }));
            // While it's open, the slots in it can be found too.
            if toggled && want_open {
                let inside = SlotLocation::new(ItemContainerKind::Inventory, usize::from(bag.0), Some(0));
                let found = mq.item_slot_window(inside).and_then(|w| w.location());
                report.print(mq, &format!("item_slot_window finds \"{inside}\" while the bag is open"), Check::that(found == Some(inside), || {
                    format!("found {found:?}")
                }));
            }
        });
        if !toggled {
            break;
        }
    }
    task.with(|mq| report.print(mq, &format!("{name} is left as it was"), same_text(tf(was_open), tf(is_open(mq, &name)))));
}

/// The item display window showing `title`, found as
/// `${Window[ItemDisplayWindow]}` finds it (a shown one first).
fn showing<'a, P: Plugin>(mq: &'a Mq<P>, title: &str) -> Option<Window<'a>> {
    mq.window(ITEM_DISPLAY).filter(|w| w.is_open() && w.text() == title)
}

/// A link sent to the chat window (`/notify ChatWindow CW_ChatOutput link
/// <code>`) opens the item it names.
async fn link(task: &Task<RustTest>, report: &mut Report) {
    let sent = task.with(|mq| {
        let inventory = mq.inventory()?;
        let Some((slot, item)) = inventory.worn().next() else {
            report.print(mq, "sending a link", Check::Skip("wearing nothing to link".into()));
            return None;
        };
        let name = item.name();
        // "\x12<tag><code><name>\x12", the tag "0" for an item: the code is
        // what the game's link notification carries (eqlib's ExecuteTextLink).
        let link = mq.parse(&format!("${{Me.Inventory[{}].ItemLink[CLICKABLE]}}", slot.0));
        let code = link.strip_prefix("\u{12}0").and_then(|rest| rest.strip_suffix(&format!("{name}\u{12}")));
        let Some(code) = code else {
            report.print(mq, "sending a link", Check::Fail(format!("can't read the link of {name}: {link:?}")));
            return None;
        };
        let (chat, output) = (mq.window(CHAT), mq.window(&format!("{CHAT}/{CHAT_OUTPUT}")));
        let Some(chat) = chat else {
            report.print(mq, "sending a link", Check::Skip(format!("no {CHAT}")));
            return None;
        };
        if showing(mq, &name).is_some() {
            report.print(mq, "sending a link", Check::Skip(format!("{name} is already shown")));
            return None;
        }
        let notified = chat.notify_link(output, code);
        Some((name, notified))
    });
    let Some((name, notified)) = sent else { return };
    let item = name.clone();
    let shown = notified && task.wait_until_timeout(move |mq| showing(mq, &item).is_some(), CLICK_TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, &format!("notify_link to {CHAT} opens {name} in an item display window"), Check::that(shown, || {
            let found = mq.window(ITEM_DISPLAY).map(|w| (w.is_open(), w.text()));
            format!("notify_link gave {notified}; ${{Window[{ITEM_DISPLAY}]}} (open, title): {found:?}")
        }));
        let Some(display) = showing(mq, &name) else { return };
        // Item display windows are made when needed, by the game's display
        // manager: say how windows() sees them (it lists the window
        // manager's top-level screens).
        let listed = mq.windows().any(|w| w == display);
        let top = std::iter::successors(Some(display), Window::parent).last().and_then(|w| w.name());
        report.note(mq, &format!(
            "windows() {} the item display window (kind {}, top-level window {top:?})",
            if listed { "lists" } else { "doesn't list" },
            display.kind().name(),
        ));
        display.close();
    });
}
