//! Drag and drop, docking, the cursor and mouse, and invisible buttons.

use super::*;
use crate::imgui::fake;

/// One frame of [`draw_dnd`].
struct DndFrame {
    source: [f32; 2],
    target: [f32; 2],
    /// What the target accepted: the data, and whether it was a preview or a delivery.
    accepted: Option<(Vec<u8>, bool, bool)>,
    /// The payload being dragged: kind and data.
    dragging: Option<(String, Vec<u8>)>,
}

/// A window with a drag source and a drop target, side by side.
fn draw_dnd(ui: &mut Ui, accept_flags: DragDropFlags) -> DndFrame {
    test_window(ui, |ui| {
        ui.button_sized("source", [100.0, 40.0]);
        let source = last_item_center();
        ui.drag_drop_source(|drag| {
            drag.set_payload("ITEM", b"hello");
            drag.text("dragging hello");
        });
        ui.button_sized("target", [100.0, 40.0]);
        let target = last_item_center();
        let accepted = ui
            .drag_drop_target(|drop| drop.accept_with("ITEM", accept_flags).map(|p| (p.data().to_vec(), p.is_preview(), p.is_delivery())))
            .flatten();
        let dragging = ui.drag_drop_payload().map(|(kind, p)| (kind, p.data().to_vec()));
        DndFrame { source, target, accepted, dragging }
    })
    .unwrap()
}

#[test]
fn a_payload_dragged_onto_a_target_is_delivered() {
    let _imgui = lock();
    let DndFrame { source, target, .. } = frame(|ui| draw_dnd(ui, DragDropFlags::NONE)).unwrap();
    let below = [source[0], source[1] + 15.0];
    // Hover, press, drag away, over the target, release there.
    let steps = [(source, false), (source, true), (below, true), (target, true), (target, true), (target, false)];
    let mut seen = Vec::new();
    for (mouse, down) in steps {
        let DndFrame { accepted, dragging, .. } = frame_at(mouse, down, |ui| draw_dnd(ui, DragDropFlags::NONE)).unwrap();
        seen.push((accepted, dragging));
    }
    let payload = Some(("ITEM".to_owned(), b"hello".to_vec()));
    assert_eq!(seen[2].1, payload, "dragging once the mouse moved");
    assert_eq!(seen[3].0, None, "not accepted before it's dropped");
    assert_eq!(seen[5].0, Some((b"hello".to_vec(), true, true)), "delivered on release");
    assert_eq!(take_error(), None);
}

#[test]
fn a_target_can_see_a_payload_before_it_is_dropped() {
    let _imgui = lock();
    let DndFrame { source, target, .. } = frame(|ui| draw_dnd(ui, DragDropFlags::ACCEPT_BEFORE_DELIVERY)).unwrap();
    let below = [source[0], source[1] + 15.0];
    let mut previews = Vec::new();
    for (mouse, down) in [(source, false), (source, true), (below, true), (target, true), (target, true)] {
        let DndFrame { accepted, .. } = frame_at(mouse, down, |ui| draw_dnd(ui, DragDropFlags::ACCEPT_BEFORE_DELIVERY)).unwrap();
        previews.push(accepted);
    }
    assert_eq!(previews[4], Some((b"hello".to_vec(), true, false)));
    // Let go elsewhere, so the drag doesn't carry into other tests' frames.
    frame_at([1000.0, 700.0], false, |_| ()).unwrap();
    assert_eq!(take_error(), None);
}

#[test]
fn payload_kinds_are_checked_before_imgui_sees_them() {
    let _imgui = lock();
    let mut ui = Ui::new(&fake::APIS);
    let long = "x".repeat(MAX_PAYLOAD_KIND + 1);
    for kind in ["", "_imgui", "nul\0", long.as_str()] {
        let mut source = DragSource::for_test(&mut ui);
        assert!(catch_unwind(AssertUnwindSafe(|| source.set_payload(kind, b""))).is_err(), "{kind:?}");
        let mut target = DropTarget::for_test(&mut ui);
        assert!(catch_unwind(AssertUnwindSafe(|| target.accept(kind).is_some())).is_err(), "{kind:?}");
    }
    let fine = "x".repeat(MAX_PAYLOAD_KIND);
    assert!(catch_unwind(AssertUnwindSafe(|| super::super::dnd::check_kind(&fine))).is_ok());
}

#[test]
fn an_invalid_payload_kind_is_an_imgui_error() {
    // The shim's own check, under payload_kinds_are_checked_before_imgui_sees_them.
    let _imgui = lock();
    frame(|_| unsafe {
        let kind = "_reserved";
        (sys::API.mqemu_imgui_set_drag_drop_payload)(kind.as_ptr().cast(), kind.len(), std::ptr::null(), 0, 0);
    })
    .unwrap();
    assert_eq!(take_error().as_deref(), Some("drag and drop payload types are 1 to 32 bytes and don't start with '_'"));
}

#[test]
fn windows_dock_into_a_dock_space() {
    let _imgui = lock();
    let draw = |ui: &mut Ui| {
        assert!(ui.is_docking_enabled(), "the test context turns docking on, as MacroQuest does");
        let id = ui.dock_id("space");
        let host = ui
            .window("host")
            .position([600.0, 10.0], Cond::Always)
            .size([400.0, 300.0], Cond::Always)
            .flags(WindowFlags::NO_SAVED_SETTINGS)
            .show(|ui| ui.dock_space(id, [0.0, 0.0], DockNodeFlags::NONE));
        let docked = ui
            .window("docked")
            .dock(id, Cond::Always)
            .flags(WindowFlags::NO_SAVED_SETTINGS)
            .show(|ui| (ui.is_window_docked(), ui.window_dock_id().is_some()));
        (id, host, docked)
    };
    let mut last = None;
    for _ in 0..3 {
        last = Some(frame(draw).unwrap());
    }
    let (id, host, docked) = last.unwrap();
    assert_ne!(id, DockId(0));
    assert_eq!(host, Some(Some(id)));
    assert_eq!(docked, Some((true, true)));
    assert_eq!(take_error(), None);
}

#[test]
fn a_dock_space_needs_an_id() {
    let _imgui = lock();
    frame(|ui| test_window(ui, |ui| ui.dock_space(DockId(0), [0.0, 0.0], DockNodeFlags::NONE))).unwrap();
    assert_eq!(take_error().as_deref(), Some("DockSpace() needs an ID other than 0"));
}

#[test]
fn cursor_window_and_mouse_positions() {
    let _imgui = lock();
    let result = frame_at([30.0, 40.0], true, |ui| {
        let screen = ui.screen_rect();
        test_window(ui, |ui| {
            ui.set_cursor_screen_pos([100.0, 200.0]);
            let cursor = ui.cursor_screen_pos();
            let hovering = (ui.is_mouse_hovering_rect([20.0, 30.0], [40.0, 50.0]), ui.is_mouse_hovering_rect([200.0, 200.0], [210.0, 210.0]));
            (cursor, ui.window_pos(), ui.window_size(), ui.mouse_pos(), ui.is_mouse_down(MouseButton::Left), ui.is_mouse_down(MouseButton::Right), hovering, screen)
        })
    });
    let expected = ([100.0, 200.0], [10.0, 10.0], [500.0, 600.0], [30.0, 40.0], true, false, (true, false), ([0.0, 0.0], [1280.0, 800.0]));
    assert_eq!(result, Ok(Some(expected)));
    assert_eq!(take_error(), None, "moving the cursor past the content and ending the window is fine");
    frame_at([0.0, 0.0], false, |_| ()).unwrap();
}

#[test]
fn imgui_itself_rejects_a_cursor_moved_past_the_content() {
    // What set_cursor_screen_pos prevents: with ImGui's own call, ending the
    // window throws, leaving it open, and the frame fails as it would pause
    // MacroQuest's ImGui.
    let _imgui = lock();
    let result = frame(|ui| test_window(ui, |_| unsafe { mqemu_imgui_test_set_cursor_screen_pos_raw(100.0, 900.0) }));
    let error = take_error().unwrap_or_default();
    assert!(error.starts_with("Code uses SetCursorPos()/SetCursorScreenPos() to extend window/parent boundaries"), "{error}");
    assert_eq!(result, Err("Missing End()".to_owned()));
}

#[test]
fn an_invisible_button_is_clickable() {
    let _imgui = lock();
    let draw = |ui: &mut Ui| test_window(ui, |ui| (ui.invisible_button("hit area", [80.0, 30.0]), last_item_center())).unwrap();
    let (_, at) = frame(draw).unwrap();
    let mut clicks = Vec::new();
    for down in [false, true, false] {
        clicks.push(frame_at(at, down, draw).unwrap().0);
    }
    assert_eq!(clicks, [false, false, true]);
    let right_only = |ui: &mut Ui| test_window(ui, |ui| ui.invisible_button_with("right", [80.0, 30.0], ButtonFlags::MOUSE_BUTTON_RIGHT)).unwrap();
    frame(right_only).unwrap();
    assert_eq!(take_error(), None);
}

#[test]
#[should_panic(expected = "can't be 0 wide or tall")]
fn an_invisible_button_needs_a_size() {
    let mut ui = Ui::new(&fake::APIS);
    ui.invisible_button("nothing", [0.0, 10.0]);
}

#[test]
fn a_zero_size_invisible_button_is_an_imgui_error() {
    // What an_invisible_button_needs_a_size keeps from ImGui.
    let _imgui = lock();
    frame(|ui| test_window(ui, |_| unsafe { (sys::API.mqemu_imgui_invisible_button)(c"b".as_ptr(), 1, 0.0, 10.0, 0) })).unwrap();
    assert_eq!(take_error().as_deref(), Some("InvisibleButton() can't be 0 wide or tall"));
}
