//! Subclassing the game's windows, and what hooked windows pass on, against
//! fake windows: subclassed when the game has the window, taken off before
//! the UI is cleaned up and at unload, put back on when the UI loads; the
//! plugin's answer reaching the window; drawing with a painter; the mouse.

use std::ops::ControlFlow;

use mq_emu_sys::constants::{UI_BUTTON, UI_SCREEN, XWM_LCLICK};
use mq_emu_sys::layouts::{CXWnd, CXWndManager};
use mq_emu_sys::windows::{HOOK_LBUTTON_DOWN, HOOK_MOUSE_MOVE, HOOK_WHEEL, mqemu_rect};

use super::*;
use crate::imgui::EqTextFlags;
use crate::items::tests::Fake;
use crate::windows::fake;
use crate::{MouseAction, SubclassId, WindowHooks, WindowRect};

fn leak<T>() -> &'static Fake<T> {
    Box::leak(Box::new(Fake::new()))
}

/// The game's "Inventory" window, holding a "Done" button. `listed`: the
/// window manager has it (the game hasn't deleted it).
struct Game {
    window: *mut CXWnd,
    done: *mut CXWnd,
    manager: &'static Fake<CXWndManager>,
}

impl Game {
    fn new() -> Game {
        fake::reset();
        let [window, done] = [leak::<CXWnd>(), leak()].map(Fake::ptr);
        fake::add(window, UI_SCREEN, Some("Inventory"), Some("InventoryWindow"));
        fake::add(done, UI_BUTTON, None, Some("IW_Done"));
        unsafe {
            (*window).set_m_pFirstNode(done);
            (*done).set_ParentWindow(window);
        }
        let manager = leak::<CXWndManager>();
        set_windows(|w| w.window_manager = manager.ptr());
        let game = Game { window, done, manager };
        game.show(true);
        game
    }

    /// Whether the game has the window: found by name and listed.
    fn show(&self, has: bool) {
        let list: &'static mut [*mut CXWnd] = Box::leak(Box::new([self.window]));
        let list_ptr = list.as_mut_ptr();
        self.manager.get().pWindows().set_m_array(if has { list_ptr } else { std::ptr::null_mut() });
        self.manager.get().pWindows().set_m_length(i32::from(has));
        let window = self.window as usize;
        fake::with(|w| {
            if has {
                w.by_name.insert("inventory".into(), window);
            } else {
                w.by_name.remove("inventory");
            }
        });
    }
}

thread_local! {
    static SUBCLASS: Cell<Option<SubclassId>> = const { Cell::new(None) };
}

fn subclass_id() -> SubclassId {
    SUBCLASS.with(Cell::get).unwrap()
}

/// A runtime whose plugin subclasses "Inventory" with `hooks` when it starts.
fn subclassing(hooks: impl Fn() -> WindowHooks<Recorder> + 'static) -> &'static Runtime<Recorder> {
    let rt = runtime();
    set_hook(&ON_NEW, move |_, mq| {
        SUBCLASS.with(|s| s.set(Some(mq.subclass_window("Inventory", hooks()))));
    });
    rt.initialize();
    rt
}

/// Logs notifications, handling them if `handle`.
fn logging(handle: bool) -> WindowHooks<Recorder> {
    WindowHooks::new().on_notification(move |plugin: &mut Recorder, _, event| {
        let sender = event.sender().map_or("none".into(), |s| s.screen_id().unwrap_or_default());
        plugin.log.push(format!("event {} {sender} {:?} {}", event.window().name().unwrap_or_default(), event.notification(), event.data()));
        if handle { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
    })
}

#[test]
fn a_window_the_game_has_is_subclassed_at_once() {
    let game = Game::new();
    let rt = subclassing(|| logging(false));
    assert_eq!(fake::log(), ["subclass InventoryWindow 0x1"], "only notifications asked for");
    assert!(!fake::send(game.window, game.done, XWM_LCLICK, 4 as *mut c_void), "passed on to the window");
    assert_eq!(log_of(rt)[1..], ["event Inventory IW_Done Some(LeftClick) 4"]);
    set_hook(&ON_PULSE, |plugin, mq| {
        let on = mq.subclassed_window(subclass_id()).and_then(|w| w.name());
        plugin.log.push(format!("on {on:?}"));
    });
    rt.pulse();
    assert_eq!(log_of(rt).last().unwrap(), "on Some(\"Inventory\")");
}

#[test]
fn a_handled_notification_doesnt_reach_the_window() {
    let game = Game::new();
    let rt = subclassing(|| logging(true));
    assert!(fake::send(game.window, std::ptr::null_mut(), XWM_LCLICK, std::ptr::null_mut()));
    assert_eq!(log_of(rt)[1..], ["event Inventory none Some(LeftClick) 0"]);
}

#[test]
fn a_window_not_there_yet_is_subclassed_once_it_is() {
    let game = Game::new();
    game.show(false);
    let rt = subclassing(|| logging(false));
    rt.pulse();
    assert_eq!(fake::log(), Vec::<String>::new());
    game.show(true);
    rt.pulse();
    assert_eq!(fake::log(), ["subclass InventoryWindow 0x1"], "on the next pulse");
    rt.pulse();
    rt.reload_ui();
    assert_eq!(fake::log(), Vec::<String>::new(), "once");
}

#[test]
fn the_subclass_comes_off_after_on_clean_ui_and_back_on_when_the_ui_loads() {
    let game = Game::new();
    let rt = subclassing(|| logging(false));
    fake::log();
    set_hook(&ON_UI, |plugin, mq| plugin.log.push(format!("subclassed {}", mq.subclassed_window(subclass_id()).is_some())));
    rt.clean_ui();
    assert_eq!(fake::log(), ["unsubclass InventoryWindow"]);
    assert_eq!(log_of(rt)[1..], ["clean_ui", "subclassed true"], "still on in on_clean_ui");
    rt.reload_ui();
    assert_eq!(fake::log(), ["subclass InventoryWindow 0x1"]);
    assert_eq!(log_of(rt)[3..], ["reload_ui", "subclassed true"], "back on before on_reload_ui");
    game.show(true);
    rt.set_game_state(mq_emu_sys::constants::GAMESTATE_INGAME);
    assert_eq!(fake::log(), Vec::<String>::new());
}

#[test]
fn unloading_takes_the_subclass_off() {
    Game::new();
    let rt = subclassing(|| logging(false));
    fake::log();
    rt.shutdown();
    assert_eq!(fake::log(), ["unsubclass InventoryWindow"]);
}

#[test]
fn a_window_the_game_deleted_isnt_touched() {
    let game = Game::new();
    let rt = subclassing(|| logging(false));
    fake::log();
    game.show(false);
    rt.shutdown();
    assert_eq!(fake::log(), ["unsubclass (gone)"], "taken off without reading the window");
}

#[test]
fn a_subclass_can_be_removed_from_its_own_hook() {
    let game = Game::new();
    let rt = subclassing(|| {
        WindowHooks::new().on_notification(|plugin: &mut Recorder, mq, _| {
            plugin.log.push(format!("removed {} {}", mq.remove_subclass(subclass_id()), mq.remove_subclass(subclass_id())));
            plugin.log.push(format!("on {}", mq.subclassed_window(subclass_id()).is_some()));
            ControlFlow::Continue(())
        })
    });
    fake::log();
    fake::send(game.window, game.done, XWM_LCLICK, std::ptr::null_mut());
    assert_eq!(fake::log(), ["unsubclass InventoryWindow"], "at once");
    assert_eq!(log_of(rt)[1..], ["removed true false", "on false"]);
    rt.pulse();
    rt.reload_ui();
    rt.shutdown();
    assert_eq!(fake::log(), Vec::<String>::new(), "gone for good");
}

#[test]
fn a_notification_during_a_callback_comes_after_it_and_isnt_handled() {
    let game = Game::new();
    let rt = subclassing(|| logging(true));
    let (window, done) = (game.window as usize, game.done as usize);
    set_hook(&ON_PULSE, move |plugin, _| {
        let handled = fake::send(window as *mut CXWnd, done as *mut CXWnd, XWM_LCLICK, std::ptr::null_mut());
        plugin.log.push(format!("handled {handled}"));
    });
    rt.pulse();
    assert_eq!(log_of(rt)[1..], ["pulse", "handled false", "event Inventory IW_Done Some(LeftClick) 0"]);
}

#[test]
fn one_window_is_subclassed_once() {
    Game::new();
    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| {
        let first = mq.subclass_window("Inventory", logging(false));
        let second = mq.subclass_window("inventory", WindowHooks::new().on_draw(|_, _, _| {}));
        plugin.log.push(format!("{} {}", mq.subclassed_window(first).is_some(), mq.subclassed_window(second).is_some()));
        mq.remove_subclass(first);
    });
    rt.initialize();
    assert_eq!(log_of(rt)[1], "true false", "the first has it");
    assert_eq!(fake::log(), ["subclass InventoryWindow 0x1", "unsubclass InventoryWindow"]);
    rt.pulse();
    assert_eq!(fake::log(), ["subclass InventoryWindow 0x2"], "the second once the first is gone");
}

#[test]
fn masks_follow_the_hooks() {
    let hooks: [(WindowHooks<Recorder>, u32); 4] = [
        (WindowHooks::new(), 0),
        (WindowHooks::new().on_draw(|_, _, _| {}), 0x2),
        (WindowHooks::new().on_mouse(|_, _, _| ControlFlow::Continue(())), 0xfc),
        (logging(false).on_draw(|_, _, _| {}).on_mouse(|_, _, _| ControlFlow::Continue(())), 0xff),
    ];
    for (hooks, mask) in hooks {
        assert_eq!(hooks.mask(), mask, "{hooks:?}");
    }
}

#[test]
fn the_mouse_reaches_its_hook() {
    let game = Game::new();
    let rt = subclassing(|| {
        WindowHooks::new().on_mouse(|plugin: &mut Recorder, _, event| {
            plugin.log.push(format!("{:?} {:?} {}", event.action(), event.position(), event.flags()));
            if event.action() == MouseAction::LeftDown { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
        })
    });
    let at = |kind, x, y, scroll| mq_emu_sys::windows::mqemu_window_event { x, y, flags: 2, scroll, ..fake::event(kind) };
    assert!(fake::pass(game.window, at(HOOK_LBUTTON_DOWN, 10, 20, 0)));
    assert!(!fake::pass(game.window, at(HOOK_MOUSE_MOVE, 11, 21, 0)));
    assert!(!fake::pass(game.window, at(HOOK_WHEEL, 12, 22, -120)));
    assert!(!fake::send(game.window, game.done, XWM_LCLICK, std::ptr::null_mut()), "notifications weren't asked for");
    assert_eq!(log_of(rt)[1..], ["LeftDown (10, 20) 2", "Move (11, 21) 2", "Wheel(-120) (12, 22) 2"]);
}

/// The window's client area on screen, in the fake game.
fn client_area(window: *mut CXWnd, rect: mqemu_rect) {
    fake::with(|w| w.client_rects.insert(window as usize, rect));
}

#[test]
fn drawing_is_clipped_to_the_window() {
    let game = Game::new();
    client_area(game.window, mqemu_rect { left: 100, top: 200, right: 300, bottom: 400 });
    let rt = subclassing(|| {
        WindowHooks::new().on_draw(|plugin: &mut Recorder, _, painter| {
            let clip = painter.clip_rect();
            painter.fill_rect(clip, 0x8000_ff00);
            let line = painter.line_height(2).unwrap();
            let width = painter.text_width(2, "Hail").unwrap();
            plugin.log.push(format!("{line} {width} {:?} {:?}", painter.line_height(10), painter.text_width(10, "x")));
            let text = WindowRect::new(clip.left + 5, clip.top + 5, width, line);
            assert!(painter.text(2, "Hail", text, 0xffff_ffff, EqTextFlags::NO_WRAP));
            assert!(!painter.text(10, "Hail", text, 0xffff_ffff, EqTextFlags::NONE), "no such font");
            painter.clipped(WindowRect::new(250, 0, 100, 1000)).fill_rect(clip, 0xff00_0000);
        })
    });
    fake::log();
    fake::draw(game.window);
    assert_eq!(log_of(rt)[1..], ["12 12 None None"]);
    assert_eq!(
        fake::log(),
        [
            "fill_rect 100,200,300,400 0x8000ff00 clip 100,200,300,400",
            "draw_text 2 \"Hail\" 105,205,117,217 0xffffffff 1 clip 100,200,300,400",
            "fill_rect 100,200,300,400 0xff000000 clip 250,200,300,400",
        ]
    );
}

#[test]
fn a_window_without_a_client_area_draws_nowhere() {
    let game = Game::new();
    let rt = subclassing(|| {
        WindowHooks::new().on_draw(|plugin: &mut Recorder, _, painter| plugin.log.push(format!("{:?}", painter.clip_rect())))
    });
    fake::draw(game.window);
    assert_eq!(log_of(rt)[1..], [format!("{:?}", WindowRect::default())]);
}

#[test]
fn drawing_during_a_callback_is_dropped() {
    // By the time the callback ends, the frame is drawn.
    let game = Game::new();
    let rt = subclassing(|| WindowHooks::new().on_draw(|plugin: &mut Recorder, _, _| plugin.log.push("drew".into())));
    let window = game.window as usize;
    set_hook(&ON_PULSE, move |_, _| fake::draw(window as *mut CXWnd));
    rt.pulse();
    assert_eq!(log_of(rt)[1..], ["pulse"]);
    assert_eq!(rt.stats().dropped, 1);
    fake::draw(game.window);
    assert_eq!(log_of(rt)[2..], ["drew"]);
}

#[test]
fn a_panicking_hook_is_caught_and_passes_on() {
    let game = Game::new();
    let rt = subclassing(|| WindowHooks::new().on_notification(|_: &mut Recorder, _, _| panic!("bad hook")));
    assert!(!fake::send(game.window, game.done, XWM_LCLICK, std::ptr::null_mut()), "the window still gets it");
    assert!(chat_lines().iter().any(|line| line.contains("a window handler") && line.contains("bad hook")), "{:?}", chat_lines());
    assert_eq!(rt.stats().panics, 1);
}
