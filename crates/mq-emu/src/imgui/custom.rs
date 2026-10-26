//! Custom widgets, made the way ImGui makes its own (from `imgui_internal.h`):
//! lay out an item and give it an ID ([`Ui::add_item`]), ask how the mouse
//! treats it ([`Ui::button_behavior`]), and draw it with the style's frames
//! and text ([`Ui::render_frame`], [`Ui::render_text_clipped`]) or any
//! [`DrawList`](super::DrawList).
//!
//! ```no_run
//! # fn draw(ui: &mut mq_emu::Ui, on: &mut bool) {
//! use mq_emu::imgui::{ButtonFlags, StyleColor};
//! // A toggle switch.
//! let height = ui.frame_height();
//! if let Some(item) = ui.add_item("switch", [height * 1.8, height]) {
//!     if ui.button_behavior(&item, ButtonFlags::NONE).pressed {
//!         *on = !*on;
//!         ui.mark_item_edited(item.id);
//!     }
//!     let track = ui.style_color(if *on { StyleColor::ButtonActive } else { StyleColor::FrameBg });
//!     ui.render_frame(item.min, item.max, track, true, height / 2.0);
//!     let x = if *on { item.max[0] - height / 2.0 } else { item.min[0] + height / 2.0 };
//!     ui.draw_list().add_circle_filled([x, item.min[1] + height / 2.0], height / 2.0 - 2.0, [1.0; 4]);
//! }
//! # }
//! ```

use mq_emu_sys::imgui as sys;

use super::{ButtonFlags, Color, Dir, Ui, Vec2, pack_color, s};

/// An item's ID (`ImGuiID`), from its label and the ID stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ItemId(pub u32);

/// An item added with [`Ui::add_item`]: its ID and rectangle on the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Item {
    pub id: ItemId,
    pub min: Vec2,
    pub max: Vec2,
}

/// How the mouse treats an item, from [`Ui::button_behavior`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ButtonState {
    /// Pressed this frame, as its flags say (by default, clicked and released on it).
    pub pressed: bool,
    pub hovered: bool,
    /// The mouse went down on it and hasn't been released.
    pub held: bool,
}

/// Which way a [`Ui::splitter`] divides its space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Split {
    /// Side by side, with an upright bar dragged left and right.
    Columns,
    /// One above the other, with a flat bar dragged up and down.
    Rows,
}

fn id(n: Option<ItemId>) -> Option<ItemId> {
    n.filter(|id| id.0 != 0)
}

impl Ui {
    /// The ID a widget labeled `label` would have here (`##` and `###` work
    /// as in labels).
    pub fn item_id(&self, label: &str) -> ItemId {
        let (p, n) = s(label);
        ItemId(unsafe { (self.api.mqemu_imgui_get_id)(p, n) })
    }

    /// Lays out an item of `size`, `[w, h]`, at the cursor, with the ID of
    /// `label`, as ImGui's widgets do: it takes its space, is the last item
    /// (for [`Ui::is_item_hovered`], tooltips and the like) and can be
    /// navigated to. None if it's clipped (out of view), when there's no need
    /// to draw it.
    pub fn add_item(&mut self, label: &str, size: Vec2) -> Option<Item> {
        let id = self.item_id(label);
        let (mut x0, mut y0, mut x1, mut y1) = (0.0, 0.0, 0.0, 0.0);
        let visible = unsafe { (self.api.mqemu_imgui_item_add)(size[0], size[1], id.0, &mut x0, &mut y0, &mut x1, &mut y1) };
        visible.then_some(Item { id, min: [x0, y0], max: [x1, y1] })
    }

    /// How the mouse treats `item`: pressed (when, `flags` say), hovered,
    /// held. Call it once a frame, before drawing the item in its state.
    pub fn button_behavior(&mut self, item: &Item, flags: ButtonFlags) -> ButtonState {
        let mut state = ButtonState::default();
        let ([x0, y0], [x1, y1]) = (item.min, item.max);
        state.pressed =
            unsafe { (self.api.mqemu_imgui_button_behavior)(x0, y0, x1, y1, item.id.0, flags.bits(), &mut state.hovered, &mut state.held) };
        state
    }

    fn item_ids(&self) -> [Option<ItemId>; 3] {
        let (mut active, mut hovered, mut focus) = (0, 0, 0);
        unsafe { (self.api.mqemu_imgui_item_ids)(&mut active, &mut hovered, &mut focus) };
        [active, hovered, focus].map(|n| id(Some(ItemId(n))))
    }

    /// The item being interacted with (clicked and held, or edited), in any
    /// window.
    pub fn active_id(&self) -> Option<ItemId> {
        self.item_ids()[0]
    }

    /// The item under the mouse.
    pub fn hovered_id(&self) -> Option<ItemId> {
        self.item_ids()[1]
    }

    /// The item with keyboard navigation focus.
    pub fn focus_id(&self) -> Option<ItemId> {
        self.item_ids()[2]
    }

    /// Lets go of the active item: ends a drag, say.
    pub fn clear_active_id(&mut self) {
        unsafe { (self.api.mqemu_imgui_clear_active_id)() }
    }

    /// Keeps `id` active or hovered on a frame its item isn't drawn.
    pub fn keep_alive_id(&mut self, id: ItemId) {
        unsafe { (self.api.mqemu_imgui_keep_alive_id)(id.0) }
    }

    /// Marks the item's value as changed, as ImGui's widgets do when theirs
    /// is (ImGui's `IsItemEdited` and `IsItemDeactivatedAfterEdit` report it).
    pub fn mark_item_edited(&mut self, id: ItemId) {
        unsafe { (self.api.mqemu_imgui_mark_item_edited)(id.0) }
    }

    /// A bar at `min`..`max` (screen coordinates) between two parts of
    /// `sizes`, `[before, after]` the bar: dragging it moves size from one to
    /// the other, keeping each at least `min_sizes`. It's an item, but
    /// doesn't move the cursor: lay out the parts around it. True while it's
    /// dragged.
    ///
    /// ```no_run
    /// # fn draw(ui: &mut mq_emu::Ui, sizes: &mut [f32; 2]) {
    /// use mq_emu::imgui::Split;
    /// let [x, y] = ui.cursor_screen_pos();
    /// let height = 200.0;
    /// ui.splitter("##split", Split::Columns, [x + sizes[0], y], [x + sizes[0] + 4.0, y + height], sizes, [40.0, 40.0]);
    /// ui.child("left").size([sizes[0], height]).show(|ui| ui.text("left"));
    /// ui.same_line_with(0.0, 4.0);
    /// ui.child("right").size([sizes[1], height]).show(|ui| ui.text("right"));
    /// # }
    /// ```
    pub fn splitter(&mut self, label: &str, split: Split, min: Vec2, max: Vec2, sizes: &mut [f32; 2], min_sizes: [f32; 2]) -> bool {
        let id = self.item_id(label);
        let axis = match split {
            Split::Columns => sys::ImGuiAxis_X,
            Split::Rows => sys::ImGuiAxis_Y,
        };
        let [size1, size2] = sizes;
        unsafe {
            (self.api.mqemu_imgui_splitter)(min[0], min[1], max[0], max[1], id.0, axis, size1, size2, min_sizes[0], min_sizes[1], 0.0, 0.0)
        }
    }

    /// A frame in the current window, as widgets draw theirs: filled with
    /// `fill`, and the style's frame border if `borders` (and it has one).
    pub fn render_frame(&self, min: Vec2, max: Vec2, fill: Color, borders: bool, rounding: f32) {
        unsafe { (self.api.mqemu_imgui_render_frame)(min[0], min[1], max[0], max[1], pack_color(fill), borders, rounding) }
    }

    /// Just the style's frame border (if it has one).
    pub fn render_frame_border(&self, min: Vec2, max: Vec2, rounding: f32) {
        unsafe { (self.api.mqemu_imgui_render_frame_border)(min[0], min[1], max[0], max[1], rounding) }
    }

    /// The keyboard navigation cursor around `item`, when it has focus.
    pub fn render_nav_cursor(&self, item: &Item) {
        unsafe { (self.api.mqemu_imgui_render_nav_cursor)(item.min[0], item.min[1], item.max[0], item.max[1], item.id.0) }
    }

    /// `text` in the style's text color, clipped to `min`..`max` and aligned
    /// in it by `align` (0 to 1 on each axis: `[0.5, 0.5]` centers it). Stops
    /// at a `##`, as labels do.
    pub fn render_text_clipped(&self, min: Vec2, max: Vec2, text: &str, align: Vec2) {
        let (p, n) = s(text);
        unsafe { (self.api.mqemu_imgui_render_text_clipped)(min[0], min[1], max[0], max[1], p, n, align[0], align[1]) }
    }

    /// [`Ui::button_sized`] with flags, ImGui's private ones included
    /// (`ButtonEx`).
    pub fn button_with(&mut self, label: &str, size: Vec2, flags: ButtonFlags) -> bool {
        let (p, n) = s(label);
        unsafe { (self.api.mqemu_imgui_button_ex)(p, n, size[0], size[1], flags.bits()) }
    }

    /// A button of `size` with an arrow pointing `dir` (`ArrowButtonEx`).
    pub fn arrow_button_with(&mut self, id: &str, dir: Dir, size: Vec2, flags: ButtonFlags) -> bool {
        let (p, n) = s(id);
        unsafe { (self.api.mqemu_imgui_arrow_button_ex)(p, n, dir as i32, size[0], size[1], flags.bits()) }
    }

    /// An upright separator, the line's height: between items on one line
    /// ([`Ui::same_line`]).
    pub fn vertical_separator(&mut self) {
        unsafe { (self.api.mqemu_imgui_separator_ex)(true, 1.0) }
    }

    /// [`Ui::separator`] (or with `vertical`, [`Ui::vertical_separator`])
    /// `thickness` pixels thick.
    ///
    /// # Panics
    /// If `thickness` isn't more than 0.
    pub fn separator_with(&mut self, vertical: bool, thickness: f32) {
        assert!(thickness > 0.0, "a separator's thickness must be more than 0, not {thickness}");
        unsafe { (self.api.mqemu_imgui_separator_ex)(vertical, thickness) }
    }
}
