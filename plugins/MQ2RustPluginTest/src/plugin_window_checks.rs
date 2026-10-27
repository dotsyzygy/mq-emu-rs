//! Selftest checks for a plugin's own EQ UI window: the plugin adds
//! `MQUI_RustTestWnd.xml` and its "RustTestWnd" window when it loads; the
//! UI is reloaded once if it hasn't read the file yet. The window's button,
//! edit box and links reach the plugin's handler, from mq-emu and from
//! MacroQuest's `/notify`; it draws on itself each frame it's shown;
//! removed, it's gone, and added again it's back.

use std::ops::ControlFlow;
use std::time::Duration;

use mq_emu::imgui::EqTextFlags;
use mq_emu::{Click, GameState, Mq, Task, WindowEvent, WindowHooks, WindowRect};

use crate::RustTest;
use crate::checks::{Check, Report, same_text};

/// The plugin's UI file (deployed to resources/uifiles/default) and the
/// screen piece in it.
pub const XML_FILE: &str = "MQUI_RustTestWnd.xml";
pub const SCREEN: &str = "RustTestWnd";
const BUTTON: &str = "RTW_Button";
const INPUT: &str = "RTW_Input";
/// How long the UI may take to reload. The game freezes while it does (no
/// pulses), and the wait checks for the screen piece before its deadline,
/// so a freeze longer than this still passes once the game is back; this
/// only bounds waiting for a reload that never happens.
const RELOAD_TIMEOUT: Duration = Duration::from_secs(60);
/// How long a notification may take to arrive.
const NOTIFY_TIMEOUT: Duration = Duration::from_secs(2);

/// What the window's handler records: the notification, the control that
/// sent it, and its data or link.
pub fn describe(event: &WindowEvent<'_>) -> String {
    let what = event.notification().map_or_else(|| event.message().to_string(), |n| n.name().to_owned());
    let sender = event.sender().and_then(|s| s.screen_id()).unwrap_or_else(|| "-".into());
    match event.link() {
        Some(link) => format!("{what} {sender} {link}"),
        None => format!("{what} {sender} {}", event.data()),
    }
}

/// Adds the plugin's window, recording what its handler gets, and drawing
/// a line of text under its controls each frame.
pub fn add(mq: &Mq<RustTest>) -> mq_emu::PluginWindowId {
    let hooks = WindowHooks::new()
        .on_notification(|plugin: &mut RustTest, _, event| {
            plugin.window_events.push(describe(event));
            ControlFlow::Continue(())
        })
        .on_draw(|plugin, _, painter| {
            let clip = painter.clip_rect();
            let height = painter.line_height(FONT);
            let line = WindowRect { left: clip.left + 4, top: clip.bottom - height.unwrap_or(12) - 4, right: clip.right - 4, bottom: clip.bottom - 4 };
            painter.fill_rect(line, 0x8000_0000);
            let drew = painter.text(FONT, "drawn by mq-emu", line, 0xff80_ff80, EqTextFlags::NO_WRAP);
            plugin.own_draws += 1;
            plugin.own_drawing = Some((drew, height, painter.text_width(FONT, "drawn by mq-emu")));
        });
    mq.add_window_with(SCREEN, hooks)
}

/// The EQ font style the window's text is drawn in.
const FONT: u8 = 2;

/// Waits until the handler has recorded an event starting with `event`.
async fn got(task: &Task<RustTest>, event: &str) -> bool {
    let seen = |plugin: &RustTest| plugin.window_events.iter().any(|e| e.starts_with(event));
    for _ in 0..(NOTIFY_TIMEOUT.as_millis() / 50) {
        if task.with_plugin(|plugin, _| seen(plugin)) {
            return true;
        }
        task.delay(Duration::from_millis(50)).await;
    }
    task.with_plugin(|plugin, _| seen(plugin))
}

fn events(task: &Task<RustTest>) -> Vec<String> {
    task.with_plugin(|plugin, _| plugin.window_events.clone())
}

pub async fn run(task: &Task<RustTest>, report: &mut Report) {
    let in_game = task.with(|mq| {
        let in_game = mq.game_state() == GameState::InGame;
        if !in_game {
            report.print(mq, "the plugin's own window", Check::Skip("needs to be in game".into()));
        }
        in_game
    });
    if !in_game {
        return;
    }
    let loaded = task.with(|mq| mq.is_screen_piece_loaded(SCREEN));
    if !loaded {
        // The plugin added its file when it loaded; the UI reads it when it
        // next loads. Loaded before entering the world, it's read then, with
        // no reload. The reload runs from MacroQuest's command queue, outside
        // the plugin's pulse, so the freeze isn't a slow pulse of ours.
        task.with(|mq| {
            report.note(mq, &format!(
                "the UI hasn't read {XML_FILE} yet: reloading it, which freezes the game for a while. \
                 To skip this, load the plugin before entering the world"
            ));
            mq.command("/reloadui");
        });
        let started = std::time::Instant::now();
        let reloaded = task.wait_until_timeout(|mq| mq.is_screen_piece_loaded(SCREEN), RELOAD_TIMEOUT).await;
        let took = started.elapsed();
        task.with(|mq| report.note(mq, &format!("the UI reload took {:.1} s", took.as_secs_f32())));
        if reloaded {
            // Let the game finish setting the new UI up.
            task.delay(Duration::from_secs(1)).await;
        }
    }
    let ready = task.with_plugin(|plugin, mq| {
        let loaded = mq.is_screen_piece_loaded(SCREEN);
        report.print(mq, &format!("add_xml_file({XML_FILE:?}) puts its screen piece in the UI (IsScreenPieceLoaded)"), Check::that(loaded, || {
            format!("not loaded; is {XML_FILE} in MacroQuest's resources/uifiles/default (scripts/deploy-plugin copies it)?")
        }));
        let window = plugin.plugin_window.and_then(|id| mq.plugin_window(id));
        report.print(mq, "add_window's window exists once the UI has its screen piece", Check::that(window.is_some(), || "no window".into()));
        let Some(window) = window else { return false };
        window.open();
        report.print(mq, &format!("it's a top-level window ${{Window[{SCREEN}]}} finds, shown"), same_text("TRUE", mq.parse(&format!("${{Window[{SCREEN}].Open}}"))));
        report.print(mq, "made from its screen piece, with its controls", Check::that(window.child(BUTTON).is_some() && window.child(INPUT).is_some(), || {
            format!("children: {:?}", window.children().filter_map(|c| c.screen_id()).collect::<Vec<_>>())
        }));
        plugin.window_events.clear();
        true
    });
    if !ready {
        return;
    }

    // It draws on itself each frame it's shown.
    let mut draws = 0;
    for _ in 0..(NOTIFY_TIMEOUT.as_millis() / 50) {
        draws = task.with_plugin(|plugin, _| plugin.own_draws);
        if draws >= 3 {
            break;
        }
        task.delay(Duration::from_millis(50)).await;
    }
    task.with_plugin(|plugin, mq| {
        report.print(mq, "add_window_with's on_draw runs each frame the window is shown", Check::that(draws >= 3, || format!("{draws} draws")));
        let drawing = plugin.own_drawing;
        report.print(mq, &format!("the painter draws text in EQ font style {FONT}, and measures it"), Check::that(
            matches!(drawing, Some((true, Some(height), Some(width))) if height > 0 && width > 0),
            || format!("(drew, line height, width): {drawing:?}"),
        ));
    });

    // A click on its button, as a player's would.
    task.with(|mq| mq.window(&format!("{SCREEN}/{BUTTON}")).map(|button| button.click(Click::Left)));
    let clicked = got(task, &format!("leftmouse {BUTTON} ")).await;
    task.with(|mq| report.print(mq, "clicking its button reaches the handler (leftmouse from RTW_Button)", Check::that(clicked, || format!("got {:?}", events(task)))));

    // MacroQuest's own /notify (a notification it sends, not a click).
    task.with(|mq| mq.command(format!("/notify {SCREEN} {BUTTON} enter")));
    let notified = got(task, &format!("enter {BUTTON} 0")).await;
    task.with(|mq| report.print(mq, &format!("/notify {SCREEN} {BUTTON} enter reaches it too"), Check::that(notified, || format!("got {:?}", events(task)))));

    // Typing into its edit box tells the window.
    task.with(|mq| mq.window(&format!("{SCREEN}/{INPUT}")).map(|input| input.set_text("typed")));
    let typed = got(task, &format!("newvalue {INPUT} 0")).await;
    task.with(|mq| report.print(mq, "set_text in its edit box reaches it as newvalue", Check::that(typed, || format!("got {:?}", events(task)))));

    // A link, with its text.
    task.with(|mq| {
        let window = mq.window(SCREEN);
        window.map(|w| w.notify_link(w.child(BUTTON), "rusttest-link"))
    });
    let linked = got(task, &format!("27 {BUTTON} rusttest-link")).await;
    task.with(|mq| report.print(mq, "notify_link reaches it with the link's text", Check::that(linked, || format!("got {:?}", events(task)))));

    // Removed: hidden now, deleted before the next pulse.
    let removed = task.with_plugin(|plugin, mq| plugin.plugin_window.take().is_some_and(|id| mq.remove_window(id)));
    task.next_pulse().await;
    task.with(|mq| {
        report.print(mq, &format!("remove_window deletes it (${{Window[{SCREEN}]}} is NULL)"), Check::that(
            removed && mq.window(SCREEN).is_none() && mq.parse(&format!("${{Window[{SCREEN}]}}")) == "NULL",
            || format!("removed: {removed}; ${{Window[{SCREEN}]}} is {}", mq.parse(&format!("${{Window[{SCREEN}]}}"))),
        ));
    });
    // Added again: made right away, as the UI has the screen piece.
    task.with_plugin(|plugin, mq| {
        let id = add(mq);
        plugin.plugin_window = Some(id);
        let window = mq.plugin_window(id);
        report.print(mq, "add_window again makes it right away", same_text("TRUE", mq.parse(&format!("${{Window[{SCREEN}].Open}}"))));
        // Out of the way until the next selftest.
        if let Some(window) = window {
            window.close();
        }
    });
}
