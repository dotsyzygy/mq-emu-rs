//! Multi-select: clicks, shift and ctrl clicks, ctrl+A and box selection
//! reach a `Selection`, and the scope always ends.

use super::*;

const ITEMS: usize = 6;

/// A multi-select list of `ITEMS` selectables in the test window: their
/// centers, and whether each one's selection toggled this frame.
fn draw_list(ui: &mut Ui, selection: &mut Selection, flags: MultiSelectFlags) -> Vec<([f32; 2], bool)> {
    test_window(ui, |ui| {
        ui.multi_select(flags, selection, ITEMS, |list, selection| {
            (0..ITEMS)
                .map(|i| {
                    list.set_next_item_index(i);
                    list.selectable(&format!("item {i}"), selection.contains(i));
                    (last_item_center(), list.is_item_toggled_selection())
                })
                .collect()
        })
        .unwrap()
    })
    .unwrap()
}

/// Hovers, presses and releases at `at`, holding ctrl and shift as given.
fn click(selection: &mut Selection, flags: MultiSelectFlags, at: [f32; 2], ctrl: bool, shift: bool) -> bool {
    unsafe { mqemu_imgui_test_modifiers(ctrl, shift) };
    let mut toggled = false;
    for down in [false, true, false] {
        let items = frame_at(at, down, |ui| draw_list(ui, selection, flags)).unwrap();
        toggled |= items.iter().any(|(center, toggled)| *center == at && *toggled);
    }
    unsafe { mqemu_imgui_test_modifiers(false, false) };
    frame_at(at, false, |ui| draw_list(ui, selection, flags)).unwrap();
    toggled
}

#[test]
fn clicks_shift_clicks_ctrl_clicks_and_ctrl_a_select() {
    let _imgui = lock();
    let flags = MultiSelectFlags::NONE;
    let mut selection = Selection::new();
    let centers: Vec<[f32; 2]> = frame(|ui| draw_list(ui, &mut selection, flags)).unwrap().into_iter().map(|(c, _)| c).collect();

    assert!(click(&mut selection, flags, centers[1], false, false), "the item reports the toggle before the list ends");
    assert_eq!(selection.iter().collect::<Vec<_>>(), [1]);
    click(&mut selection, flags, centers[4], false, true);
    assert_eq!(selection.iter().collect::<Vec<_>>(), [1, 2, 3, 4], "shift+click selects the range");
    click(&mut selection, flags, centers[2], true, false);
    assert_eq!(selection.iter().collect::<Vec<_>>(), [1, 3, 4], "ctrl+click toggles one");
    click(&mut selection, flags, centers[5], false, false);
    assert_eq!(selection.iter().collect::<Vec<_>>(), [5], "a plain click selects only it");

    // Ctrl+A, with the list focused by the click.
    let at = [centers[5][0], centers[5][1]];
    unsafe {
        mqemu_imgui_test_modifiers(true, false);
        mqemu_imgui_test_letter_key(b'A' as c_char, true);
    }
    frame_at(at, false, |ui| draw_list(ui, &mut selection, flags)).unwrap();
    unsafe {
        mqemu_imgui_test_letter_key(b'A' as c_char, false);
        mqemu_imgui_test_modifiers(false, false);
    }
    frame_at(at, false, |ui| draw_list(ui, &mut selection, flags)).unwrap();
    assert_eq!(selection.len(), ITEMS, "ctrl+A selects all {ITEMS}: {selection:?}");
    assert_eq!(take_error(), None);
}

#[test]
fn single_select_keeps_one() {
    let _imgui = lock();
    let flags = MultiSelectFlags::SINGLE_SELECT;
    let mut selection = Selection::new();
    let centers: Vec<[f32; 2]> = frame(|ui| draw_list(ui, &mut selection, flags)).unwrap().into_iter().map(|(c, _)| c).collect();
    click(&mut selection, flags, centers[0], false, false);
    click(&mut selection, flags, centers[3], false, true);
    assert_eq!(selection.iter().collect::<Vec<_>>(), [3]);
    assert_eq!(take_error(), None);
}

#[test]
fn dragging_a_box_from_empty_space_selects_what_it_covers() {
    let _imgui = lock();
    let flags = MultiSelectFlags::BOX_SELECT_1D;
    let mut selection = Selection::new();
    let centers: Vec<[f32; 2]> = frame(|ui| draw_list(ui, &mut selection, flags)).unwrap().into_iter().map(|(c, _)| c).collect();
    // From below the last item, up to the middle of item 3.
    let start = [centers[0][0], centers[ITEMS - 1][1] + 60.0];
    let path = [(start, false), (start, true), ([start[0], start[1] - 30.0], true), (centers[3], true), (centers[3], true), (centers[3], false)];
    for (at, down) in path {
        frame_at(at, down, |ui| draw_list(ui, &mut selection, flags)).unwrap();
    }
    assert_eq!(selection.iter().collect::<Vec<_>>(), [3, 4, 5]);
    frame_at([1000.0, 700.0], false, |ui| draw_list(ui, &mut selection, flags)).unwrap();
    assert_eq!(take_error(), None);
}

#[test]
fn requests_can_go_to_a_selection_kept_elsewhere() {
    let _imgui = lock();
    let mut checked = [false; ITEMS];
    let draw = |ui: &mut Ui, checked: &mut [bool; ITEMS]| {
        test_window(ui, |ui| {
            let (centers, end) = ui
                .multi_select_io(MultiSelectFlags::NONE, Some(checked.iter().filter(|c| **c).count()), Some(ITEMS), |list| {
                    list.begin_io().apply_with(|i, selected| checked[i] = selected);
                    (0..ITEMS)
                        .map(|i| {
                            list.set_next_item_index(i);
                            list.selectable(&format!("row {i}"), checked[i]);
                            last_item_center()
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap();
            end.apply_with(|i, selected| checked[i] = selected);
            centers
        })
        .unwrap()
    };
    let centers = frame(|ui| draw(ui, &mut checked)).unwrap();
    for down in [false, true, false] {
        frame_at(centers[2], down, |ui| draw(ui, &mut checked)).unwrap();
    }
    assert_eq!(checked, [false, false, true, false, false, false]);
    assert_eq!(take_error(), None);
}

#[test]
fn a_list_ends_when_its_closure_panics_and_lists_nest() {
    let _imgui = lock();
    let mut outer = Selection::new();
    let mut inner = Selection::new();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let panicked = catch_unwind(AssertUnwindSafe(|| {
                ui.multi_select(MultiSelectFlags::NONE, &mut outer, 2, |list, _| {
                    list.set_next_item_index(0);
                    list.selectable("a", false);
                    panic!("deliberate");
                })
            }))
            .is_err();
            ui.multi_select(MultiSelectFlags::SCOPE_RECT, &mut outer, 2, |list, outer| {
                list.set_next_item_index(0);
                list.selectable("outer 0", outer.contains(0));
                // Its own ID, or both lists would share ImGui's state for them.
                list.with_id("inner", |ui| {
                    ui.multi_select(MultiSelectFlags::SCOPE_RECT, &mut inner, 2, |list, inner| {
                        list.set_next_item_index(1);
                        list.selectable("inner 1", inner.contains(1));
                    })
                });
                list.reset_range_src();
            });
            panicked
        })
    });
    assert_eq!(result, Ok(Some(true)));
    assert_eq!(take_error(), None);
}

#[test]
fn scope_only_calls_outside_a_list_are_refused() {
    let _imgui = lock();
    // ImGui would dereference the missing list.
    let errors = frame(|_| unsafe {
        (sys::API.mqemu_imgui_is_item_toggled_selection)();
        let toggled = take_error();
        (sys::API.mqemu_imgui_multi_select_reset_range_src)();
        (toggled, take_error())
    })
    .unwrap();
    assert_eq!(errors.0.as_deref(), Some("IsItemToggledSelection() is only for inside a multi-select scope"));
    assert_eq!(errors.1.as_deref(), Some("not in a multi-select scope"));
}

fn io(requests: Vec<SelectionRequest>, items: Option<usize>) -> MultiSelectIo {
    MultiSelectIo { requests, items_count: items, ..MultiSelectIo::default() }
}

#[test]
fn a_selection_applies_requests_as_imgui_s_basic_storage_does() {
    let mut selection: Selection = [7, 9].into_iter().collect();
    selection.apply(&io(vec![SelectionRequest::SetAll { selected: true }], Some(4)));
    assert_eq!(selection.iter().collect::<Vec<_>>(), [0, 1, 2, 3], "select all replaces what was there");
    selection.apply(&io(vec![SelectionRequest::SetRange { range: 1..=2, selected: false, backwards: true }], Some(4)));
    assert_eq!(selection.iter().collect::<Vec<_>>(), [0, 3]);
    selection.apply(&io(vec![SelectionRequest::SetAll { selected: false }, SelectionRequest::SetRange { range: 5..=6, selected: true, backwards: false }], None));
    assert_eq!(selection.iter().collect::<Vec<_>>(), [5, 6], "requests apply in order");
    selection.apply(&io(vec![SelectionRequest::SetAll { selected: true }], None));
    assert!(selection.is_empty(), "selecting all of an unknown count selects none");
    selection.set(3, true);
    selection.set(8, true);
    selection.retain(|&i| i < 5);
    assert_eq!(selection.iter().collect::<Vec<_>>(), [3]);

    let mut external = [true, false, false];
    io(vec![SelectionRequest::SetAll { selected: false }, SelectionRequest::SetRange { range: 1..=2, selected: true, backwards: false }], Some(3))
        .apply_with(|i, selected| external[i] = selected);
    assert_eq!(external, [false, true, true]);
}
