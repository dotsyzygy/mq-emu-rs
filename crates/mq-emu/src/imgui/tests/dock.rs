//! The dock builder: layouts built in code land where they say, and what the
//! shim refuses because ImGui would crash or undo everyone's docking.

use super::*;

const LEFT: &str = "Left##dock test";
const RIGHT: &str = "Right##dock test";

/// A dock space split in two, `LEFT` and `RIGHT` docked in its halves:
/// (root, left, right).
fn build_split(ui: &mut Ui, id: DockId) -> (DockId, DockId, DockId) {
    ui.dock_builder(|dock| {
        let root = dock.add_dock_space(id, DockNodeFlags::NONE)?;
        dock.set_node_size(root, [600.0, 400.0]);
        let (left, right) = dock.split(root, Dir::Left, 0.25)?;
        dock.dock_window(LEFT, left);
        dock.dock_window(RIGHT, right);
        Some((root, left, right))
    })
    .flatten()
    .expect("docking is on in the test context, and nothing is refused")
}

/// The dock space's host window, then `LEFT` and `RIGHT` (with `inside`
/// drawing in `LEFT`); the nodes they're docked in.
fn draw_docked(ui: &mut Ui, id: DockId, inside: impl FnOnce(&mut Ui)) -> (Option<DockId>, Option<DockId>) {
    ui.window("dock host").position([10.0, 10.0], Cond::Always).size([600.0, 400.0], Cond::Always).flags(WindowFlags::NO_SAVED_SETTINGS).show(|ui| {
        ui.dock_space(id, [0.0, 0.0], DockNodeFlags::NONE);
    });
    let left = ui.window(LEFT).show(|ui| {
        inside(ui);
        ui.window_dock_id()
    });
    let right = ui.window(RIGHT).show(|ui| ui.window_dock_id());
    (left.flatten(), right.flatten())
}

#[test]
fn a_built_layout_docks_windows_where_it_says() {
    let _imgui = lock();
    let id = DockId(0xD0C0_0001);
    let (root, left, right) = frame(|ui| build_split(ui, id)).unwrap();
    assert_eq!(root, id);
    let mut docked = (None, None);
    for _ in 0..3 {
        docked = frame(|ui| draw_docked(ui, id, |_| ())).unwrap();
    }
    assert_eq!(docked, (Some(left), Some(right)));
    let (root_node, left_node, right_node) = frame(|ui| (ui.dock_node(root), ui.dock_node(left), ui.dock_node(right))).unwrap();
    let (root_node, left_node, right_node) = (root_node.unwrap(), left_node.unwrap(), right_node.unwrap());
    assert!(root_node.is_dock_space);
    assert_eq!(root_node.parent, None);
    assert_eq!(root_node.children, Some([left, right]));
    assert_eq!(left_node.parent, Some(root));
    assert_eq!(left_node.windows, [LEFT]);
    assert_eq!(right_node.windows, [RIGHT]);
    assert!(left_node.size[0] < right_node.size[0], "a quarter on the left: {:?} vs {:?}", left_node.size, right_node.size);
    assert_eq!(root_node.central, Some(right), "the dock space keeps the bigger part");
    assert_eq!(take_error(), None);
}

#[test]
fn node_flags_reach_the_node_and_unknown_nodes_are_none() {
    let _imgui = lock();
    let (added, flags, missing) = frame(|ui| {
        let added = ui.dock_builder(|dock| dock.add_node(None, DockNodeFlags::NO_TAB_BAR | DockNodeFlags::NO_CLOSE_BUTTON)).flatten();
        let flags = added.and_then(|id| ui.dock_node(id)).map(|node| (node.flags, node.is_floating, node.is_dock_space));
        (added, flags, ui.dock_node(DockId(0xD0C0_FFFF)).is_none() && ui.dock_node(DockId(0)).is_none())
    })
    .unwrap();
    assert!(added.is_some());
    let (flags, floating, dock_space) = flags.unwrap();
    assert!(flags.contains(DockNodeFlags::NO_TAB_BAR | DockNodeFlags::NO_CLOSE_BUTTON), "{flags:?}");
    assert!(floating && !dock_space);
    assert!(missing);
    assert_eq!(take_error(), None);
}

#[test]
fn node_zero_which_imgui_takes_as_every_node_is_refused() {
    let _imgui = lock();
    let id = DockId(0xD0C0_0002);
    let (_, left, right) = frame(|ui| build_split(ui, id)).unwrap();
    // To ImGui, these would remove every plugin's dock nodes and undock
    // every window. The layout above is the control: it must survive.
    let errors = frame(|_| unsafe {
        let api = &sys::API;
        (api.mqemu_imgui_dock_builder_remove_node_child_nodes)(0);
        let first = take_error();
        (api.mqemu_imgui_dock_builder_remove_node_docked_windows)(0, true);
        (first, take_error())
    })
    .unwrap();
    assert_eq!(errors, (Some("dock node 0 isn't a node".to_owned()), Some("dock node 0 isn't a node".to_owned())));
    let children = frame(|ui| ui.dock_node(id).and_then(|node| node.children)).unwrap();
    assert_eq!(children, Some([left, right]));
    // The safe API stops it before ImGui sees it.
    let mut ui = Ui::new(&fake::APIS);
    let mut builder_panics = |f: fn(&mut DockBuilder<'_>)| catch_unwind(AssertUnwindSafe(|| ui.dock_builder(f))).is_err();
    assert!(builder_panics(|dock| dock.remove_child_nodes(DockId(0))));
    assert!(builder_panics(|dock| dock.remove_docked_windows(DockId(0), true)));
    assert!(builder_panics(|dock| dock.remove_node(DockId(0))));
}

#[test]
fn nodes_cannot_change_from_inside_a_window_docked_in_them() {
    let _imgui = lock();
    let id = DockId(0xD0C0_0003);
    frame(|ui| build_split(ui, id)).unwrap();
    for _ in 0..2 {
        frame(|ui| draw_docked(ui, id, |_| ())).unwrap();
    }
    // Removing the nodes would free the node LEFT is drawing in.
    let result = frame(|ui| {
        draw_docked(ui, id, |ui| {
            ui.dock_builder(|dock| dock.remove_node(id));
        })
    });
    assert!(result.is_ok(), "the frame still ends: {result:?}");
    assert_eq!(take_error().as_deref(), Some("can't change dock nodes from inside a window docked in them"));
    assert!(frame(|ui| ui.dock_node(id).is_some()).unwrap(), "nothing was removed");
    // From outside the windows, it's fine.
    frame(|ui| ui.dock_builder(|dock| dock.remove_node(id))).unwrap();
    assert!(!frame(|ui| ui.dock_node(id).is_some()).unwrap());
    for _ in 0..2 {
        assert!(frame(|ui| draw_docked(ui, id, |_| ())).is_ok());
    }
    assert_eq!(take_error(), None);
}

#[test]
fn only_leaf_nodes_split_and_only_in_a_direction() {
    let _imgui = lock();
    let id = DockId(0xD0C0_0004);
    frame(|ui| build_split(ui, id)).unwrap();
    let (again, missing) = frame(|ui| ui.dock_builder(|dock| (dock.split(id, Dir::Up, 0.5), dock.split(DockId(0xD0C0_FFFE), Dir::Up, 0.5)))).unwrap().unwrap();
    assert_eq!((again, missing), (None, None), "split already, and no such node");
    assert_eq!(take_error(), None, "the safe API checks first");
    // What the shim does when asked anyway: ImGui would split the node's
    // children apart, or index past its directions.
    let (mut a, mut b) = (0, 0);
    let errors = frame(|_| unsafe {
        (sys::API.mqemu_imgui_dock_builder_split_node)(id.0, sys::ImGuiDir_Up, 0.5, &mut a, &mut b);
        let split = take_error();
        (sys::API.mqemu_imgui_dock_builder_split_node)(id.0, 7, 0.5, &mut a, &mut b);
        (split, take_error())
    })
    .unwrap();
    assert_eq!(errors.0.as_deref(), Some("the dock node is already split; split one of its children"));
    assert_eq!(errors.1.as_deref(), Some("a dock node splits Left, Right, Up or Down"));
    let mut ui = Ui::new(&fake::APIS);
    let zero_size = catch_unwind(AssertUnwindSafe(|| ui.dock_builder(|dock| dock.set_node_size(id, [0.0, 10.0]))));
    assert!(zero_size.is_err(), "a node can't be 0 wide");
}

#[test]
fn with_docking_off_nothing_is_built_and_imgui_is_not_handed_a_missing_node() {
    let _imgui = lock();
    unsafe { mqemu_imgui_test_set_docking(false) };
    let _restore = OnDrop(|| unsafe { mqemu_imgui_test_set_docking(true) });
    let (built, raw) = frame(|ui| {
        let built = ui.dock_builder(|_| ()).is_some();
        // ImGui's DockSpace() makes no node with docking off, and
        // DockBuilderAddNode then dereferences the node it didn't make.
        let raw = unsafe { (sys::API.mqemu_imgui_dock_builder_add_node)(0xD0C0_0005, 0, true) };
        (built, raw)
    })
    .unwrap();
    assert_eq!((built, raw), (false, 0));
    assert_eq!(take_error(), None);
}

#[test]
fn nodes_copy_with_their_children_but_not_over_their_source() {
    let _imgui = lock();
    let (source, copy) = (DockId(0xD0C0_0006), DockId(0xD0C0_0007));
    let result = frame(|ui| {
        ui.dock_builder(|dock| {
            dock.add_node(Some(source), DockNodeFlags::NONE)?;
            dock.set_node_size(source, [300.0, 200.0]);
            let (top, _) = dock.split(source, Dir::Up, 0.5)?;
            let pairs = dock.copy_node(source, copy)?;
            let copied = dock.node(copy)?;
            // Copying over the source's own root would remove the source.
            let over_root = dock.copy_node(top, source);
            Some((pairs, copied.children.is_some(), over_root))
        })
    })
    .unwrap()
    .flatten();
    let (pairs, split, over_root) = result.unwrap();
    assert_eq!(pairs.len(), 3, "the root and its two halves: {pairs:?}");
    assert_eq!(pairs[0], (source, copy));
    assert!(split, "the copy is split too");
    assert_eq!(over_root, None);
    assert_eq!(take_error().as_deref(), Some("removing the destination dock node removed the source node too"));
    let mut ui = Ui::new(&fake::APIS);
    assert!(catch_unwind(AssertUnwindSafe(|| ui.dock_builder(|dock| dock.copy_node(copy, copy)))).is_err());
}

#[test]
fn dock_spaces_copy_with_their_windows_remapped() {
    let _imgui = lock();
    let (source, copy) = (DockId(0xD0C0_0008), DockId(0xD0C0_0009));
    frame(|ui| build_split(ui, source)).unwrap();
    let copied = frame(|ui| {
        ui.dock_builder(|dock| {
            dock.add_dock_space(copy, DockNodeFlags::NONE);
            dock.copy_dock_space(source, copy, &[(LEFT, "Left copy##dock test"), (RIGHT, "Right copy##dock test")]);
            dock.copy_window_settings(LEFT, "Left copy##dock test");
            dock.undock_window("Right copy##dock test");
            dock.node(copy).and_then(|node| node.children)
        })
    })
    .unwrap()
    .flatten();
    assert!(copied.is_some(), "the copy is split like the source");
    assert_eq!(take_error(), None);
}

#[test]
fn a_builder_whose_closure_panics_still_finishes() {
    let _imgui = lock();
    let id = DockId(0xD0C0_000A);
    let result = frame(|ui| {
        catch_unwind(AssertUnwindSafe(|| {
            ui.dock_builder(|dock| {
                dock.add_dock_space(id, DockNodeFlags::NONE);
                dock.dock_window(LEFT, id);
                panic!("deliberate");
            })
        }))
        .is_err()
    });
    assert_eq!(result, Ok(true));
    assert!(frame(|ui| ui.dock_node(id).is_some()).unwrap());
    assert_eq!(take_error(), None);
}
