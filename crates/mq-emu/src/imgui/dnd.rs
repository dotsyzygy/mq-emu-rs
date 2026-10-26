//! Drag and drop between ImGui items (and plots), carrying bytes.

use std::ffi::{c_char, c_void};
use std::ops::{Deref, DerefMut};

use mq_emu_sys::imgui as sys;

use super::{EndOnDrop, Ui, s};

flags! {
    /// Options for drag sources ([`Ui::drag_drop_source_with`]) and accepting
    /// payloads ([`DropTarget::accept_with`]) (`ImGuiDragDropFlags_`).
    DragDropFlags {
        /// No tooltip by the mouse while dragging.
        SOURCE_NO_PREVIEW_TOOLTIP = ImGuiDragDropFlags_SourceNoPreviewTooltip,
        /// Keep reporting the source item as hovered while dragging it.
        SOURCE_NO_DISABLE_HOVER = ImGuiDragDropFlags_SourceNoDisableHover,
        /// Don't open tree nodes and headers held over while dragging.
        SOURCE_NO_HOLD_TO_OPEN_OTHERS = ImGuiDragDropFlags_SourceNoHoldToOpenOthers,
        /// Let items without an ID (text, images) be dragged.
        SOURCE_ALLOW_NULL_ID = ImGuiDragDropFlags_SourceAllowNullID,
        /// Drop the payload when the source stops being drawn.
        PAYLOAD_AUTO_EXPIRE = ImGuiDragDropFlags_PayloadAutoExpire,
        /// Accept before the mouse is released ([`Payload::is_delivery`] says when it is).
        ACCEPT_BEFORE_DELIVERY = ImGuiDragDropFlags_AcceptBeforeDelivery,
        /// Don't highlight the target while a payload hovers it.
        ACCEPT_NO_DRAW_DEFAULT_RECT = ImGuiDragDropFlags_AcceptNoDrawDefaultRect,
        /// Hide the source's tooltip over this target.
        ACCEPT_NO_PREVIEW_TOOLTIP = ImGuiDragDropFlags_AcceptNoPreviewTooltip,
        /// Draw the accepting item as hovered.
        ACCEPT_DRAW_AS_HOVERED = ImGuiDragDropFlags_AcceptDrawAsHovered,
        /// Look at a payload before it's dropped, without highlighting.
        ACCEPT_PEEK_ONLY = ImGuiDragDropFlags_AcceptPeekOnly,
    }
}

/// Most bytes a payload's kind may have.
pub const MAX_PAYLOAD_KIND: usize = 32;

/// Panics unless `kind` is a payload kind ImGui can hold.
pub(super) fn check_kind(kind: &str) {
    assert!(
        !kind.is_empty() && kind.len() <= MAX_PAYLOAD_KIND && !kind.starts_with('_') && !kind.contains('\0'),
        "a drag and drop payload kind is 1 to {MAX_PAYLOAD_KIND} bytes, without NUL, not starting with '_' (ImGui's own): {kind:?}"
    );
}

/// What's being dragged: bytes, of a kind the source named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Payload<'a> {
    data: &'a [u8],
    preview: bool,
    delivery: bool,
}

impl<'a> Payload<'a> {
    /// The bytes the source set (ImGui's copy).
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The data as text, if it's UTF-8.
    pub fn as_str(&self) -> Option<&'a str> {
        std::str::from_utf8(self.data).ok()
    }

    /// Hovering over the target, not dropped yet.
    pub fn is_preview(&self) -> bool {
        self.preview
    }

    /// Dropped on the target this frame.
    pub fn is_delivery(&self) -> bool {
        self.delivery
    }

    /// # Safety
    /// `data` must point to `size` bytes valid for `'a` (or be null).
    unsafe fn new(data: *const c_void, size: usize, preview: bool, delivery: bool) -> Payload<'a> {
        let data = if data.is_null() || size == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(data.cast(), size) } };
        Payload { data, preview, delivery }
    }
}

/// A drag in progress from the last item; see [`Ui::drag_drop_source`].
/// Draws the tooltip by the mouse like a [`Ui`] (through `Deref`).
pub struct DragSource<'ui> {
    ui: &'ui mut Ui,
}

impl<'ui> DragSource<'ui> {
    /// One outside a drag, for checks that panic before reaching ImGui.
    #[cfg(test)]
    pub(crate) fn for_test(ui: &'ui mut Ui) -> Self {
        DragSource { ui }
    }

    /// Sets what's dragged: `data` of `kind`, which targets accept by kind.
    /// Call every frame of the drag (only the latest counts). True once a
    /// target has accepted it.
    ///
    /// # Panics
    /// If `kind` is empty, over [`MAX_PAYLOAD_KIND`] bytes, has a NUL, or
    /// starts with `_` (ImGui's own kinds).
    pub fn set_payload(&mut self, kind: &str, data: &[u8]) -> bool {
        check_kind(kind);
        let (k, kn) = s(kind);
        unsafe { (self.ui.api.mqemu_imgui_set_drag_drop_payload)(k, kn, data.as_ptr().cast(), data.len(), 0) }
    }
}

impl Deref for DragSource<'_> {
    type Target = Ui;
    fn deref(&self) -> &Ui {
        self.ui
    }
}

impl DerefMut for DragSource<'_> {
    fn deref_mut(&mut self) -> &mut Ui {
        self.ui
    }
}

/// The last item as a drop target, while something is dragged over it; see
/// [`Ui::drag_drop_target`].
pub struct DropTarget<'ui> {
    ui: &'ui mut Ui,
}

impl<'ui> DropTarget<'ui> {
    /// One outside a drag, for checks that panic before reaching ImGui.
    #[cfg(test)]
    pub(crate) fn for_test(ui: &'ui mut Ui) -> Self {
        DropTarget { ui }
    }

    /// The payload of `kind` if one was just dropped here.
    ///
    /// # Panics
    /// If `kind` isn't a valid kind (see [`DragSource::set_payload`]).
    pub fn accept(&mut self, kind: &str) -> Option<Payload<'_>> {
        self.accept_with(kind, DragDropFlags::NONE)
    }

    /// [`DropTarget::accept`] with flags, e.g. [`DragDropFlags::ACCEPT_BEFORE_DELIVERY`]
    /// to see it while it hovers.
    ///
    /// # Panics
    /// If `kind` isn't a valid kind.
    pub fn accept_with(&mut self, kind: &str, flags: DragDropFlags) -> Option<Payload<'_>> {
        check_kind(kind);
        let (k, kn) = s(kind);
        let (mut data, mut size, mut preview, mut delivery) = (std::ptr::null(), 0, false, false);
        let accepted = unsafe {
            (self.ui.api.mqemu_imgui_accept_drag_drop_payload)(k, kn, flags.bits(), &mut data, &mut size, &mut preview, &mut delivery)
        };
        // SAFETY: ImGui keeps the payload until the drag ends, after this frame.
        accepted.then(|| unsafe { Payload::new(data, size, preview, delivery) })
    }
}

impl Ui {
    /// Makes the last item draggable: while it's dragged, `f` sets the
    /// payload ([`DragSource::set_payload`]) and draws the tooltip by the
    /// mouse. Call right after the item. Items without an ID (text, images)
    /// need [`DragDropFlags::SOURCE_ALLOW_NULL_ID`].
    pub fn drag_drop_source<R>(&mut self, f: impl FnOnce(&mut DragSource<'_>) -> R) -> Option<R> {
        self.drag_drop_source_with(DragDropFlags::NONE, f)
    }

    pub fn drag_drop_source_with<R>(&mut self, flags: DragDropFlags, f: impl FnOnce(&mut DragSource<'_>) -> R) -> Option<R> {
        if !unsafe { (self.api.mqemu_imgui_begin_drag_drop_source)(flags.bits()) } {
            return None;
        }
        let _end = EndOnDrop(self.api.mqemu_imgui_end_drag_drop_source);
        Some(f(&mut DragSource { ui: self }))
    }

    /// Makes the last item a drop target: while something is dragged over
    /// it, `f` sees it ([`DropTarget::accept`]). Call right after the item.
    pub fn drag_drop_target<R>(&mut self, f: impl FnOnce(&mut DropTarget<'_>) -> R) -> Option<R> {
        if !unsafe { (self.api.mqemu_imgui_begin_drag_drop_target)() } {
            return None;
        }
        let _end = EndOnDrop(self.api.mqemu_imgui_end_drag_drop_target);
        Some(f(&mut DropTarget { ui: self }))
    }

    /// Whatever is being dragged anywhere, with its kind.
    pub fn drag_drop_payload(&self) -> Option<(String, Payload<'_>)> {
        let mut kind = [0 as c_char; MAX_PAYLOAD_KIND + 1];
        let (mut data, mut size, mut preview, mut delivery) = (std::ptr::null(), 0, false, false);
        let found = unsafe {
            (self.api.mqemu_imgui_drag_drop_payload)(kind.as_mut_ptr(), kind.len(), &mut data, &mut size, &mut preview, &mut delivery)
        };
        // SAFETY: as in DropTarget::accept_with.
        found.then(|| (crate::text::from_c_array(&kind), unsafe { Payload::new(data, size, preview, delivery) }))
    }
}

/// Runs `f` with a [`DragSource`] for `ui`; for ImPlot's sources, which begin
/// and end differently.
pub(super) fn source_scope<R>(ui: &mut Ui, end: unsafe extern "C" fn(), f: impl FnOnce(&mut DragSource<'_>) -> R) -> R {
    let _end = EndOnDrop(end);
    f(&mut DragSource { ui })
}

/// As [`source_scope`], for targets.
pub(super) fn target_scope<R>(ui: &mut Ui, end: unsafe extern "C" fn(), f: impl FnOnce(&mut DropTarget<'_>) -> R) -> R {
    let _end = EndOnDrop(end);
    f(&mut DropTarget { ui })
}
