//! Drives the ImGui wrappers (shim/imgui.cpp) against MacroQuest's real
//! imgui.dll in a headless context: widgets draw, clicks register, and ImGui's
//! C++ exceptions are caught instead of crossing into Rust.

use core::ffi::c_char;
use std::sync::Mutex;

use mq_emu_sys::imgui::testing::*;
use mq_emu_sys::imgui::*;

/// ImGui's context is process-global; tests take turns.
static IMGUI: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    let guard = IMGUI.lock().unwrap_or_else(|e| e.into_inner());
    unsafe { mqemu_imgui_test_init() };
    take_error(); // Start clean.
    guard
}

fn frame(mouse: (f32, f32), down: bool, draw: impl FnOnce()) -> Result<(), String> {
    assert!(unsafe { mqemu_imgui_test_new_frame(mouse.0, mouse.1, down) }, "NewFrame threw: {:?}", take_error());
    draw();
    let mut error = [0 as c_char; 512];
    if unsafe { mqemu_imgui_test_end_frame(error.as_mut_ptr(), error.len()) } {
        Ok(())
    } else {
        Err(text(&error))
    }
}

fn text(buffer: &[c_char]) -> String {
    let bytes: Vec<u8> = buffer.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn take_error() -> Option<String> {
    let mut error = [0 as c_char; 512];
    unsafe { mqemu_imgui_take_error(error.as_mut_ptr(), error.len()) }.then(|| text(&error))
}

fn s(text: &str) -> (*const c_char, usize) {
    (text.as_ptr().cast(), text.len())
}

fn begin(name: &str) -> bool {
    let (p, n) = s(name);
    unsafe {
        mqemu_imgui_set_next_window_pos(10.0, 10.0, ImGuiCond_Always, 0.0, 0.0);
        mqemu_imgui_set_next_window_size(400.0, 600.0, ImGuiCond_Always);
        mqemu_imgui_begin(p, n, core::ptr::null_mut(), ImGuiWindowFlags_NoSavedSettings)
    }
}

fn last_item_center() -> (f32, f32) {
    let (mut x0, mut y0, mut x1, mut y1) = (0.0, 0.0, 0.0, 0.0);
    unsafe { mqemu_imgui_test_last_item_rect(&mut x0, &mut y0, &mut x1, &mut y1) };
    assert!(x1 > x0 && y1 > y0, "the item has no area: ({x0}, {y0})-({x1}, {y1})");
    ((x0 + x1) / 2.0, (y0 + y1) / 2.0)
}

#[test]
fn every_widget_draws_in_a_balanced_frame() {
    let _imgui = lock();
    let result = frame((0.0, 0.0), false, || unsafe {
        assert!(begin("widgets"));
        let (p, n) = s("hello ${Me.Name} 100% %s %n"); // Not a format: nothing to misread.
        mqemu_imgui_text(p, n);
        let (l, ln) = s("label");
        mqemu_imgui_label_text(l, ln, p, n);
        mqemu_imgui_separator();
        mqemu_imgui_separator_text(l, ln);
        mqemu_imgui_bullet();
        mqemu_imgui_same_line(0.0, -1.0);
        mqemu_imgui_text(p, n);
        mqemu_imgui_button(l, ln, 0.0, 0.0);
        mqemu_imgui_small_button(l, ln);
        let mut checked = true;
        mqemu_imgui_checkbox(l, ln, &mut checked);
        mqemu_imgui_radio_button(l, ln, true);
        mqemu_imgui_selectable(l, ln, false, 0, 0.0, 0.0);
        let (f, fl) = s("%.2f");
        let mut value = 0.5f32;
        mqemu_imgui_slider_float(l, ln, &mut value, 0.0, 1.0, f, fl, 0);
        mqemu_imgui_drag_float(l, ln, &mut value, 0.1, 0.0, 1.0, f, fl, 0);
        mqemu_imgui_input_float(l, ln, &mut value, 0.0, 0.0, f, fl, 0);
        let (d, dl) = s("%d");
        let mut number = 3;
        mqemu_imgui_slider_int(l, ln, &mut number, 0, 10, d, dl, 0);
        mqemu_imgui_drag_int(l, ln, &mut number, 1.0, 0, 10, d, dl, 0);
        mqemu_imgui_input_int(l, ln, &mut number, 1, 10, 0);
        let mut color = [1.0f32, 0.5, 0.25, 1.0];
        mqemu_imgui_color_edit4(l, ln, color.as_mut_ptr(), 0);
        mqemu_imgui_progress_bar(0.3, -f32::MIN_POSITIVE, 0.0, p, n);
        mqemu_imgui_progress_bar(0.3, 100.0, 0.0, core::ptr::null(), NO_STRING);
        let values = [1.0f32, 3.0, 2.0];
        mqemu_imgui_plot_lines(l, ln, values.as_ptr(), 3, 0, core::ptr::null(), NO_STRING, f32::MAX, f32::MAX, 0.0, 40.0, false);
        mqemu_imgui_plot_lines(l, ln, values.as_ptr(), 3, 0, p, n, 0.0, 5.0, 0.0, 40.0, true);
        mqemu_imgui_set_next_item_open(true, ImGuiCond_Always);
        if mqemu_imgui_tree_node(l, ln, 0) {
            mqemu_imgui_text(p, n);
            mqemu_imgui_tree_pop();
        }
        mqemu_imgui_collapsing_header(l, ln, ImGuiTreeNodeFlags_DefaultOpen);
        let (t, tn) = s("table");
        if mqemu_imgui_begin_table(t, tn, 2, ImGuiTableFlags_Borders, 0.0, 0.0, 0.0) {
            mqemu_imgui_table_setup_column(l, ln, 0, 0.0);
            mqemu_imgui_table_setup_column(l, ln, 0, 0.0);
            mqemu_imgui_table_headers_row();
            mqemu_imgui_table_next_row(0, 0.0);
            mqemu_imgui_table_next_column();
            mqemu_imgui_text(p, n);
            mqemu_imgui_table_set_column_index(1);
            mqemu_imgui_text(p, n);
            mqemu_imgui_end_table();
        }
        if mqemu_imgui_begin_tab_bar(t, tn, 0) {
            if mqemu_imgui_begin_tab_item(l, ln, core::ptr::null_mut(), 0) {
                mqemu_imgui_text(p, n);
                mqemu_imgui_end_tab_item();
            }
            mqemu_imgui_end_tab_bar();
        }
        mqemu_imgui_push_id_int(7);
        mqemu_imgui_push_id_str(p, n);
        mqemu_imgui_push_style_color(ImGuiCol_Text, 1.0, 0.0, 0.0, 1.0);
        mqemu_imgui_push_style_var_float(ImGuiStyleVar_Alpha, 0.5);
        mqemu_imgui_push_style_var_vec2(ImGuiStyleVar_ItemSpacing, 2.0, 2.0);
        mqemu_imgui_push_text_wrap_pos(0.0);
        mqemu_imgui_begin_disabled(true);
        mqemu_imgui_indent(0.0);
        mqemu_imgui_begin_group();
        mqemu_imgui_text(p, n);
        mqemu_imgui_end_group();
        mqemu_imgui_unindent(0.0);
        mqemu_imgui_end_disabled();
        mqemu_imgui_pop_text_wrap_pos();
        mqemu_imgui_pop_style_var(2);
        mqemu_imgui_pop_style_color(1);
        mqemu_imgui_pop_id();
        mqemu_imgui_pop_id();
        if mqemu_imgui_begin_child(t, tn, 0.0, 50.0, ImGuiChildFlags_Borders, 0) {
            mqemu_imgui_text(p, n);
        }
        mqemu_imgui_end_child();
        let (mut w, mut h) = (0.0, 0.0);
        mqemu_imgui_calc_text_size(p, n, &mut w, &mut h);
        assert!(w > 0.0 && h > 0.0, "text has a size: {w}x{h}");
        mqemu_imgui_content_region_avail(&mut w, &mut h);
        assert!(w > 0.0, "the window has room: {w}");
        mqemu_imgui_end();
    });
    assert_eq!(result, Ok(()));
    assert_eq!(take_error(), None);
}

#[test]
fn a_button_click_is_reported_on_release() {
    let _imgui = lock();
    let (label, len) = s("Click me");
    let draw = || unsafe {
        begin("click");
        let clicked = mqemu_imgui_button(label, len, 0.0, 0.0);
        let center = last_item_center();
        mqemu_imgui_end();
        (clicked, center)
    };
    let mut seen = None;
    frame((0.0, 0.0), false, || seen = Some(draw())).unwrap();
    let (_, center) = seen.unwrap();
    let mut clicks = Vec::new();
    // Hover, press, release, then move away.
    for (mouse, down) in [(center, false), (center, true), (center, false), ((0.0, 0.0), false)] {
        frame(mouse, down, || clicks.push(draw().0)).unwrap();
    }
    assert_eq!(clicks, [false, false, true, false]);
}

#[test]
fn a_checkbox_click_flips_its_value() {
    let _imgui = lock();
    let (label, len) = s("Check");
    let mut value = false;
    let draw = |value: &mut bool| unsafe {
        begin("checkbox");
        let changed = mqemu_imgui_checkbox(label, len, value);
        let center = last_item_center();
        mqemu_imgui_end();
        (changed, center)
    };
    let mut target = (0.0, 0.0);
    frame((0.0, 0.0), false, || target = draw(&mut value).1).unwrap();
    let mut changes = Vec::new();
    for (mouse, down) in [(target, false), (target, true), (target, false)] {
        frame(mouse, down, || changes.push(draw(&mut value).0)).unwrap();
    }
    assert_eq!(changes, [false, false, true]);
    assert!(value, "the click checked it");
}

/// What a user error does in game: ImGui throws, and the shim catches it.
#[test]
fn calling_end_too_often_is_caught_and_reported() {
    let _imgui = lock();
    let result = frame((0.0, 0.0), false, || unsafe {
        begin("extra end");
        mqemu_imgui_end();
        mqemu_imgui_end(); // One too many: ImGui throws ImGuiException.
    });
    assert_eq!(take_error().as_deref(), Some("Calling End() too many times!"));
    // The error was recovered from where it happened; the frame still ends.
    assert_eq!(result, Ok(()));
    assert_eq!(take_error(), None, "taking the error clears it");
}

#[test]
fn only_the_first_error_is_kept() {
    let _imgui = lock();
    frame((0.0, 0.0), false, || unsafe {
        mqemu_imgui_pop_id(); // "Calling PopID() too many times!"
        mqemu_imgui_pop_style_color(1); // "Calling PopStyleColor() too many times!"
    })
    .unwrap();
    assert_eq!(take_error().as_deref(), Some("Calling PopID() too many times!"));
}

/// The positive control for tests of the safe API's balancing: a window left
/// open is what makes MacroQuest's ImGui::Render throw and pause every
/// plugin's ImGui.
#[test]
fn a_missing_end_makes_the_frame_fail() {
    let _imgui = lock();
    let result = frame((0.0, 0.0), false, || {
        begin("never ended");
    });
    let error = result.expect_err("Render should throw for a window left open");
    assert!(error.contains("Missing End()"), "{error}");
    // A fresh context afterwards: the next frame is fine.
    assert_eq!(frame((0.0, 0.0), false, || unsafe {
        begin("fine");
        mqemu_imgui_end();
    }), Ok(()));
}

#[test]
fn window_and_id_stack_depths_are_observable() {
    let _imgui = lock();
    frame((0.0, 0.0), false, || unsafe {
        let outside = mqemu_imgui_test_window_depth();
        begin("depth");
        assert_eq!(mqemu_imgui_test_window_depth(), outside + 1);
        let ids = mqemu_imgui_test_id_depth();
        mqemu_imgui_push_id_int(1);
        assert_eq!(mqemu_imgui_test_id_depth(), ids + 1);
        mqemu_imgui_pop_id();
        mqemu_imgui_end();
        assert_eq!(mqemu_imgui_test_window_depth(), outside);
    })
    .unwrap();
}
