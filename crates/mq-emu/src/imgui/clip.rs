//! The list clipper: long lists that only draw the items in view.
//!
//! ImGui already skips drawing what's out of view, but only after laying it
//! out; a list of thousands of items still costs that much each frame. Given
//! the item count and evenly spaced items, the clipper works out which ones
//! are visible and moves the cursor past the rest. It draws the first item to
//! measure the spacing unless told it.

use std::ops::Range;

use mq_emu_sys::imgui as sys;

use super::{MultiSelect, OnDrop, Table, Ui};

/// A clipped list being configured; see [`Ui::list_clipper`].
#[must_use = "a list clipper draws by calling show"]
pub struct ListClipper<'s, S> {
    scope: &'s mut S,
    api: &'static sys::Api,
    items_count: usize,
    items_height: f32,
    flags: i32,
    include: Vec<Range<usize>>,
}

impl<'s, S> ListClipper<'s, S> {
    fn new(scope: &'s mut S, api: &'static sys::Api, items_count: usize) -> Self {
        ListClipper { scope, api, items_count, items_height: 0.0, flags: sys::ImGuiListClipperFlags_None, include: Vec::new() }
    }

    /// The distance from one item to the next (usually
    /// [`Ui::text_line_height_with_spacing`] or
    /// [`Ui::frame_height_with_spacing`]); unset or 0, the first item is
    /// drawn to measure it.
    pub fn items_height(mut self, height: f32) -> Self {
        self.items_height = height;
        self
    }

    /// Always draws the items in `range`, visible or not: one that's being
    /// edited or dragged, say.
    pub fn include(mut self, range: Range<usize>) -> Self {
        if range.start < range.end {
            self.include.push(range);
        }
        self
    }

    /// In a table, for items that aren't one row each: stops the clipper
    /// counting skipped items as rows (which alternate row colors use).
    pub fn items_are_not_rows(mut self) -> Self {
        self.flags |= sys::ImGuiListClipperFlags_NoSetTableRowCounters;
        self
    }

    /// Draws the list: calls `f` with each range of items to draw, until
    /// every visible item is drawn, then moves the cursor past the end of the
    /// list. `f` must draw each item it's given, every item the same height
    /// (the first one has to move the cursor down, or the clipper stops with
    /// an ImGui error).
    pub fn show(self, mut f: impl FnMut(&mut S, Range<usize>)) {
        let api = self.api;
        let count = i32::try_from(self.items_count).unwrap_or(i32::MAX - 1).min(i32::MAX - 1);
        let clipper = unsafe { (api.mqemu_imgui_list_clipper_begin)(count, self.items_height, self.flags) };
        if clipper.is_null() {
            return;
        }
        let _end = OnDrop(|| unsafe { (api.mqemu_imgui_list_clipper_end)(clipper) });
        let index = |i: usize| i32::try_from(i).unwrap_or(i32::MAX);
        for range in &self.include {
            unsafe { (api.mqemu_imgui_list_clipper_include)(clipper, index(range.start), index(range.end)) }
        }
        let (mut start, mut end) = (0, 0);
        while unsafe { (api.mqemu_imgui_list_clipper_step)(clipper, &mut start, &mut end) } {
            f(self.scope, start as usize..end as usize);
        }
    }
}

impl Ui {
    /// A list of `items_count` evenly spaced items, drawing only those in
    /// view; configure it, then [`ListClipper::show`] the items.
    ///
    /// ```no_run
    /// # fn draw(ui: &mut mq_emu::Ui, lines: &[String]) {
    /// ui.child("log").show(|ui| {
    ///     ui.list_clipper(lines.len()).show(|ui, visible| {
    ///         for line in &lines[visible] {
    ///             ui.text(line);
    ///         }
    ///     });
    /// });
    /// # }
    /// ```
    ///
    /// Tables and multi-select lists have their own, whose closures get the
    /// table ([`Table::list_clipper`]) or list ([`MultiSelect::list_clipper`]).
    pub fn list_clipper(&mut self, items_count: usize) -> ListClipper<'_, Ui> {
        let api = self.api;
        ListClipper::new(self, api, items_count)
    }
}

impl<'ui> Table<'ui> {
    /// [`Ui::list_clipper`] over the table's rows: draw one row per item.
    ///
    /// ```no_run
    /// # fn draw(ui: &mut mq_emu::Ui, names: &[String]) {
    /// ui.table("names", 2).outer_size([0.0, 300.0]).flags(mq_emu::imgui::TableFlags::SCROLL_Y).show(|table| {
    ///     table.list_clipper(names.len()).show(|table, rows| {
    ///         for i in rows {
    ///             table.row(&[i.to_string(), names[i].clone()]);
    ///         }
    ///     });
    /// });
    /// # }
    /// ```
    pub fn list_clipper(&mut self, items_count: usize) -> ListClipper<'_, Table<'ui>> {
        self.rows_started = true;
        let api = self.ui.api;
        ListClipper::new(self, api, items_count)
    }
}

impl<'ui> MultiSelect<'ui> {
    /// [`Ui::list_clipper`] over the list's items. The item shift+click
    /// selects from ([`MultiSelectIo::range_src_item`](super::MultiSelectIo::range_src_item))
    /// is always drawn, as ImGui needs.
    pub fn list_clipper(&mut self, items_count: usize) -> ListClipper<'_, MultiSelect<'ui>> {
        let api = self.api;
        let src = self.begin_io().range_src_item;
        let clipper = ListClipper::new(self, api, items_count);
        match src {
            Some(src) => clipper.include(src..src + 1),
            None => clipper,
        }
    }
}
