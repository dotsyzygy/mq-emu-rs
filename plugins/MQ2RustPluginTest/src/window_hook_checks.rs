//! Selftest checks for hooking the game's windows and acting on items
//! without them: the inventory window subclassed (its done button's click
//! handled by the plugin so the window stays open, the mouse over it, drawing
//! on it each frame, and the subclass taken off again), and an item in a
//! closed bag picked up and put back with no slot window.

use std::ops::ControlFlow;
use std::time::Duration;

use mq_emu::{GameState, ItemLocation, Mq, SlotLocation, Task, WindowHooks, WindowRect};

use crate::RustTest;
use crate::checks::{Check, Report};

const INVENTORY: &str = "InventoryWindow";
const DONE: &str = "IW_DoneButton";
/// How long a hooked event or an item move may take.
const TIMEOUT: Duration = Duration::from_secs(2);

/// Waits until `seen` holds for the plugin, up to [`TIMEOUT`].
async fn wait_for(task: &Task<RustTest>, seen: impl Fn(&RustTest, &Mq<RustTest>) -> bool) -> bool {
    for _ in 0..(TIMEOUT.as_millis() / 50) {
        if task.with_plugin(|plugin, mq| seen(plugin, mq)) {
            return true;
        }
        task.delay(Duration::from_millis(50)).await;
    }
    task.with_plugin(|plugin, mq| seen(plugin, mq))
}

fn hook_events(task: &Task<RustTest>) -> Vec<String> {
    task.with_plugin(|plugin, _| plugin.hook_events.clone())
}

/// The selftest's hooks on the inventory window: notifications and the
/// mouse recorded, the done button's click handled while `block_done`, and
/// a bar drawn along the top of the window each frame.
fn inventory_hooks() -> WindowHooks<RustTest> {
    WindowHooks::new()
        .on_notification(|plugin: &mut RustTest, _, event| {
            // The button's name is IW_DoneButton (what the path finds it by);
            // its screen ID is DoneButton.
            let done = event.sender().is_some_and(|s| s.name().as_deref() == Some(DONE));
            let sender = event.sender().and_then(|s| s.name().or_else(|| s.screen_id())).unwrap_or_else(|| "-".into());
            let what = event.notification().map_or_else(|| event.message().to_string(), |n| n.name().to_owned());
            plugin.hook_events.push(format!("{what} {sender}"));
            if plugin.block_done && done { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
        })
        .on_mouse(|plugin, _, event| {
            let (x, y) = event.position();
            plugin.hook_events.push(format!("mouse {:?} {x} {y}", event.action()));
            ControlFlow::Continue(())
        })
        .on_draw(|plugin, _, painter| {
            plugin.hook_draws += 1;
            let clip = painter.clip_rect();
            painter.fill_rect(WindowRect::new(clip.left, clip.top, clip.width(), 2), 0xffff_d000);
        })
}

pub async fn subclassing(task: &Task<RustTest>, report: &mut Report) {
    let Some(was_open) = task.with(|mq| {
        let window = mq.window(INVENTORY);
        if window.is_none() {
            report.print(mq, "subclassing the inventory window", Check::Skip(format!("no {INVENTORY}")));
        }
        window.map(|w| w.is_open())
    }) else {
        return;
    };

    let id = task.with_plugin(|plugin, mq| {
        plugin.hook_events.clear();
        plugin.hook_draws = 0;
        plugin.block_done = true;
        let id = mq.subclass_window(INVENTORY, inventory_hooks());
        let on = mq.subclassed_window(id);
        report.print(mq, "subclass_window subclasses the inventory window at once", Check::that(on.and_then(|w| w.name()).as_deref() == Some(INVENTORY), || {
            format!("subclassed_window is {:?}", on.and_then(|w| w.name()))
        }));
        if let Some(window) = mq.window(INVENTORY) {
            window.open();
        }
        id
    });

    // The game draws it each frame; the subclass's PostDraw reaches on_draw.
    let drew = wait_for(task, |plugin, _| plugin.hook_draws >= 3).await;
    task.with_plugin(|plugin, mq| {
        report.print(mq, "on_draw runs each frame it's shown (drawing a bar on it)", Check::that(drew, || format!("{} draws", plugin.hook_draws)))
    });

    // MacroQuest's /notify runs from its command queue, outside the
    // plugin's callbacks, so the hook's answer reaches the window.
    task.with(|mq| mq.command(format!("/notify {INVENTORY} {DONE} leftmouseup")));
    let got = wait_for(task, |plugin, _| plugin.hook_events.iter().any(|e| e.ends_with(&format!(" {DONE}")))).await;
    task.next_pulse().await;
    task.with(|mq| {
        report.print(mq, &format!("on_notification gets {DONE}'s click"), Check::that(got, || format!("got {:?}", hook_events(task))));
        let open = mq.window(INVENTORY).is_some_and(|w| w.is_open());
        report.print(mq, "handling it (ControlFlow::Break) keeps the window from closing", Check::that(open, || "it closed".into()));
    });

    // A click on the window itself, not one of its controls: the mouse hook.
    let rect = task.with(|mq| {
        mq.command(format!("/notify {INVENTORY} 0 leftmouse"));
        mq.command(format!("/notify {INVENTORY} 0 leftmouseup"));
        mq.window(INVENTORY).map(|w| w.screen_rect())
    });
    let pressed = wait_for(task, |plugin, _| plugin.hook_events.iter().any(|e| e.starts_with("mouse LeftUp"))).await;
    task.with(|mq| {
        let events = hook_events(task);
        let down = events.iter().find_map(|e| {
            let rest = e.strip_prefix("mouse LeftDown ")?;
            let (x, y) = rest.split_once(' ')?;
            Some((x.parse::<i32>().ok()?, y.parse::<i32>().ok()?))
        });
        let inside = match (down, rect) {
            (Some((x, y)), Some(r)) => x >= r.left && x < r.right && y >= r.top && y < r.bottom,
            _ => false,
        };
        report.print(mq, "on_mouse gets a click on the window, where it is on screen", Check::that(pressed && inside, || {
            format!("got {events:?}; the window is at {rect:?}")
        }));
    });

    // Taken off, the done button closes the window again.
    let removed = task.with_plugin(|plugin, mq| {
        plugin.hook_events.clear();
        let removed = mq.remove_subclass(id);
        report.print(mq, "remove_subclass takes it off", Check::that(removed && mq.subclassed_window(id).is_none(), || format!("removed: {removed}")));
        mq.command(format!("/notify {INVENTORY} {DONE} leftmouseup"));
        removed
    });
    if removed {
        let closed = task.wait_until_timeout(|mq| mq.window(INVENTORY).is_some_and(|w| !w.is_open()), TIMEOUT).await;
        let draws = task.with_plugin(|plugin, _| plugin.hook_draws);
        task.next_pulse().await;
        task.with_plugin(|plugin, mq| {
            report.print(mq, "then the done button closes the window, and the hooks get nothing", Check::that(
                closed && plugin.hook_events.is_empty() && plugin.hook_draws == draws,
                || format!("closed: {closed}; got {:?}, {} more draws", plugin.hook_events, plugin.hook_draws - draws),
            ));
        });
    }
    task.with_plugin(|plugin, mq| {
        plugin.block_done = false;
        if let Some(window) = mq.window(INVENTORY) {
            if was_open { window.open() } else { window.close() };
        }
    });
}

/// An item in a bag no window shows: its place, ID and stack.
fn item_in_closed_bag(mq: &Mq<RustTest>) -> Option<(ItemLocation, i32, i32)> {
    let inventory = mq.inventory()?;
    inventory.slots().all().find_map(|(at, item)| {
        let in_closed_bag = at.in_bag.is_some() && mq.item_slot_window(SlotLocation::inventory(at)).is_none();
        in_closed_bag.then(|| (at, item.id(), item.stack()))
    })
}

fn item_at(mq: &Mq<RustTest>, at: ItemLocation) -> Option<(i32, i32)> {
    mq.inventory()?.slots().all().find(|(place, _)| *place == at).map(|(_, item)| (item.id(), item.stack()))
}

fn cursor(mq: &Mq<RustTest>) -> Option<(i32, i32)> {
    mq.inventory()?.cursor().map(|item| (item.id(), item.stack()))
}

pub async fn items(task: &Task<RustTest>, report: &mut Report) {
    let found = task.with(|mq| {
        let skip = if mq.game_state() != GameState::InGame {
            Some("needs to be in game".to_owned())
        } else if cursor(mq).is_some() {
            Some("something is on the cursor".to_owned())
        } else if mq.window("MerchantWnd").is_some_and(|w| w.is_open()) {
            Some("the merchant window is open (picking up selects instead)".to_owned())
        } else {
            None
        };
        let found = if skip.is_none() { item_in_closed_bag(mq) } else { None };
        match (&skip, found) {
            (Some(why), _) => report.print(mq, "picking up an item no window shows", Check::Skip(why.clone())),
            (None, None) => report.print(mq, "picking up an item no window shows", Check::Skip("no item in a closed bag".into())),
            _ => {}
        }
        found
    });
    let Some((at, id, stack)) = found else { return };
    let place = SlotLocation::inventory(at);

    let picked = task.with(|mq| mq.pick_up_item(place));
    let on_cursor = task.wait_until_timeout(move |mq| cursor(mq) == Some((id, stack)), TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, &format!("pick_up_item({place}) puts it on the cursor, with no slot window"), Check::that(picked && on_cursor, || {
            format!("pick_up_item: {picked}; the cursor has {:?}, wanted {:?}", cursor(mq), (id, stack))
        }));
    });
    // Put back whatever happened, unless the cursor is empty.
    let dropped = task.with(|mq| cursor(mq).is_some() && mq.drop_item(place));
    let back = task.wait_until_timeout(move |mq| cursor(mq).is_none() && item_at(mq, at) == Some((id, stack)), TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, &format!("drop_item({place}) puts it back"), Check::that(dropped && back, || {
            format!("drop_item: {dropped}; there: {:?}, cursor: {:?}", item_at(mq, at), cursor(mq))
        }));
    });
}
