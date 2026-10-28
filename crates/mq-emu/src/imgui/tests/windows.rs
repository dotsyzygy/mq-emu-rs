//! Any window by name or ID: what it is, focus and display order, moving,
//! sizing and collapsing it, and scrolling it to a place, an item or a
//! rectangle.

use super::*;

/// A window that places itself only the first time, so others can move it.
fn free_window<R>(ui: &mut Ui, name: &str, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    ui.window(name).position([600.0, 100.0], Cond::FirstUseEver).size([200.0, 150.0], Cond::FirstUseEver).flags(WindowFlags::NO_SAVED_SETTINGS).show(f)
}

/// A 200 x 200 window without a title bar over 2000 pixels of content, a
/// button halfway down; `f` runs right after the button.
fn tall_window<R>(ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> R {
    ui.window("tall")
        .position([10.0, 10.0], Cond::Always)
        .size([200.0, 200.0], Cond::Always)
        .flags(WindowFlags::NO_SAVED_SETTINGS | WindowFlags::NO_TITLE_BAR)
        .show(|ui| {
            ui.dummy([100.0, 1000.0]);
            ui.button("middle");
            let result = f(ui);
            ui.dummy([100.0, 1000.0]);
            result
        })
        .unwrap()
}

fn last_item_rect() -> ([f32; 2], [f32; 2]) {
    let (mut x0, mut y0, mut x1, mut y1) = (0.0, 0.0, 0.0, 0.0);
    unsafe { mqemu_imgui_test_last_item_rect(&mut x0, &mut y0, &mut x1, &mut y1) };
    ([x0, y0], [x1, y1])
}

fn scroll_y(ui: &mut Ui) -> f32 {
    ui.find_window("tall").unwrap().info().scroll[1]
}

#[test]
fn windows_are_found_by_name_and_described() {
    let _imgui = lock();
    let (test, child, current, missing) = frame(|ui| {
        let (child, current) = test_window(ui, |ui| {
            let child = ui.child("inner").size([100.0, 100.0]).show(|ui| ui.current_window().info()).unwrap();
            (child, ui.current_window().id())
        })
        .unwrap();
        let test = ui.find_window("test").unwrap().info();
        (test, child, current, ui.find_window("no window has this name").is_none())
    })
    .unwrap();
    assert!(missing);
    assert_eq!((test.name.as_str(), test.kind, test.parent), ("test", WindowKind::TopLevel, None));
    assert_eq!(test.id, current, "the current window inside it");
    assert_eq!(test.id, frame(|ui| ui.window_id("test")).unwrap(), "its ID is its name's");
    assert_eq!((test.pos, test.size), ([10.0, 10.0], [500.0, 600.0]));
    assert!(test.flags.contains(WindowFlags::NO_SAVED_SETTINGS));
    assert_eq!(test.root, test.id);
    assert!(test.focus_order.is_some());
    assert!(child.name.starts_with("test/inner"), "{}", child.name);
    assert_eq!((child.kind, child.parent, child.root, child.focus_order), (WindowKind::Child, Some(test.id), test.id, None));
    assert_eq!(frame(|ui| ui.find_window(&child.name).map(|w| w.id())).unwrap(), Some(child.id));
    assert_eq!(take_error(), None);
}

#[test]
fn popups_menus_and_tooltips_have_their_kinds() {
    let _imgui = lock();
    let kinds = frame(|ui| {
        test_window(ui, |ui| {
            ui.open_popup("popup");
            let popup = ui.popup("popup", |ui| ui.current_window().info().kind);
            let tooltip = ui.tooltip(|ui| ui.current_window().info().kind);
            (popup, tooltip)
        })
        .unwrap()
    })
    .unwrap();
    assert_eq!(kinds, (Some(WindowKind::Popup), Some(WindowKind::Tooltip)));
    frame(|ui| test_window(ui, |ui| ui.popup("popup", |ui| ui.close_current_popup()))).unwrap();
    assert_eq!(take_error(), None);
}

#[test]
fn windows_list_back_to_front() {
    let _imgui = lock();
    let windows = frame(|ui| {
        free_window(ui, "first", |_| ());
        free_window(ui, "second", |_| ());
        ui.windows()
    })
    .unwrap();
    for (index, window) in windows.iter().enumerate() {
        assert_eq!(window.display_index, index, "{}", window.name);
    }
    let names: Vec<&str> = windows.iter().map(|w| w.name.as_str()).collect();
    assert!(names.contains(&"first") && names.contains(&"second"), "{names:?}");
    assert_eq!(take_error(), None);
}

#[test]
fn bringing_a_window_to_the_front_back_or_behind_another_reorders_them() {
    let _imgui = lock();
    let draw = |ui: &mut Ui| {
        free_window(ui, "first", |_| ());
        free_window(ui, "second", |_| ());
        [ui.window_id("first"), ui.window_id("second")]
    };
    let [first, second] = frame(draw).unwrap();
    let above = |ui: &mut Ui, a: WindowId, b: WindowId| ui.window_by_id(a).unwrap().is_above(b);
    let order = frame(|ui| {
        draw(ui);
        ui.window_by_id(first).unwrap().bring_to_front();
        let front = (above(ui, first, second), ui.windows().last().map(|w| w.id));
        ui.window_by_id(first).unwrap().bring_to_back();
        let back = (above(ui, first, second), ui.window_info(first).unwrap().display_index);
        ui.window_by_id(second).unwrap().bring_behind(first);
        let behind = above(ui, first, second);
        (front, back, behind)
    })
    .unwrap();
    assert_eq!(order, ((true, Some(first)), (false, 0), true));
    assert!(!frame(|ui| above(ui, first, WindowId(0x1234_5678))).unwrap(), "no such window");
    assert_eq!(take_error(), None);
}

#[test]
fn focusing_a_window_brings_it_to_the_front_and_clearing_focus_leaves_none() {
    let _imgui = lock();
    let draw = |ui: &mut Ui| {
        free_window(ui, "first", |_| ());
        free_window(ui, "second", |_| ());
        [ui.window_id("first"), ui.window_id("second")]
    };
    let [first, second] = frame(draw).unwrap();
    for (window, other) in [(first, second), (second, first)] {
        frame(|ui| {
            draw(ui);
            ui.window_by_id(window).unwrap().focus(FocusRequestFlags::NONE);
        })
        .unwrap();
        let (focused, info, above) = frame(|ui| {
            draw(ui);
            (ui.focused_window(), ui.window_info(window).unwrap(), ui.window_by_id(window).unwrap().is_above(other))
        })
        .unwrap();
        assert_eq!(focused, Some(window));
        assert!(info.focused && above);
        assert!(info.focus_order > frame(|ui| ui.window_info(other).unwrap().focus_order).unwrap(), "last in focus order");
    }
    let focused = frame(|ui| {
        ui.clear_window_focus();
        ui.focused_window()
    })
    .unwrap();
    assert_eq!(focused, None);
    assert_eq!(take_error(), None);
}

#[test]
fn focusing_another_window_from_inside_a_popup_ends_the_frame_cleanly() {
    let _imgui = lock();
    // Focusing closes popups over other windows, here the one being drawn.
    let other = frame(|ui| {
        free_window(ui, "first", |_| ());
        ui.window_id("first")
    })
    .unwrap();
    let ran = frame(|ui| {
        free_window(ui, "first", |_| ());
        test_window(ui, |ui| {
            ui.open_popup("popup");
            ui.popup("popup", |ui| ui.window_by_id(other).unwrap().focus(FocusRequestFlags::NONE)).is_some()
        })
        .unwrap()
    });
    assert_eq!(ran, Ok(true));
    assert_eq!(take_error(), None);
    let open = frame(|ui| test_window(ui, |ui| ui.popup("popup", |_| ()).is_some()).unwrap()).unwrap();
    assert!(!open, "the popup closed");
}

#[test]
fn another_window_is_moved_sized_and_collapsed() {
    let _imgui = lock();
    frame(|ui| free_window(ui, "moved", |_| ())).unwrap();
    let info = frame(|ui| {
        let mut moved = ui.find_window("moved").unwrap();
        moved.set_pos([300.0, 200.0], Cond::Always);
        moved.set_size([250.0, 120.0], Cond::Always);
        free_window(ui, "moved", |_| ());
        ui.find_window("moved").unwrap().info()
    })
    .unwrap();
    assert_eq!((info.pos, info.size), ([300.0, 200.0], [250.0, 120.0]));
    let collapsed = frame(|ui| {
        ui.find_window("moved").unwrap().set_collapsed(true, Cond::Always);
        let drew = free_window(ui, "moved", |_| ()).is_some();
        (drew, ui.find_window("moved").unwrap().info().collapsed)
    })
    .unwrap();
    assert_eq!(collapsed, (false, true));
    frame(|ui| {
        ui.find_window("moved").unwrap().set_collapsed(false, Cond::Always);
        free_window(ui, "moved", |_| ());
    })
    .unwrap();
    assert_eq!(take_error(), None);
}

#[test]
fn another_window_is_scrolled_to_a_place() {
    let _imgui = lock();
    frame(|ui| tall_window(ui, |_| ())).unwrap();
    frame(|ui| {
        tall_window(ui, |_| ());
        ui.find_window("tall").unwrap().set_scroll_y(500.0);
    })
    .unwrap();
    let info = frame(|ui| {
        tall_window(ui, |_| ());
        ui.find_window("tall").unwrap().info()
    })
    .unwrap();
    assert_eq!(info.scroll, [0.0, 500.0]);
    assert!(info.scroll_max[1] > 1500.0 && info.content_size[1] > 2000.0, "{info:?}");
    // From a position: 300 down from its top goes to the top of its view.
    frame(|ui| {
        tall_window(ui, |_| ());
        ui.find_window("tall").unwrap().set_scroll_from_pos_y(300.0, 0.0);
    })
    .unwrap();
    assert_eq!(frame(|ui| tall_window(ui, scroll_y)).unwrap(), 800.0);
    assert_eq!(take_error(), None);
}

#[test]
fn the_current_window_scrolls_to_the_cursor_and_an_item() {
    let _imgui = lock();
    let button = |ui: &mut Ui| tall_window(ui, |ui| (last_item_rect(), ui.current_window().info()));
    // The button's line at the top of the view, then the bottom.
    for (ratio, top) in [(0.0, true), (1.0, false)] {
        frame(|ui| tall_window(ui, |ui| ui.set_scroll_here_y(ratio))).unwrap();
        let ((min, max), info) = frame(button).unwrap();
        let (edge, at) = if top { (info.pos[1], min[1]) } else { (info.pos[1] + info.size[1], max[1]) };
        assert!(info.scroll[1] > 0.0 && (at - edge).abs() < 10.0, "ratio {ratio}: the button at {min:?}..{max:?} in {info:?}");
    }

    frame(|ui| tall_window(ui, |ui| ui.current_window().set_scroll_y(0.0))).unwrap();
    // The button, centered.
    frame(|ui| tall_window(ui, |ui| ui.scroll_to_item(ScrollFlags::ALWAYS_CENTER_Y))).unwrap();
    let ((min, max), info) = frame(button).unwrap();
    let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let middle = info.pos[1] + info.size[1] / 2.0;
    assert!((center[1] - middle).abs() < 2.0, "the button at {center:?}, the middle at {middle}");
    assert_eq!(take_error(), None);
}

#[test]
fn a_window_scrolls_to_a_rectangle() {
    let _imgui = lock();
    frame(|ui| tall_window(ui, |ui| ui.current_window().set_scroll_y(0.0))).unwrap();
    frame(|ui| tall_window(ui, |_| ())).unwrap();
    let (in_view, below) = frame(|ui| {
        tall_window(ui, |_| ());
        let mut tall = ui.find_window("tall").unwrap();
        let in_view = tall.scroll_to_rect([20.0, 20.0], [60.0, 40.0], ScrollFlags::NONE);
        let below = tall.scroll_to_rect([20.0, 1510.0], [60.0, 1530.0], ScrollFlags::KEEP_VISIBLE_EDGE_Y);
        (in_view, below)
    })
    .unwrap();
    assert_eq!(in_view, [0.0, 0.0], "already in view");
    assert!(below[1] > 1300.0 && below[0] == 0.0, "{below:?}");
    let scroll = frame(|ui| tall_window(ui, scroll_y)).unwrap();
    assert_eq!(scroll, below[1], "it scrolled as far as it said");
    assert_eq!(take_error(), None);
}

#[test]
#[should_panic(expected = "a scroll ratio is 0 to 1")]
fn a_scroll_ratio_past_1_panics() {
    let _imgui = lock();
    let _ = frame(|ui| ui.set_scroll_here_y(1.5));
}

#[test]
#[should_panic(expected = "ImGui scrolls one way per axis")]
fn two_ways_to_scroll_an_axis_panic() {
    let _imgui = lock();
    let _ = frame(|ui| ui.scroll_to_item(ScrollFlags::KEEP_VISIBLE_EDGE_Y | ScrollFlags::ALWAYS_CENTER_Y));
}

#[test]
fn misuse_the_safe_api_prevents_is_refused_by_the_shim() {
    let _imgui = lock();
    // A release imgui.dll would dereference a null window, and only asserts
    // on combined conditions, ratios past 0 to 1 and two ways to scroll an
    // axis (which pick one silently or scroll past the content).
    let errors = frame(|ui| {
        test_window(ui, |ui| unsafe {
            let api = &sys::API;
            let test = ui.window_id("test").0;
            let (mut dx, mut dy) = (0.0, 0.0);
            let mut errors = Vec::new();
            (api.mqemu_imgui_set_window_pos_by_id)(0, 0.0, 0.0, 0);
            errors.push(take_error());
            (api.mqemu_imgui_set_window_size_by_id)(0x1234_5678, 10.0, 10.0, 0);
            errors.push(take_error());
            (api.mqemu_imgui_bring_window_to_front)(0);
            errors.push(take_error());
            (api.mqemu_imgui_set_window_pos_by_id)(test, 10.0, 10.0, sys::ImGuiCond_Once | sys::ImGuiCond_Appearing);
            errors.push(take_error());
            (api.mqemu_imgui_set_window_scroll_from_pos)(test, sys::ImGuiAxis_Y, 0.0, 1.5);
            errors.push(take_error());
            (api.mqemu_imgui_set_window_scroll)(test, sys::ImGuiAxis_None, 0.0);
            errors.push(take_error());
            (api.mqemu_imgui_scroll_to_rect)(
                test,
                0.0,
                0.0,
                1.0,
                1.0,
                sys::ImGuiScrollFlags_KeepVisibleEdgeX | sys::ImGuiScrollFlags_AlwaysCenterX,
                &mut dx,
                &mut dy,
            );
            errors.push(take_error());
            errors
        })
        .unwrap()
    })
    .unwrap();
    let errors: Vec<Option<&str>> = errors.iter().map(Option::as_deref).collect();
    assert_eq!(errors, [
        Some("no such window"),
        Some("no such window"),
        Some("no such window"),
        Some("one ImGuiCond_ at a time"),
        Some("a scroll ratio is 0 to 1"),
        Some("an axis is ImGuiAxis_X or ImGuiAxis_Y"),
        Some("one ImGuiScrollFlags_ behavior per axis"),
    ]);
}
