//! The ImGui callback: delivered every frame, never queued, and errors the
//! shim caught reported once.

use super::*;

/// What the fake shim says it caught during the next frame.
fn imgui_error_next_frame(error: &str) {
    IMGUI_ERROR.with(|e| *e.borrow_mut() = Some(error.to_owned()));
}

#[test]
fn frames_reach_on_update_imgui() {
    let rt = runtime();
    rt.initialize();
    rt.update_imgui();
    rt.update_imgui();
    assert_eq!(log_of(rt), ["new", "imgui", "imgui"]);
    assert_eq!(rt.stats().imgui_errors, 0);
}

#[test]
fn an_imgui_error_is_shown_once_then_counted() {
    let rt = runtime();
    rt.initialize();
    imgui_error_next_frame("Calling End() too many times!");
    rt.update_imgui();
    assert_eq!(rt.stats().imgui_errors, 1);
    assert_eq!(chat_lines(), ["[test] ImGui error while drawing: Calling End() too many times!"]);

    // The same mistake every frame doesn't flood chat...
    imgui_error_next_frame("Calling End() too many times!");
    rt.update_imgui();
    rt.update_imgui(); // (a frame without an error)
    assert_eq!(rt.stats().imgui_errors, 2);
    assert_eq!(chat_lines().len(), 1);

    // ...but shows up in the next rate-limited warning.
    advance_clock(crate::WARNING_INTERVAL);
    rt.pulse();
    assert_eq!(chat_lines().last().map(String::as_str), Some("[test] warning: 1 ImGui error"));
}

#[test]
fn an_imgui_error_after_a_panic_is_still_reported() {
    let rt = runtime();
    rt.initialize();
    set_hook(&ON_IMGUI, |_, _| panic!("drawing went wrong"));
    imgui_error_next_frame("Missing End()");
    rt.update_imgui();
    let stats = rt.stats();
    assert_eq!((stats.panics, stats.imgui_errors), (1, 1));
    assert_eq!(chat_lines(), [
        "[test] panicked in on_update_imgui: drawing went wrong",
        "[test] ImGui error while drawing: Missing End()",
    ]);
}

#[test]
fn a_frame_arriving_mid_callback_is_dropped_not_queued() {
    let rt = runtime();
    rt.initialize();
    set_hook(&ON_PULSE, move |_, _| rt.update_imgui());
    rt.pulse();
    // Queued, it would have run after on_pulse, outside MacroQuest's frame.
    assert_eq!(log_of(rt), ["new", "pulse"]);
    assert_eq!((rt.stats().queued, rt.stats().dropped), (0, 1));

    // A pulse arriving mid-callback is still queued, as before.
    set_hook(&ON_PULSE, |_, _| {});
    set_hook(&ON_IMGUI, move |_, _| rt.pulse());
    rt.update_imgui();
    assert_eq!(log_of(rt), ["new", "pulse", "imgui", "pulse"]);
}
