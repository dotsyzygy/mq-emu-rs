//! Any ImGui window, this plugin's or another's, from ImGui's internals
//! (`imgui_internal.h`): found by name ([`Ui::find_window`]), read
//! ([`WindowInfo`]: place, scrolling, focus and display order), and moved,
//! sized, focused, brought to the front and scrolled ([`WindowRef`]).
//!
//! ```no_run
//! # fn draw(ui: &mut mq_emu::Ui) {
//! use mq_emu::imgui::{Cond, FocusRequestFlags};
//! // Bring MacroQuest's console to the front, at the top left.
//! if let Some(mut console) = ui.find_window("MacroQuest Console") {
//!     console.set_pos([0.0, 0.0], Cond::Always);
//!     console.focus(FocusRequestFlags::NONE);
//! }
//! # }
//! ```

use std::ffi::c_char;

use mq_emu_sys::imgui as sys;

use super::{Cond, DockId, Ui, Vec2, WindowFlags, s};

/// An ImGui window's ID: the hash of its name ([`Ui::window_id`]), which
/// ImGui finds windows by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub u32);

/// What a window is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowKind {
    /// A window of its own ([`Ui::window`]).
    TopLevel,
    /// A child region ([`Ui::child`]), its name made from its parent's.
    Child,
    Popup,
    Modal,
    /// A menu's popup.
    Menu,
    Tooltip,
}

/// A window as it is now ([`Ui::window_info`], [`WindowRef::info`]).
#[derive(Debug, Clone, PartialEq)]
pub struct WindowInfo {
    pub id: WindowId,
    pub name: String,
    pub kind: WindowKind,
    /// The window it's in (for a child region, popup or menu).
    pub parent: Option<WindowId>,
    /// The window drawn in front with it, which focusing and display order
    /// move: itself, or for a child region or docked window the top-level
    /// window it's in.
    pub root: WindowId,
    /// The dock node it's docked in, or last was.
    pub dock_id: Option<DockId>,
    /// Its top left and size on the screen.
    pub pos: Vec2,
    pub size: Vec2,
    /// The size of what it holds, which it scrolls over.
    pub content_size: Vec2,
    pub scroll: Vec2,
    /// The furthest it scrolls (0 on an axis that doesn't).
    pub scroll_max: Vec2,
    /// Its public flags (what it is is [`WindowInfo::kind`]).
    pub flags: WindowFlags,
    /// Its place in drawing order, 0 at the back.
    pub display_index: usize,
    /// Its place in focus order, 0 the least recently focused; None for
    /// child regions, which aren't in it.
    pub focus_order: Option<usize>,
    /// The last frame it was drawn ([`Ui::frame_count`]'s count).
    pub last_frame_drawn: i32,
    pub collapsed: bool,
    /// Drawn but not shown (sizing itself on its first frame, say).
    pub hidden: bool,
    /// It has keyboard focus ([`Ui::focused_window`]).
    pub focused: bool,
    /// It's under the mouse ([`Ui::hovered_window`]).
    pub hovered: bool,
}

/// Longest window name [`WindowInfo::name`] keeps; ImGui's names are rarely
/// longer.
const MAX_WINDOW_NAME: usize = 512;

/// The internal `ImGuiWindowFlags_` that say what a window is.
const KIND_FLAGS: i32 = sys::ImGuiWindowFlags_ChildWindow
    | sys::ImGuiWindowFlags_Tooltip
    | sys::ImGuiWindowFlags_Popup
    | sys::ImGuiWindowFlags_Modal
    | sys::ImGuiWindowFlags_ChildMenu;

fn kind(flags: i32) -> WindowKind {
    let has = |flag| flags & flag != 0;
    if has(sys::ImGuiWindowFlags_Tooltip) {
        WindowKind::Tooltip
    } else if has(sys::ImGuiWindowFlags_Modal) {
        WindowKind::Modal
    } else if has(sys::ImGuiWindowFlags_ChildMenu) {
        WindowKind::Menu
    } else if has(sys::ImGuiWindowFlags_Popup) {
        WindowKind::Popup
    } else if has(sys::ImGuiWindowFlags_ChildWindow) {
        WindowKind::Child
    } else {
        WindowKind::TopLevel
    }
}

fn some_id(id: u32) -> Option<WindowId> {
    (id != 0).then_some(WindowId(id))
}

fn check_ratio(ratio: f32) {
    assert!((0.0..=1.0).contains(&ratio), "a scroll ratio is 0 to 1, not {ratio}");
}

flags! {
    /// How [`WindowRef::focus`] focuses a window (`ImGuiFocusRequestFlags_`).
    FocusRequestFlags {
        /// Focuses the child region in it that last had focus instead.
        RESTORE_FOCUSED_CHILD = ImGuiFocusRequestFlags_RestoreFocusedChild,
        /// Doesn't while a modal popup is open over it (it's brought to just
        /// under the modal instead).
        UNLESS_BELOW_MODAL = ImGuiFocusRequestFlags_UnlessBelowModal,
    }
}

flags! {
    /// How [`Ui::scroll_to_item`] and [`WindowRef::scroll_to_rect`] bring
    /// something into view (`ImGuiScrollFlags_`): at most one way per axis.
    /// With none on an axis, ImGui scrolls as little as it can (and on a
    /// window just appearing, centers vertically).
    ScrollFlags {
        /// If it's out of view, scrolls as little as brings it in.
        KEEP_VISIBLE_EDGE_X = ImGuiScrollFlags_KeepVisibleEdgeX,
        KEEP_VISIBLE_EDGE_Y = ImGuiScrollFlags_KeepVisibleEdgeY,
        /// If it's out of view, centers it.
        KEEP_VISIBLE_CENTER_X = ImGuiScrollFlags_KeepVisibleCenterX,
        KEEP_VISIBLE_CENTER_Y = ImGuiScrollFlags_KeepVisibleCenterY,
        /// Centers it, in view or not.
        ALWAYS_CENTER_X = ImGuiScrollFlags_AlwaysCenterX,
        ALWAYS_CENTER_Y = ImGuiScrollFlags_AlwaysCenterY,
        /// Scrolls only this window, not the ones it's in.
        NO_SCROLL_PARENT = ImGuiScrollFlags_NoScrollParent,
    }
}

impl ScrollFlags {
    fn check(self) {
        let x = sys::ImGuiScrollFlags_KeepVisibleEdgeX | sys::ImGuiScrollFlags_KeepVisibleCenterX | sys::ImGuiScrollFlags_AlwaysCenterX;
        let y = sys::ImGuiScrollFlags_KeepVisibleEdgeY | sys::ImGuiScrollFlags_KeepVisibleCenterY | sys::ImGuiScrollFlags_AlwaysCenterY;
        for (axis, mask) in [("x", x), ("y", y)] {
            assert!((self.0 & mask).count_ones() <= 1, "ImGui scrolls one way per axis; {self:?} has more than one on {axis}");
        }
    }
}

impl Ui {
    /// The ID of the window named `name` (whether or not there is one); `###`
    /// works as in titles.
    pub fn window_id(&self, name: &str) -> WindowId {
        let (p, n) = s(name);
        WindowId(unsafe { (self.api.mqemu_imgui_window_id)(p, n) })
    }

    /// The window named `name` (its title, or for a child region
    /// `"Parent/child_1234ABCD"`), from any plugin or MacroQuest itself;
    /// None if ImGui has none. ImGui keeps a window once it's been drawn,
    /// shown or not.
    pub fn find_window(&mut self, name: &str) -> Option<WindowRef<'_>> {
        let (p, n) = s(name);
        let id = some_id(unsafe { (self.api.mqemu_imgui_find_window)(p, n) })?;
        Some(WindowRef { ui: self, id })
    }

    /// Window `id`; None if ImGui has none.
    pub fn window_by_id(&mut self, id: WindowId) -> Option<WindowRef<'_>> {
        self.window_info(id)?;
        Some(WindowRef { ui: self, id })
    }

    /// The window being drawn into: a [`Ui::window`]'s or [`Ui::child`]'s
    /// inside it, otherwise ImGui's own fallback window.
    pub fn current_window(&mut self) -> WindowRef<'_> {
        let id = self.window_ids()[0].expect("ImGui always has a current window in a frame");
        WindowRef { ui: self, id }
    }

    fn window_ids(&self) -> [Option<WindowId>; 3] {
        let (mut current, mut focused, mut hovered) = (0, 0, 0);
        unsafe { (self.api.mqemu_imgui_window_ids)(&mut current, &mut focused, &mut hovered) };
        [current, focused, hovered].map(some_id)
    }

    /// The window with keyboard focus, in any plugin.
    pub fn focused_window(&self) -> Option<WindowId> {
        self.window_ids()[1]
    }

    /// The window under the mouse, in any plugin.
    pub fn hovered_window(&self) -> Option<WindowId> {
        self.window_ids()[2]
    }

    /// Describes window `id`; None if ImGui has none.
    pub fn window_info(&self, id: WindowId) -> Option<WindowInfo> {
        let mut info = sys::mqemu_window_info::default();
        let mut name = [0 as c_char; MAX_WINDOW_NAME];
        if id.0 == 0 || !unsafe { (self.api.mqemu_imgui_window)(id.0, &mut info, name.as_mut_ptr(), name.len()) } {
            return None;
        }
        let [_, focused, hovered] = self.window_ids();
        Some(WindowInfo {
            id,
            name: crate::text::from_c_array(&name),
            kind: kind(info.flags),
            parent: some_id(info.parent),
            root: WindowId(info.root),
            dock_id: (info.dock_id != 0).then_some(DockId(info.dock_id)),
            pos: [info.x, info.y],
            size: [info.w, info.h],
            content_size: [info.content_w, info.content_h],
            scroll: [info.scroll_x, info.scroll_y],
            scroll_max: [info.scroll_max_x, info.scroll_max_y],
            flags: WindowFlags(info.flags & !KIND_FLAGS),
            display_index: info.display_index.max(0) as usize,
            focus_order: usize::try_from(info.focus_order).ok(),
            last_frame_drawn: info.last_frame_drawn,
            collapsed: info.collapsed,
            hidden: info.hidden,
            focused: focused == Some(id),
            hovered: hovered == Some(id),
        })
    }

    /// Every window ImGui has, every plugin's and MacroQuest's, child regions
    /// and popups too, in drawing order: back to front.
    pub fn windows(&self) -> Vec<WindowInfo> {
        let count = unsafe { (self.api.mqemu_imgui_window_count)() };
        (0..count).filter_map(|index| some_id(unsafe { (self.api.mqemu_imgui_window_at)(index) })).filter_map(|id| self.window_info(id)).collect()
    }

    /// Takes keyboard focus from every window.
    pub fn clear_window_focus(&mut self) {
        unsafe { (self.api.mqemu_imgui_focus_window)(0, 0) }
    }

    /// Scrolls the current window, from next frame, so the cursor ends up
    /// `ratio` of the way across its view: 0 its left edge, 0.5 the middle,
    /// 1 its right edge.
    ///
    /// # Panics
    /// If `ratio` isn't 0 to 1.
    pub fn set_scroll_here_x(&mut self, ratio: f32) {
        check_ratio(ratio);
        unsafe { (self.api.mqemu_imgui_set_scroll_here)(sys::ImGuiAxis_X, ratio) }
    }

    /// Scrolls the current window, from next frame, so the cursor's line
    /// ends up `ratio` of the way down its view: 0 the top, 0.5 the middle,
    /// 1 the bottom (as a log keeping its newest line in view does).
    ///
    /// # Panics
    /// If `ratio` isn't 0 to 1.
    pub fn set_scroll_here_y(&mut self, ratio: f32) {
        check_ratio(ratio);
        unsafe { (self.api.mqemu_imgui_set_scroll_here)(sys::ImGuiAxis_Y, ratio) }
    }

    /// Scrolls the current window (and the windows it's in, unless
    /// [`ScrollFlags::NO_SCROLL_PARENT`]), from next frame, to bring the last
    /// item into view.
    ///
    /// # Panics
    /// If `flags` has more than one way to scroll an axis.
    pub fn scroll_to_item(&mut self, flags: ScrollFlags) {
        flags.check();
        unsafe { (self.api.mqemu_imgui_scroll_to_item)(flags.bits()) }
    }
}

/// A window ImGui has, found with [`Ui::find_window`], [`Ui::window_by_id`]
/// or [`Ui::current_window`]. What it changes takes effect when the window is
/// next drawn (this frame, if it hasn't been yet).
pub struct WindowRef<'ui> {
    ui: &'ui mut Ui,
    id: WindowId,
}

impl WindowRef<'_> {
    pub fn id(&self) -> WindowId {
        self.id
    }

    pub fn info(&self) -> WindowInfo {
        // ImGui never deletes a window while its context lives, and the
        // context outlives the frame this borrows.
        self.ui.window_info(self.id).expect("ImGui keeps its windows")
    }

    /// Moves its top left to `pos` on the screen (a docked window or child
    /// region is placed by what it's in instead).
    pub fn set_pos(&mut self, pos: Vec2, cond: Cond) {
        unsafe { (self.ui.api.mqemu_imgui_set_window_pos_by_id)(self.id.0, pos[0], pos[1], cond as i32) }
    }

    /// `[w, h]`; 0 on an axis fits the contents.
    pub fn set_size(&mut self, size: Vec2, cond: Cond) {
        unsafe { (self.ui.api.mqemu_imgui_set_window_size_by_id)(self.id.0, size[0], size[1], cond as i32) }
    }

    pub fn set_collapsed(&mut self, collapsed: bool, cond: Cond) {
        unsafe { (self.ui.api.mqemu_imgui_set_window_collapsed_by_id)(self.id.0, collapsed, cond as i32) }
    }

    /// Gives it keyboard focus and brings it to the front (unless it has
    /// [`WindowFlags::NO_BRING_TO_FRONT_ON_FOCUS`]), closing popups over
    /// other windows, as clicking it does.
    pub fn focus(&mut self, flags: FocusRequestFlags) {
        unsafe { (self.ui.api.mqemu_imgui_focus_window)(self.id.0, flags.bits()) }
    }

    /// Draws it (with [`WindowInfo::root`], the window it's in) over every
    /// other window, without focusing it. Popups and tooltips still draw
    /// over it.
    pub fn bring_to_front(&mut self) {
        unsafe { (self.ui.api.mqemu_imgui_bring_window_to_front)(self.id.0) }
    }

    /// Draws it (with its root) under every other window.
    pub fn bring_to_back(&mut self) {
        unsafe { (self.ui.api.mqemu_imgui_bring_window_to_back)(self.id.0) }
    }

    /// Draws it (with its root) just under `other`'s root. Nothing happens if
    /// there's no window `other`.
    pub fn bring_behind(&mut self, other: WindowId) {
        if self.ui.window_info(other).is_some() {
            unsafe { (self.ui.api.mqemu_imgui_bring_window_behind)(self.id.0, other.0) }
        }
    }

    /// Whether it's drawn over `other` (false if there's no window `other`).
    pub fn is_above(&self, other: WindowId) -> bool {
        self.ui.window_info(other).is_some() && unsafe { (self.ui.api.mqemu_imgui_is_window_above)(self.id.0, other.0) }
    }

    /// Scrolls it to `x` across, from next frame.
    pub fn set_scroll_x(&mut self, x: f32) {
        unsafe { (self.ui.api.mqemu_imgui_set_window_scroll)(self.id.0, sys::ImGuiAxis_X, x) }
    }

    /// Scrolls it to `y` down, from next frame.
    pub fn set_scroll_y(&mut self, y: f32) {
        unsafe { (self.ui.api.mqemu_imgui_set_window_scroll)(self.id.0, sys::ImGuiAxis_Y, y) }
    }

    /// Scrolls it, from next frame, so `x` (from its left edge, as the
    /// cursor's position in it is) ends up `ratio` of the way across its view.
    ///
    /// # Panics
    /// If `ratio` isn't 0 to 1.
    pub fn set_scroll_from_pos_x(&mut self, x: f32, ratio: f32) {
        check_ratio(ratio);
        unsafe { (self.ui.api.mqemu_imgui_set_window_scroll_from_pos)(self.id.0, sys::ImGuiAxis_X, x, ratio) }
    }

    /// Scrolls it, from next frame, so `y` (from its top) ends up `ratio` of
    /// the way down its view.
    ///
    /// # Panics
    /// If `ratio` isn't 0 to 1.
    pub fn set_scroll_from_pos_y(&mut self, y: f32, ratio: f32) {
        check_ratio(ratio);
        unsafe { (self.ui.api.mqemu_imgui_set_window_scroll_from_pos)(self.id.0, sys::ImGuiAxis_Y, y, ratio) }
    }

    /// Scrolls it (and the windows it's in, unless
    /// [`ScrollFlags::NO_SCROLL_PARENT`]), from next frame, to bring
    /// `min`..`max` (screen coordinates) into view. Returns how far it will
    /// scroll.
    ///
    /// # Panics
    /// If `flags` has more than one way to scroll an axis.
    pub fn scroll_to_rect(&mut self, min: Vec2, max: Vec2, flags: ScrollFlags) -> Vec2 {
        flags.check();
        let (mut dx, mut dy) = (0.0, 0.0);
        unsafe { (self.ui.api.mqemu_imgui_scroll_to_rect)(self.id.0, min[0], min[1], max[0], max[1], flags.bits(), &mut dx, &mut dy) };
        [dx, dy]
    }
}
