//! Laying out dock spaces in code, with ImGui's dock builder.

use std::ffi::c_char;

use mq_emu_sys::imgui as sys;

use super::draw::Vec2;
use super::{DockId, DockNodeFlags, Ui, s};

/// A side of a dock node, to split it towards (`ImGuiDir_`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Dir {
    Left = sys::ImGuiDir_Left,
    Right = sys::ImGuiDir_Right,
    Up = sys::ImGuiDir_Up,
    Down = sys::ImGuiDir_Down,
}

/// A dock node as it is now ([`Ui::dock_node`]).
#[derive(Debug, Clone, PartialEq)]
pub struct DockNode {
    pub id: DockId,
    /// None for a root node: a dock space, or a floating node.
    pub parent: Option<DockId>,
    /// The two nodes it's split into, if it is.
    pub children: Option<[DockId; 2]>,
    /// Its tree's central node: what a dock space keeps for itself, the
    /// space left once the other nodes have theirs.
    pub central: Option<DockId>,
    /// Its top left and size on the screen.
    pub pos: Vec2,
    pub size: Vec2,
    /// Its own and inherited flags.
    pub flags: DockNodeFlags,
    /// The titles of the windows docked in it.
    pub windows: Vec<String>,
    pub is_dock_space: bool,
    pub is_central: bool,
    /// A root node not in a dock space: a window of its own.
    pub is_floating: bool,
    pub is_visible: bool,
}

/// Longest window title [`DockNode::windows`] keeps; ImGui's titles are
/// rarely longer.
const MAX_WINDOW_TITLE: usize = 512;

fn check_id(id: DockId) {
    // ImGui takes node 0 to mean every node (the shim refuses it too).
    assert!(id.0 != 0, "dock node 0 isn't a node");
}

fn some_id(id: u32) -> Option<DockId> {
    (id != 0).then_some(DockId(id))
}

impl Ui {
    /// Describes dock node `id` (a dock space, or a node in one); None if
    /// there's no such node.
    pub fn dock_node(&self, id: DockId) -> Option<DockNode> {
        if id.0 == 0 {
            return None;
        }
        let mut info = sys::mqemu_dock_node_info::default();
        if !unsafe { (self.api.mqemu_imgui_dock_node)(id.0, &mut info) } {
            return None;
        }
        let windows = (0..info.windows)
            .filter_map(|index| {
                let mut title = [0 as c_char; MAX_WINDOW_TITLE];
                unsafe { (self.api.mqemu_imgui_dock_node_window)(id.0, index, title.as_mut_ptr(), title.len()) }
                    .then(|| crate::text::from_c_array(&title))
            })
            .collect();
        let [first, second] = info.children;
        // DockSpace and CentralNode are ImGui's to set; they're the two bools.
        let own = sys::ImGuiDockNodeFlags_DockSpace | sys::ImGuiDockNodeFlags_CentralNode;
        Some(DockNode {
            id,
            parent: some_id(info.parent),
            children: (first != 0 && second != 0).then_some([DockId(first), DockId(second)]),
            central: some_id(info.central),
            pos: [info.x, info.y],
            size: [info.w, info.h],
            flags: DockNodeFlags(info.flags & !own),
            windows,
            is_dock_space: info.dock_space,
            is_central: info.central_node,
            is_floating: info.floating,
            is_visible: info.visible,
        })
    }

    /// Lays out dock nodes with `f` through ImGui's dock builder. The nodes
    /// `f` adds are finished (their windows docked) when it returns. None,
    /// without calling `f`, if docking is off.
    ///
    /// ```no_run
    /// # fn draw(ui: &mut mq_emu::Ui, laid_out: &mut bool) {
    /// use mq_emu::imgui::{Cond, Dir, DockNodeFlags};
    /// let id = ui.dock_id("my plugin's dock space");
    /// if !*laid_out {
    ///     // Once: a dock space split in two, a window docked in each half.
    ///     ui.dock_builder(|dock| {
    ///         let root = dock.add_dock_space(id, DockNodeFlags::NONE)?;
    ///         dock.set_node_size(root, [600.0, 400.0]);
    ///         let (left, right) = dock.split(root, Dir::Left, 0.3)?;
    ///         dock.dock_window("Spawns", left);
    ///         dock.dock_window("Details", right);
    ///         Some(())
    ///     });
    ///     *laid_out = true;
    /// }
    /// ui.window("My plugin").size([600.0, 400.0], Cond::FirstUseEver).show(|ui| ui.dock_space(id, [0.0, 0.0], DockNodeFlags::NONE));
    /// ui.window("Spawns").show(|ui| ui.text("left"));
    /// ui.window("Details").show(|ui| ui.text("right"));
    /// # }
    /// ```
    ///
    /// Build a layout before the dock space and its windows are drawn that
    /// frame. ImGui saves docking in MacroQuest's ImGui ini, so build once (or
    /// when the user asks to reset the layout), not every frame.
    ///
    /// Changing nodes from inside a window docked in them is an ImGui error
    /// (ImGui would free nodes the window is using), as is node 0.
    pub fn dock_builder<R>(&mut self, f: impl FnOnce(&mut DockBuilder<'_>) -> R) -> Option<R> {
        if !self.is_docking_enabled() {
            return None;
        }
        Some(f(&mut DockBuilder { ui: self, roots: Vec::new() }))
    }
}

/// Builds dock nodes; see [`Ui::dock_builder`]. Calls that make nodes return
/// None when ImGui refused (the error is reported as ImGui errors are).
pub struct DockBuilder<'ui> {
    ui: &'ui mut Ui,
    /// Root nodes added, finished when it's dropped.
    roots: Vec<DockId>,
}

impl Drop for DockBuilder<'_> {
    /// Even if the builder's closure panicked, so what was built is consistent.
    fn drop(&mut self) {
        for root in &self.roots {
            unsafe { (self.ui.api.mqemu_imgui_dock_builder_finish)(root.0) }
        }
    }
}

impl DockBuilder<'_> {
    fn api(&self) -> &'static sys::Api {
        self.ui.api
    }

    /// Replaces any node `id` with an empty dock space node, to split and
    /// dock windows into, then show with [`Ui::dock_space`] with the same ID.
    ///
    /// # Panics
    /// If `id` is 0.
    pub fn add_dock_space(&mut self, id: DockId, flags: DockNodeFlags) -> Option<DockId> {
        check_id(id);
        let added = some_id(unsafe { (self.api().mqemu_imgui_dock_builder_add_node)(id.0, flags.bits(), true) })?;
        self.roots.push(added);
        Some(added)
    }

    /// A floating node (a window of its own that windows dock into),
    /// replacing any node `id`; None picks an ID. Place it with
    /// [`DockBuilder::set_node_pos`] and [`DockBuilder::set_node_size`].
    pub fn add_node(&mut self, id: Option<DockId>, flags: DockNodeFlags) -> Option<DockId> {
        let raw = id.map_or(0, |id| id.0);
        let added = some_id(unsafe { (self.api().mqemu_imgui_dock_builder_add_node)(raw, flags.bits(), false) })?;
        self.roots.push(added);
        Some(added)
    }

    /// Removes the node and the nodes in it, undocking their windows.
    ///
    /// # Panics
    /// If `id` is 0.
    pub fn remove_node(&mut self, id: DockId) {
        check_id(id);
        unsafe { (self.api().mqemu_imgui_dock_builder_remove_node)(id.0) }
    }

    /// Removes the nodes root node `id` was split into, moving their windows into it.
    ///
    /// # Panics
    /// If `id` is 0.
    pub fn remove_child_nodes(&mut self, id: DockId) {
        check_id(id);
        unsafe { (self.api().mqemu_imgui_dock_builder_remove_node_child_nodes)(id.0) }
    }

    /// Undocks the windows of root node `id` and the nodes in it; with
    /// `forget`, they also forget (in the ini) that they were docked there.
    ///
    /// # Panics
    /// If `id` is 0.
    pub fn remove_docked_windows(&mut self, id: DockId, forget: bool) {
        check_id(id);
        unsafe { (self.api().mqemu_imgui_dock_builder_remove_node_docked_windows)(id.0, forget) }
    }

    /// Moves a floating node. Nothing if there's no such node.
    ///
    /// # Panics
    /// If `id` is 0.
    pub fn set_node_pos(&mut self, id: DockId, pos: Vec2) {
        check_id(id);
        unsafe { (self.api().mqemu_imgui_dock_builder_set_node_pos)(id.0, pos[0], pos[1]) }
    }

    /// Sizes a node; set a new node's size before splitting it, or the split
    /// sizes come out wrong. Nothing if there's no such node.
    ///
    /// # Panics
    /// If `id` is 0, or a size isn't more than 0.
    pub fn set_node_size(&mut self, id: DockId, size: Vec2) {
        check_id(id);
        assert!(size[0] > 0.0 && size[1] > 0.0, "a dock node's size must be more than 0: {size:?}");
        unsafe { (self.api().mqemu_imgui_dock_builder_set_node_size)(id.0, size[0], size[1]) }
    }

    /// Splits node `id` in two: `ratio` (0 to 1) of it towards `dir`, the rest
    /// opposite. Returns the new nodes, towards `dir` first; None if there's
    /// no such node, it's split already, or ImGui refused.
    ///
    /// # Panics
    /// If `id` is 0.
    pub fn split(&mut self, id: DockId, dir: Dir, ratio: f32) -> Option<(DockId, DockId)> {
        check_id(id);
        if self.ui.dock_node(id)?.children.is_some() {
            return None;
        }
        let (mut at_dir, mut opposite) = (0, 0);
        some_id(unsafe { (self.api().mqemu_imgui_dock_builder_split_node)(id.0, dir as i32, ratio, &mut at_dir, &mut opposite) })?;
        Some((DockId(at_dir), DockId(opposite)))
    }

    /// Docks the window titled `title` (shown yet or not) into node `id`,
    /// from the next time it's drawn.
    ///
    /// # Panics
    /// If `id` is 0 ([`DockBuilder::undock_window`] undocks).
    pub fn dock_window(&mut self, title: &str, id: DockId) {
        check_id(id);
        let (t, n) = s(title);
        unsafe { (self.api().mqemu_imgui_dock_builder_dock_window)(t, n, id.0) }
    }

    /// Undocks the window titled `title`.
    pub fn undock_window(&mut self, title: &str) {
        let (t, n) = s(title);
        unsafe { (self.api().mqemu_imgui_dock_builder_dock_window)(t, n, 0) }
    }

    /// Gives window `to` the position, size and collapsed state of window
    /// `from` (if `from` exists).
    pub fn copy_window_settings(&mut self, from: &str, to: &str) {
        let ((f, fl), (t, tl)) = (s(from), s(to));
        unsafe { (self.api().mqemu_imgui_dock_builder_copy_window_settings)(f, fl, t, tl) }
    }

    /// Replaces node `to` with a copy of node `from` and the nodes in it.
    /// Returns each copied node with its copy; None if there's no node
    /// `from`, or replacing `to` would remove it.
    ///
    /// # Panics
    /// If either is 0, or they're the same node.
    pub fn copy_node(&mut self, from: DockId, to: DockId) -> Option<Vec<(DockId, DockId)>> {
        check_id(from);
        check_id(to);
        assert!(from != to, "can't copy a dock node onto itself");
        let nodes = self.tree_size(from)?;
        let mut pairs = vec![0; nodes * 2];
        let cap = i32::try_from(nodes).unwrap_or(i32::MAX);
        let count = unsafe { (self.api().mqemu_imgui_dock_builder_copy_node)(from.0, to.0, pairs.as_mut_ptr(), cap) };
        let count = usize::try_from(count).ok()?.min(nodes);
        Some(pairs.chunks_exact(2).take(count).map(|p| (DockId(p[0]), DockId(p[1]))).collect())
    }

    /// Nodes in the tree under `id`, counting it.
    fn tree_size(&self, id: DockId) -> Option<usize> {
        let node = self.ui.dock_node(id)?;
        Some(1 + node.children.map_or(0, |children| children.iter().filter_map(|&c| self.tree_size(c)).sum()))
    }

    /// Copies dock space `from`'s layout to dock space `to`; each `(source,
    /// destination)` pair of window titles puts the destination window where
    /// the source one is.
    ///
    /// # Panics
    /// If either is 0, or they're the same node.
    pub fn copy_dock_space(&mut self, from: DockId, to: DockId, windows: &[(&str, &str)]) {
        check_id(from);
        check_id(to);
        assert!(from != to, "can't copy a dock space onto itself");
        let (titles, lens): (Vec<*const c_char>, Vec<usize>) = windows.iter().flat_map(|&(a, b)| [s(a), s(b)]).unzip();
        let pairs = i32::try_from(windows.len()).unwrap_or(i32::MAX);
        unsafe { (self.api().mqemu_imgui_dock_builder_copy_dock_space)(from.0, to.0, titles.as_ptr(), lens.as_ptr(), pairs) }
    }

    /// Docks the windows docked under root node `id` now (rather than when
    /// the builder ends, as for nodes it added).
    ///
    /// # Panics
    /// If `id` is 0.
    pub fn finish(&mut self, id: DockId) {
        check_id(id);
        unsafe { (self.api().mqemu_imgui_dock_builder_finish)(id.0) }
    }

    /// Describes node `id`, as [`Ui::dock_node`].
    pub fn node(&self, id: DockId) -> Option<DockNode> {
        self.ui.dock_node(id)
    }
}
