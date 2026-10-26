//! Custom widgets: items laid out with an ID, their button behavior through
//! simulated clicks, splitters dragged, and the render helpers drawing.

use super::*;

/// A 100 x 20 item at the top of the test window, and how the mouse treats it.
fn draw_item(ui: &mut Ui, flags: ButtonFlags) -> (Item, ButtonState, Option<ItemId>) {
    test_window(ui, |ui| {
        let item = ui.add_item("custom", [100.0, 20.0]).unwrap();
        let state = ui.button_behavior(&item, flags);
        (item, state, ui.active_id())
    })
    .unwrap()
}

fn center(item: &Item) -> [f32; 2] {
    [(item.min[0] + item.max[0]) / 2.0, (item.min[1] + item.max[1]) / 2.0]
}

#[test]
fn an_item_takes_its_space_at_the_cursor_and_is_the_last_item() {
    let _imgui = lock();
    let (start, item, after, last, clipped) = frame(|ui| {
        test_window(ui, |ui| {
            let start = ui.cursor_screen_pos();
            let item = ui.add_item("custom", [100.0, 20.0]).unwrap();
            let last = last_item_center();
            let after = ui.cursor_screen_pos();
            ui.set_cursor_screen_pos([start[0], start[1] + 5000.0]);
            let clipped = ui.add_item("far below", [100.0, 20.0]);
            (start, item, after, last, clipped)
        })
        .unwrap()
    })
    .unwrap();
    assert_eq!((item.min, item.max), (start, [start[0] + 100.0, start[1] + 20.0]));
    assert!(after[1] > item.max[1], "the cursor moved below it");
    assert_eq!(last, center(&item));
    assert_eq!(clipped, None, "out of view");
    assert_eq!(frame(|ui| test_window(ui, |ui| ui.item_id("custom"))).unwrap(), Some(item.id));
    assert_eq!(take_error(), None);
}

#[test]
fn button_behavior_reports_hover_hold_and_press() {
    let _imgui = lock();
    let (item, ..) = frame(|ui| draw_item(ui, ButtonFlags::NONE)).unwrap();
    let at = center(&item);
    let states: Vec<_> = [false, true, true, false].into_iter().map(|down| frame_at(at, down, |ui| draw_item(ui, ButtonFlags::NONE)).unwrap()).collect();
    assert!(states[0].1.hovered && !states[0].1.held);
    assert!(states[2].1.held && !states[2].1.pressed, "held, not pressed until released");
    assert_eq!(states[2].2, Some(item.id), "the active item while held");
    assert!(states[3].1.pressed, "pressed on release");
    assert_eq!(states.iter().filter(|(_, s, _)| s.pressed).count(), 1);

    // PRESSED_ON_CLICK: as the mouse goes down.
    let pressed: Vec<bool> =
        [false, true, false].into_iter().map(|down| frame_at(at, down, |ui| draw_item(ui, ButtonFlags::PRESSED_ON_CLICK)).unwrap().1.pressed).collect();
    assert_eq!(pressed, [false, true, false]);
    assert_eq!(take_error(), None);
}

#[test]
fn clearing_the_active_id_lets_go() {
    let _imgui = lock();
    let (item, ..) = frame(|ui| draw_item(ui, ButtonFlags::NONE)).unwrap();
    let at = center(&item);
    frame_at(at, false, |ui| draw_item(ui, ButtonFlags::NONE)).unwrap();
    frame_at(at, true, |ui| draw_item(ui, ButtonFlags::NONE)).unwrap();
    let active = frame_at(at, true, |ui| {
        let (.., active) = draw_item(ui, ButtonFlags::NONE);
        ui.clear_active_id();
        (active, ui.active_id())
    })
    .unwrap();
    assert_eq!(active, (Some(item.id), None));
    let (_, state, _) = frame_at(at, false, |ui| draw_item(ui, ButtonFlags::NONE)).unwrap();
    assert!(!state.pressed, "released after letting go isn't a press");
    assert_eq!(take_error(), None);
}

#[test]
fn dragging_a_splitter_moves_size_between_its_sides() {
    let _imgui = lock();
    let mut sizes = [200.0, 200.0];
    let draw = |ui: &mut Ui, sizes: &mut [f32; 2]| {
        test_window(ui, |ui| {
            let [x, y] = ui.cursor_screen_pos();
            let bar = ([x + sizes[0], y], [x + sizes[0] + 6.0, y + 100.0]);
            let dragging = ui.splitter("##split", Split::Columns, bar.0, bar.1, sizes, [50.0, 50.0]);
            ui.dummy([400.0, 100.0]);
            (bar, dragging)
        })
        .unwrap()
    };
    let ((min, max), _) = frame(|ui| draw(ui, &mut sizes)).unwrap();
    let grab = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    frame_at(grab, false, |ui| draw(ui, &mut sizes)).unwrap();
    frame_at(grab, true, |ui| draw(ui, &mut sizes)).unwrap();
    let (_, dragging) = frame_at([grab[0] + 30.0, grab[1]], true, |ui| draw(ui, &mut sizes)).unwrap();
    assert!(dragging);
    assert_eq!(sizes, [230.0, 170.0]);
    frame_at([grab[0] + 400.0, grab[1]], true, |ui| draw(ui, &mut sizes)).unwrap();
    assert_eq!(sizes, [350.0, 50.0], "the right side keeps its minimum");
    frame_at([grab[0] + 400.0, grab[1]], false, |ui| draw(ui, &mut sizes)).unwrap();
    assert_eq!(take_error(), None);
}

#[test]
fn the_ex_buttons_take_private_flags() {
    let _imgui = lock();
    let draw = |ui: &mut Ui| {
        test_window(ui, |ui| {
            let button = ui.button_with("Down", [80.0, 0.0], ButtonFlags::PRESSED_ON_CLICK);
            let button_at = last_item_center();
            ui.same_line();
            ui.vertical_separator();
            ui.same_line();
            let arrow = ui.arrow_button_with("arrow", Dir::Left, [30.0, 30.0], ButtonFlags::NONE);
            (button, button_at, arrow, last_item_center())
        })
        .unwrap()
    };
    let (_, button_at, _, arrow_at) = frame(draw).unwrap();
    frame_at(button_at, false, draw).unwrap();
    assert!(frame_at(button_at, true, draw).unwrap().0, "pressed as the mouse goes down");
    frame_at(button_at, false, draw).unwrap();
    let clicks: Vec<bool> = [false, true, false].into_iter().map(|down| frame_at(arrow_at, down, draw).unwrap().2).collect();
    assert_eq!(clicks, [false, false, true]);
    assert_eq!(take_error(), None);
}

#[test]
fn the_render_helpers_draw() {
    let _imgui = lock();
    let drew = frame(|ui| {
        test_window(ui, |ui| {
            let counted = |f: &dyn Fn(&Ui)| {
                let before = unsafe { mqemu_imgui_test_window_vertices() };
                f(ui);
                let after = unsafe { mqemu_imgui_test_window_vertices() };
                after - before
            };
            let [x, y] = ui.cursor_screen_pos();
            let (min, max) = ([x, y], [x + 200.0, y + 30.0]);
            let frame = counted(&|ui| ui.render_frame(min, max, [0.2, 0.3, 0.4, 1.0], true, 4.0));
            let text = counted(&|ui| ui.render_text_clipped(min, max, "abcd", [0.5, 0.5]));
            let hidden = counted(&|ui| ui.render_text_clipped(min, max, "abcd##efgh", [0.5, 0.5]));
            let shapes = counted(&|ui| {
                let list = ui.draw_list();
                list.add_arrow([x, y], [1.0; 4], Dir::Down, 1.0);
                list.add_bullet([x + 40.0, y + 10.0], [1.0; 4]);
                list.add_check_mark([x + 60.0, y], [1.0; 4], 14.0);
            });
            ui.dummy([200.0, 30.0]);
            (frame, text, hidden, shapes)
        })
        .unwrap()
    })
    .unwrap();
    let (frame, text, hidden, shapes) = drew;
    assert!(frame > 0 && text > 0 && shapes > 0, "{drew:?}");
    assert_eq!(text, hidden, "text after ## isn't drawn");
    assert_eq!(take_error(), None);
}

#[test]
#[should_panic(expected = "a separator's thickness must be more than 0")]
fn a_separator_without_thickness_panics() {
    let _imgui = lock();
    let _ = frame(|ui| ui.separator_with(false, 0.0));
}

#[test]
fn misuse_the_safe_api_prevents_is_refused_by_the_shim() {
    let _imgui = lock();
    // A release imgui.dll only asserts on these: ID 0 would become the
    // active ID of nothing, a bad axis or direction picks garbage, and a
    // separator with no axis or thickness draws nothing sensible.
    let errors = frame(|ui| {
        test_window(ui, |ui| unsafe {
            let api = &sys::API;
            let list = ui.draw_list().raw_list();
            let (mut a, mut b) = (false, false);
            let (mut s1, mut s2) = (10.0, 10.0);
            let mut errors = Vec::new();
            (api.mqemu_imgui_button_behavior)(0.0, 0.0, 10.0, 10.0, 0, 0, &mut a, &mut b);
            errors.push(take_error());
            (api.mqemu_imgui_splitter)(0.0, 0.0, 10.0, 10.0, 0, sys::ImGuiAxis_X, &mut s1, &mut s2, 1.0, 1.0, 0.0, 0.0);
            errors.push(take_error());
            (api.mqemu_imgui_splitter)(0.0, 0.0, 10.0, 10.0, 7, sys::ImGuiAxis_None, &mut s1, &mut s2, 1.0, 1.0, 0.0, 0.0);
            errors.push(take_error());
            (api.mqemu_imgui_render_arrow)(list, 0.0, 0.0, 0, sys::ImGuiDir_None, 1.0);
            errors.push(take_error());
            let (p, n) = s("arrow");
            (api.mqemu_imgui_arrow_button_ex)(p, n, 4, 10.0, 10.0, 0);
            errors.push(take_error());
            (api.mqemu_imgui_separator_ex)(false, 0.0);
            errors.push(take_error());
            errors
        })
        .unwrap()
    })
    .unwrap();
    let errors: Vec<Option<&str>> = errors.iter().map(Option::as_deref).collect();
    assert_eq!(errors, [
        Some("ButtonBehavior() needs an item ID other than 0"),
        Some("a splitter needs an item ID other than 0"),
        Some("a splitter's axis is ImGuiAxis_X or ImGuiAxis_Y"),
        Some("not a direction (ImGuiDir_Left to ImGuiDir_Down)"),
        Some("not a direction (ImGuiDir_Left to ImGuiDir_Down)"),
        Some("a separator's thickness must be more than 0"),
    ]);
}
