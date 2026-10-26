//! Multi-select: lists whose items the user selects several of at once, with
//! ctrl+click, shift+click, ctrl+A, the keyboard and (optionally) dragging a
//! box.
//!
//! Items are identified by their index in the list. ImGui asks for changes as
//! [`SelectionRequest`]s, once as the list begins and again as it ends;
//! [`Ui::multi_select`] applies them to a [`Selection`], and
//! [`Ui::multi_select_io`] hands them over for a selection kept some other way.

use std::collections::BTreeSet;
use std::ops::{Deref, DerefMut, RangeInclusive};

use mq_emu_sys::imgui as sys;

use super::Ui;

flags! {
    /// Options for [`Ui::multi_select`] (`ImGuiMultiSelectFlags_`).
    MultiSelectFlags {
        /// Only one item at a time (the same code, without the multi).
        SINGLE_SELECT = ImGuiMultiSelectFlags_SingleSelect,
        /// No ctrl+A to select everything.
        NO_SELECT_ALL = ImGuiMultiSelectFlags_NoSelectAll,
        /// No shift+click or shift+arrow ranges.
        NO_RANGE_SELECT = ImGuiMultiSelectFlags_NoRangeSelect,
        /// Moving with the keyboard doesn't select (say, for a list of checkboxes).
        NO_AUTO_SELECT = ImGuiMultiSelectFlags_NoAutoSelect,
        /// Selecting doesn't clear what else was selected (usually with NO_AUTO_SELECT).
        NO_AUTO_CLEAR = ImGuiMultiSelectFlags_NoAutoClear,
        /// Clicking a selected item doesn't clear the others.
        NO_AUTO_CLEAR_ON_RESELECT = ImGuiMultiSelectFlags_NoAutoClearOnReselect,
        /// Dragging from empty space selects items in a box: for full-width
        /// items lined up (selectables in rows).
        BOX_SELECT_1D = ImGuiMultiSelectFlags_BoxSelect1d,
        /// Box selection for items of any width and position (a grid); slower.
        BOX_SELECT_2D = ImGuiMultiSelectFlags_BoxSelect2d,
        /// Box selection doesn't scroll near the edges.
        BOX_SELECT_NO_SCROLL = ImGuiMultiSelectFlags_BoxSelectNoScroll,
        /// Escape clears the selection.
        CLEAR_ON_ESCAPE = ImGuiMultiSelectFlags_ClearOnEscape,
        /// Clicking empty space clears the selection.
        CLEAR_ON_CLICK_VOID = ImGuiMultiSelectFlags_ClearOnClickVoid,
        /// Box selection and clearing on clicking empty space work over the
        /// whole window (the default).
        SCOPE_WINDOW = ImGuiMultiSelectFlags_ScopeWindow,
        /// ... or only over the list: for several lists in one window.
        SCOPE_RECT = ImGuiMultiSelectFlags_ScopeRect,
        /// Selects an unselected item when the mouse goes down on it (the default).
        SELECT_ON_CLICK = ImGuiMultiSelectFlags_SelectOnClick,
        /// ... or when it's released, so it can be dragged without selecting it.
        SELECT_ON_CLICK_RELEASE = ImGuiMultiSelectFlags_SelectOnClickRelease,
        /// Left and right arrows wrap around rows (only with SCOPE_WINDOW).
        NAV_WRAP_X = ImGuiMultiSelectFlags_NavWrapX,
        /// Right-clicking doesn't select (as it does for context menus).
        NO_SELECT_ON_RIGHT_CLICK = ImGuiMultiSelectFlags_NoSelectOnRightClick,
    }
}

/// A change ImGui asks for in a selection (`ImGuiSelectionRequest`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionRequest {
    /// Select every item, or (`selected` false) none.
    SetAll { selected: bool },
    /// Select (or unselect) the items in `range`. `backwards` if the user
    /// selected from its end to its start (shift+click upwards).
    SetRange { range: RangeInclusive<usize>, selected: bool, backwards: bool },
}

/// What a multi-select list's beginning or end reports (`ImGuiMultiSelectIO`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MultiSelectIo {
    /// The changes to make, in order.
    pub requests: Vec<SelectionRequest>,
    /// The item shift+click selects from (usually the first selected), if
    /// any. When drawing only the visible part of a long list, draw it too.
    pub range_src_item: Option<usize>,
    /// The item with keyboard focus, if any, and whether it's selected.
    pub nav_id_item: Option<usize>,
    pub nav_id_selected: bool,
    /// The item count the list was given.
    pub items_count: Option<usize>,
}

fn index(item: i64) -> Option<usize> {
    usize::try_from(item).ok()
}

/// A count or size for ImGui: -1 for unknown.
fn count(n: Option<usize>) -> i32 {
    n.map_or(-1, |n| i32::try_from(n).unwrap_or(i32::MAX))
}

impl MultiSelectIo {
    /// Copies what `io` holds (it's ImGui's until the next list begins).
    fn read(api: &sys::Api, io: *const sys::ImGuiMultiSelectIO) -> MultiSelectIo {
        if io.is_null() {
            return MultiSelectIo::default();
        }
        let (mut src, mut nav, mut nav_selected, mut items) = (0, 0, false, 0);
        let requests = unsafe { (api.mqemu_imgui_multi_select_io)(io, &mut src, &mut nav, &mut nav_selected, &mut items) };
        let requests = (0..requests)
            .filter_map(|i| {
                let (mut kind, mut selected, mut direction, mut first, mut last) = (0, false, 0, 0, 0);
                unsafe { (api.mqemu_imgui_multi_select_request)(io, i, &mut kind, &mut selected, &mut direction, &mut first, &mut last) };
                match kind {
                    sys::ImGuiSelectionRequestType_SetAll => Some(SelectionRequest::SetAll { selected }),
                    sys::ImGuiSelectionRequestType_SetRange => {
                        Some(SelectionRequest::SetRange { range: index(first)?..=index(last)?, selected, backwards: direction < 0 })
                    }
                    _ => None,
                }
            })
            .collect();
        MultiSelectIo {
            requests,
            range_src_item: index(src),
            nav_id_item: index(nav),
            nav_id_selected: nav_selected,
            items_count: usize::try_from(items).ok(),
        }
    }

    /// Applies the requests through `set(index, selected)`, as ImGui's
    /// `ImGuiSelectionExternalStorage` does: for a selection kept in the
    /// items themselves, say. Selecting or clearing all goes through every
    /// item below [`MultiSelectIo::items_count`] (none if it's unknown).
    pub fn apply_with(&self, mut set: impl FnMut(usize, bool)) {
        for request in &self.requests {
            match request {
                SelectionRequest::SetAll { selected } => (0..self.items_count.unwrap_or(0)).for_each(|i| set(i, *selected)),
                SelectionRequest::SetRange { range, selected, .. } => range.clone().for_each(|i| set(i, *selected)),
            }
        }
    }
}

/// The items selected in a list, by index, as ImGui's
/// `ImGuiSelectionBasicStorage` keeps them; for [`Ui::multi_select`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    items: BTreeSet<usize>,
}

impl Selection {
    pub fn new() -> Selection {
        Selection::default()
    }

    pub fn contains(&self, index: usize) -> bool {
        self.items.contains(&index)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The selected indices, in order.
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.items.iter().copied()
    }

    pub fn set(&mut self, index: usize, selected: bool) {
        if selected {
            self.items.insert(index);
        } else {
            self.items.remove(&index);
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Keeps only the indices `keep` says to (after items are removed, say).
    pub fn retain(&mut self, keep: impl FnMut(&usize) -> bool) {
        self.items.retain(keep);
    }

    /// Applies ImGui's requests: selecting all selects every item below
    /// [`MultiSelectIo::items_count`].
    pub fn apply(&mut self, io: &MultiSelectIo) {
        for request in &io.requests {
            match request {
                SelectionRequest::SetAll { selected } => {
                    self.items.clear();
                    if *selected {
                        self.items.extend(0..io.items_count.unwrap_or(0));
                    }
                }
                SelectionRequest::SetRange { range, selected, .. } => range.clone().for_each(|i| self.set(i, *selected)),
            }
        }
    }
}

impl FromIterator<usize> for Selection {
    fn from_iter<I: IntoIterator<Item = usize>>(iter: I) -> Selection {
        Selection { items: iter.into_iter().collect() }
    }
}

impl Ui {
    /// A list of `items_count` items the user can select several of, kept in
    /// `selection`: `f` draws the items (giving each its index with
    /// [`MultiSelect::set_next_item_index`]) with the selection as it is
    /// then, and the user's changes are applied as it ends. None, without
    /// calling `f`, if ImGui refused (an ImGui error).
    ///
    /// ```no_run
    /// # fn draw(ui: &mut mq_emu::Ui, names: &[String], selection: &mut mq_emu::imgui::Selection) {
    /// use mq_emu::imgui::MultiSelectFlags;
    /// ui.multi_select(MultiSelectFlags::BOX_SELECT_1D, selection, names.len(), |list, selection| {
    ///     for (index, name) in names.iter().enumerate() {
    ///         list.set_next_item_index(index);
    ///         list.selectable(name, selection.contains(index));
    ///     }
    /// });
    /// ui.text(format!("{} selected", selection.len()));
    /// # }
    /// ```
    pub fn multi_select<R>(
        &mut self,
        flags: MultiSelectFlags,
        selection: &mut Selection,
        items_count: usize,
        f: impl FnOnce(&mut MultiSelect<'_>, &Selection) -> R,
    ) -> Option<R> {
        let mut list = self.begin_multi_select(flags, Some(selection.len()), Some(items_count))?;
        selection.apply(&list.begin);
        let result = f(&mut list, selection);
        selection.apply(&list.end());
        Some(result)
    }

    /// [`Ui::multi_select`] for a selection kept another way: `f` gets the
    /// list (with the requests to apply before drawing in
    /// [`MultiSelect::begin_io`]); returns `f`'s result and the requests to
    /// apply after. The selection's size and the item count are optional
    /// (None for unknown), but selecting all needs the count.
    pub fn multi_select_io<R>(
        &mut self,
        flags: MultiSelectFlags,
        selection_size: Option<usize>,
        items_count: Option<usize>,
        f: impl FnOnce(&mut MultiSelect<'_>) -> R,
    ) -> Option<(R, MultiSelectIo)> {
        let mut list = self.begin_multi_select(flags, selection_size, items_count)?;
        let result = f(&mut list);
        Some((result, list.end()))
    }

    fn begin_multi_select(&mut self, flags: MultiSelectFlags, selection_size: Option<usize>, items_count: Option<usize>) -> Option<MultiSelect<'_>> {
        let io = unsafe { (self.api.mqemu_imgui_begin_multi_select)(flags.bits(), count(selection_size), count(items_count)) };
        if io.is_null() {
            return None;
        }
        let begin = MultiSelectIo::read(self.api, io);
        Some(MultiSelect { ui: self, begin, ended: false })
    }
}

/// A multi-select list while its items are drawn ([`Ui::multi_select`]).
/// Draws like a [`Ui`] (through `Deref`).
pub struct MultiSelect<'ui> {
    ui: &'ui mut Ui,
    begin: MultiSelectIo,
    ended: bool,
}

impl MultiSelect<'_> {
    /// The requests ImGui made as the list began (already applied by
    /// [`Ui::multi_select`]).
    pub fn begin_io(&self) -> &MultiSelectIo {
        &self.begin
    }

    /// Gives the next item (a selectable, a tree node) its index in the list.
    pub fn set_next_item_index(&mut self, index: usize) {
        let data = i64::try_from(index).unwrap_or(i64::MAX);
        unsafe { (self.ui.api.mqemu_imgui_set_next_item_selection_user_data)(data) }
    }

    /// Whether the last item's selection was toggled this frame, before the
    /// list ends and the change is applied.
    pub fn is_item_toggled_selection(&self) -> bool {
        unsafe { (self.ui.api.mqemu_imgui_is_item_toggled_selection)() }
    }

    /// Forgets the item shift+click selects from: when it was deleted, say.
    pub fn reset_range_src(&mut self) {
        unsafe { (self.ui.api.mqemu_imgui_multi_select_reset_range_src)() }
    }

    fn end(&mut self) -> MultiSelectIo {
        self.ended = true;
        let io = unsafe { (self.ui.api.mqemu_imgui_end_multi_select)() };
        MultiSelectIo::read(self.ui.api, io)
    }
}

impl Drop for MultiSelect<'_> {
    /// Ends the list if its closure panicked.
    fn drop(&mut self) {
        if !self.ended {
            self.end();
        }
    }
}

impl Deref for MultiSelect<'_> {
    type Target = Ui;
    fn deref(&self) -> &Ui {
        self.ui
    }
}

impl DerefMut for MultiSelect<'_> {
    fn deref_mut(&mut self) -> &mut Ui {
        self.ui
    }
}
