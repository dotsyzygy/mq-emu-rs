//! A plugin's own windows and UI XML files against the fake game: made when
//! the UI has their screen pieces, deleted before the UI is cleaned up and
//! when the plugin unloads, and their notifications delivered like any
//! other callback.

use std::ffi::c_void;

use mq_emu_sys::constants::{GAMESTATE_CHARSELECT, GAMESTATE_INGAME, UI_BUTTON, UI_SCREEN, XWM_LCLICK, XWM_LINK, XWM_NEWVALUE};
use mq_emu_sys::layouts::CXWnd;

use super::*;
use crate::items::tests::Fake;
use crate::windows::fake;
use crate::{Notification, PluginWindowId, WindowEvent};

fn leak<T>() -> &'static Fake<T> {
    Box::leak(Box::new(Fake::new()))
}

/// A screen piece "TestWnd" the fake UI makes as `window`, holding `button`
/// ("TW_Button"); `other` is a control of some other window. Not loaded yet.
struct Screen {
    window: *mut CXWnd,
    button: *mut CXWnd,
    other: *mut CXWnd,
}

fn screen() -> Screen {
    fake::reset();
    let [window, button, other] = [leak::<CXWnd>(), leak(), leak()].map(Fake::ptr);
    fake::add(window, UI_SCREEN, Some("TestWnd"), None);
    fake::add(button, UI_BUTTON, None, Some("TW_Button"));
    fake::add(other, UI_BUTTON, None, Some("Elsewhere"));
    unsafe {
        (*window).set_m_pFirstNode(button);
        (*button).set_ParentWindow(window);
    }
    fake::with(|w| {
        w.plugin_windows.insert("testwnd".into(), window as usize);
    });
    Screen { window, button, other }
}

fn load_screen_piece() {
    fake::with(|w| w.screen_pieces.push("testwnd".into()));
}

fn describe(event: &WindowEvent<'_>) -> String {
    let sender = event.sender().map_or("none".into(), |s| s.screen_id().unwrap_or_default());
    format!("event {sender} {:?} {} {} {:?}", event.notification(), event.message(), event.data(), event.link())
}

thread_local! {
    static WINDOW_ID: Cell<Option<PluginWindowId>> = const { Cell::new(None) };
}

/// A runtime whose plugin adds the "TestWnd" window, logging its events.
fn with_window() -> &'static Runtime<Recorder> {
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        let id = mq.add_window("TestWnd", |plugin: &mut Recorder, _mq, event| plugin.log.push(describe(event)));
        WINDOW_ID.with(|w| w.set(Some(id)));
    });
    rt.initialize();
    rt
}

fn window_id() -> PluginWindowId {
    WINDOW_ID.with(Cell::get).unwrap()
}

/// Logs whether the plugin's window exists, in on_clean_ui and the like.
fn log_window_in_ui_callbacks() {
    set_hook(&ON_UI, |plugin, mq| {
        let open = mq.plugin_window(window_id()).map(|w| w.name().unwrap_or_default());
        plugin.log.push(format!("window {open:?}"));
    });
}

#[test]
fn a_window_is_made_when_the_ui_has_its_screen_piece() {
    let screen = screen();
    load_screen_piece();
    let rt = with_window();
    assert_eq!(fake::log(), ["create testwnd"], "made right away");
    let window = screen.window as usize;
    set_hook(&ON_PULSE, move |plugin, mq| {
        let found = mq.plugin_window(window_id()).map(|w| std::ptr::from_ref(w.raw()) as usize == window);
        plugin.log.push(format!("{found:?}"));
    });
    rt.pulse();
    assert_eq!(log_of(rt)[2], "Some(true)");
}

#[test]
fn a_window_waits_for_the_ui_to_load_its_screen_piece() {
    screen();
    let rt = with_window();
    log_window_in_ui_callbacks();
    assert_eq!(fake::log(), Vec::<String>::new(), "no screen piece, no window");
    rt.reload_ui();
    assert_eq!(fake::log(), Vec::<String>::new(), "still none");
    load_screen_piece();
    rt.reload_ui();
    assert_eq!(fake::log(), ["create testwnd"]);
    assert_eq!(log_of(rt)[1..], ["reload_ui", "window None", "reload_ui", "window Some(\"TestWnd\")"], "made before on_reload_ui");
}

#[test]
fn entering_the_world_makes_the_window_too() {
    screen();
    let rt = with_window();
    load_screen_piece();
    rt.set_game_state(GAMESTATE_CHARSELECT);
    assert_eq!(fake::log(), Vec::<String>::new());
    rt.set_game_state(GAMESTATE_INGAME);
    assert_eq!(fake::log(), ["create testwnd"]);
    rt.set_game_state(GAMESTATE_INGAME);
    assert_eq!(fake::log(), Vec::<String>::new(), "made once");
}

#[test]
fn the_window_is_deleted_after_on_clean_ui_and_made_again_on_reload() {
    screen();
    load_screen_piece();
    let rt = with_window();
    log_window_in_ui_callbacks();
    fake::log();
    rt.clean_ui();
    assert_eq!(fake::log(), ["destroy TestWnd"]);
    rt.clean_ui();
    assert_eq!(fake::log(), Vec::<String>::new(), "deleted once");
    rt.reload_ui();
    assert_eq!(fake::log(), ["create testwnd"]);
    assert_eq!(
        log_of(rt)[1..],
        ["clean_ui", "window Some(\"TestWnd\")", "clean_ui", "window None", "reload_ui", "window Some(\"TestWnd\")"],
        "the plugin still has it in on_clean_ui"
    );
}

#[test]
fn unloading_deletes_the_window_and_takes_out_the_xml_files() {
    screen();
    load_screen_piece();
    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| {
        mq.add_window("TestWnd", |_: &mut Recorder, _, _| {});
        let added = [mq.add_xml_file("MQUI_Test.xml"), mq.add_xml_file("mqui_test.XML"), mq.add_xml_file("")];
        plugin.log.push(format!("{added:?} {}", mq.remove_xml_file("MQUI_Other.xml")));
    });
    rt.initialize();
    assert_eq!(log_of(rt)[1], "[true, true, false] false", "only files it added are taken out");
    assert_eq!(fake::log(), ["create testwnd", "add_xml_file MQUI_Test.xml", "add_xml_file mqui_test.XML"]);
    rt.shutdown();
    assert_eq!(fake::log(), ["destroy TestWnd", "remove_xml_file MQUI_Test.xml"], "each file once");
}

#[test]
fn a_removed_xml_file_stays_out() {
    fake::reset();
    let rt = in_pulse(|plugin, mq| {
        mq.add_xml_file("MQUI_Test.xml");
        plugin.log.push(format!("{} {}", mq.remove_xml_file("MQUI_TEST.xml"), mq.remove_xml_file("MQUI_Test.xml")));
    });
    assert_eq!(log_of(rt)[2], "true false");
    rt.shutdown();
    assert_eq!(fake::log(), ["add_xml_file MQUI_Test.xml", "remove_xml_file MQUI_Test.xml"]);
}

#[test]
fn screen_pieces_are_looked_up_in_the_loaded_ui() {
    screen();
    load_screen_piece();
    let rt = in_pulse(|plugin, mq| plugin.log.push(format!("{} {}", mq.is_screen_piece_loaded("TESTWND"), mq.is_screen_piece_loaded("Other"))));
    assert_eq!(log_of(rt)[2], "true false");
}

#[test]
fn notifications_reach_the_handler_with_their_sender() {
    let screen = screen();
    load_screen_piece();
    let rt = with_window();
    fake::send(screen.window, screen.button, XWM_LCLICK, 3 as *mut c_void);
    fake::send(screen.window, std::ptr::null_mut(), XWM_NEWVALUE, -2i32 as usize as *mut c_void);
    fake::send(screen.window, screen.window, 1000, std::ptr::null_mut());
    fake::send(screen.window, screen.other, XWM_LCLICK, std::ptr::null_mut());
    let link = std::ffi::CString::new("0012345678").unwrap();
    fake::send(screen.window, screen.button, XWM_LINK, link.as_ptr().cast_mut().cast());
    assert_eq!(
        log_of(rt)[1..],
        [
            "event TW_Button Some(LeftClick) 1 3 None",
            "event none Some(NewValue) 14 -2 None",
            "event  None 1000 0 None",
            "event none Some(LeftClick) 1 0 None",
            "event TW_Button None 27 0 Some(\"0012345678\")",
        ],
        "the window itself sends as itself (no screen ID); a control of another window isn't passed on"
    );
    assert_eq!(Notification::from_raw(XWM_LCLICK), Some(Notification::LeftClick));
}

#[test]
fn a_notification_during_a_callback_is_delivered_after_it() {
    let screen = screen();
    load_screen_piece();
    let rt = with_window();
    let (window, button) = (screen.window as usize, screen.button as usize);
    set_hook(&ON_PULSE, move |plugin, _| {
        fake::send(window as *mut CXWnd, button as *mut CXWnd, XWM_LCLICK, std::ptr::null_mut());
        plugin.log.push("pulse done".into());
    });
    rt.pulse();
    assert_eq!(log_of(rt)[1..], ["pulse", "pulse done", "event TW_Button Some(LeftClick) 1 0 None"]);
}

#[test]
fn a_window_removed_in_its_own_handler_is_deleted_on_the_next_pulse() {
    let screen = screen();
    load_screen_piece();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        let id = mq.add_window("TestWnd", |plugin: &mut Recorder, mq, event| {
            plugin.log.push(describe(event));
            plugin.log.push(format!("removed {} {}", mq.remove_window(window_id()), mq.remove_window(window_id())));
            plugin.log.push(format!("found {}", mq.plugin_window(window_id()).is_some()));
        });
        WINDOW_ID.with(|w| w.set(Some(id)));
    });
    rt.initialize();
    fake::log();
    fake::send(screen.window, screen.button, XWM_LCLICK, std::ptr::null_mut());
    assert_eq!(fake::log(), ["show TestWnd false"], "hidden, not deleted while its notification runs");
    fake::send(screen.window, screen.button, XWM_LCLICK, std::ptr::null_mut());
    rt.pulse();
    assert_eq!(fake::log(), ["destroy TestWnd"]);
    assert_eq!(log_of(rt)[1..], ["event TW_Button Some(LeftClick) 1 0 None", "removed true false", "found false", "pulse"], "nothing after removal");
    rt.reload_ui();
    rt.shutdown();
    assert_eq!(fake::log(), Vec::<String>::new(), "gone for good");
}

#[test]
fn a_removed_window_waiting_for_deletion_goes_with_the_ui() {
    screen();
    load_screen_piece();
    let rt = with_window();
    set_hook(&ON_PULSE, |_, mq| assert!(mq.remove_window(window_id())));
    rt.pulse();
    assert_eq!(fake::log(), ["create testwnd", "show TestWnd false"]);
    set_hook(&ON_PULSE, |_, _| {});
    rt.clean_ui();
    assert_eq!(fake::log(), ["destroy TestWnd"]);
    rt.pulse();
    rt.shutdown();
    assert_eq!(fake::log(), Vec::<String>::new());
}

#[test]
fn a_panicking_window_handler_is_caught_and_reported() {
    let screen = screen();
    load_screen_piece();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.add_window("TestWnd", |_: &mut Recorder, _, _| panic!("bad click"));
    });
    rt.initialize();
    fake::send(screen.window, screen.button, XWM_LCLICK, std::ptr::null_mut());
    assert!(chat_lines().iter().any(|line| line.contains("a window handler") && line.contains("bad click")), "{:?}", chat_lines());
    assert_eq!(rt.stats().panics, 1);
}
