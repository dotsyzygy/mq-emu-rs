//! The memory editor MacroQuest ships (ImGui club's `MemoryEditor`): bytes
//! as a hex view, with an ASCII column, editing, a data preview and an
//! options menu.

use std::ffi::c_void;
use std::ops::Range;
use std::panic::resume_unwind;
use std::ptr::NonNull;

use mq_emu_sys::imgui::NO_STRING;
use mq_emu_sys::imgui_addons as sys;

use super::draw::pack_color;
use super::plot::{Callback, Kept};
use super::{Color, Ui, s};

/// How a [`MemoryEditor`] shows bytes. The user can change some from its
/// options menu (columns and what's shown); those changes come back here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MemoryEditorOptions {
    /// No editing, whatever the data allows.
    pub read_only: bool,
    /// Bytes per row (at least 1).
    pub columns: usize,
    /// The options button and address box below the bytes.
    pub show_options: bool,
    /// The selected bytes as a number (in a type chosen there) below.
    pub show_data_preview: bool,
    /// HexII: zero bytes blank, ASCII ones as `.X`.
    pub show_hexii: bool,
    pub show_ascii: bool,
    /// Zero bytes in ImGui's disabled text color.
    pub grey_out_zeroes: bool,
    pub upper_case_hex: bool,
    /// Extra space every this many columns (0 for none).
    pub mid_columns: usize,
    /// Address digits shown (0 for as many as the last address needs).
    pub address_digits: usize,
    /// Room kept below for widgets of your own.
    pub footer_extra_height: f32,
    /// Behind highlighted bytes ([`MemoryEditor::goto`], a hook's).
    pub highlight_color: Color,
}

impl Default for MemoryEditorOptions {
    /// The editor's own defaults.
    fn default() -> Self {
        MemoryEditorOptions {
            read_only: false,
            columns: 16,
            show_options: true,
            show_data_preview: false,
            show_hexii: false,
            show_ascii: true,
            grey_out_zeroes: true,
            upper_case_hex: true,
            mid_columns: 8,
            address_digits: 0,
            footer_extra_height: 0.0,
            highlight_color: [1.0, 1.0, 1.0, 50.0 / 255.0],
        }
    }
}

impl MemoryEditorOptions {
    fn raw(&self) -> sys::mqemu_memory_editor_options {
        let count = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        sys::mqemu_memory_editor_options {
            read_only: self.read_only,
            cols: count(self.columns.max(1)),
            show_options: self.show_options,
            show_data_preview: self.show_data_preview,
            show_hexii: self.show_hexii,
            show_ascii: self.show_ascii,
            grey_out_zeroes: self.grey_out_zeroes,
            upper_case_hex: self.upper_case_hex,
            mid_cols_count: count(self.mid_columns),
            addr_digits_count: count(self.address_digits),
            footer_extra_height: self.footer_extra_height,
            highlight_color: pack_color(self.highlight_color),
        }
    }

    /// What the user may have changed.
    fn update(&mut self, raw: &sys::mqemu_memory_editor_options) {
        self.columns = usize::try_from(raw.cols).unwrap_or(1).max(1);
        self.show_data_preview = raw.show_data_preview;
        self.show_hexii = raw.show_hexii;
        self.show_ascii = raw.show_ascii;
        self.grey_out_zeroes = raw.grey_out_zeroes;
        self.upper_case_hex = raw.upper_case_hex;
    }
}

/// The bytes a [`MemoryEditor`] shows.
pub enum MemoryData<'d> {
    /// Shown and editable.
    Bytes(&'d mut [u8]),
    /// Shown only.
    ReadOnly(&'d [u8]),
    /// `size` bytes read through `read`, by offset: for memory that may not
    /// be there to read, such as the game's. Editable through `write` if
    /// given, else read only.
    Source { size: usize, read: &'d mut dyn FnMut(usize) -> u8, write: Option<&'d mut dyn FnMut(usize, u8)> },
}

impl<'d> From<&'d mut [u8]> for MemoryData<'d> {
    fn from(bytes: &'d mut [u8]) -> Self {
        MemoryData::Bytes(bytes)
    }
}

impl<'d> From<&'d [u8]> for MemoryData<'d> {
    fn from(bytes: &'d [u8]) -> Self {
        MemoryData::ReadOnly(bytes)
    }
}

impl<'d, const N: usize> From<&'d mut [u8; N]> for MemoryData<'d> {
    fn from(bytes: &'d mut [u8; N]) -> Self {
        MemoryData::Bytes(bytes)
    }
}

impl<'d, const N: usize> From<&'d [u8; N]> for MemoryData<'d> {
    fn from(bytes: &'d [u8; N]) -> Self {
        MemoryData::ReadOnly(bytes)
    }
}

/// Closures the editor asks about each byte it draws, by offset.
#[derive(Default)]
pub struct MemoryEditorHooks<'h> {
    /// Whether to highlight it (in [`MemoryEditorOptions::highlight_color`]).
    pub highlight: Option<&'h mut dyn FnMut(usize) -> bool>,
    /// What's behind it (None for nothing), unless it's highlighted.
    pub background: Option<&'h mut dyn FnMut(usize) -> Option<Color>>,
}

/// The closures for one draw.
struct Hooked<'d, 'h> {
    read: Option<&'d mut dyn FnMut(usize) -> u8>,
    write: Option<&'d mut dyn FnMut(usize, u8)>,
    highlight: Option<&'h mut dyn FnMut(usize) -> bool>,
    background: Option<&'h mut dyn FnMut(usize) -> Option<Color>>,
}

unsafe extern "C" fn read_hook(_: *const u8, offset: usize, user: *mut c_void) -> u8 {
    let state = unsafe { &mut *user.cast::<Callback<Hooked<'_, '_>>>() };
    state.run(0, |h| h.read.as_mut().map_or(0, |f| f(offset)))
}

unsafe extern "C" fn write_hook(_: *mut u8, offset: usize, value: u8, user: *mut c_void) {
    let state = unsafe { &mut *user.cast::<Callback<Hooked<'_, '_>>>() };
    state.run((), |h| {
        if let Some(f) = h.write.as_mut() {
            f(offset, value)
        }
    })
}

unsafe extern "C" fn highlight_hook(_: *const u8, offset: usize, user: *mut c_void) -> bool {
    let state = unsafe { &mut *user.cast::<Callback<Hooked<'_, '_>>>() };
    state.run(false, |h| h.highlight.as_mut().is_some_and(|f| f(offset)))
}

unsafe extern "C" fn background_hook(_: *const u8, offset: usize, user: *mut c_void) -> u32 {
    let state = unsafe { &mut *user.cast::<Callback<Hooked<'_, '_>>>() };
    state.run(0, |h| h.background.as_mut().and_then(|f| f(offset)).map_or(0, pack_color))
}

/// A hex view of bytes ([`Ui::new_memory_editor`]): click a byte to edit it,
/// right-click or use the options button for its settings. It keeps what's
/// being edited between frames, so keep it and draw it each one.
pub struct MemoryEditor {
    raw: NonNull<sys::MemoryEditor>,
    api: &'static sys::Api,
    pub options: MemoryEditorOptions,
}

impl Ui {
    /// A memory editor with the default [`MemoryEditorOptions`].
    pub fn new_memory_editor(&self) -> MemoryEditor {
        let raw = NonNull::new(unsafe { (self.addons.mqemu_memory_editor_new)() }).expect("MemoryEditor's constructor threw");
        MemoryEditor { raw, api: self.addons, options: MemoryEditorOptions::default() }
    }
}

impl MemoryEditor {
    /// Draws `data` in the current window, addresses numbered from `base`.
    pub fn draw<'d>(&mut self, ui: &mut Ui, data: impl Into<MemoryData<'d>>, base: usize) {
        self.draw_with(ui, data, base, MemoryEditorHooks::default());
    }

    pub fn draw_with<'d>(&mut self, ui: &mut Ui, data: impl Into<MemoryData<'d>>, base: usize, hooks: MemoryEditorHooks<'_>) {
        let _ = ui;
        self.draw_raw(None, data.into(), base, hooks);
    }

    /// Draws `data` in a window of its own, titled `title`, sized to fit its
    /// columns. False once the user closes it: stop drawing it then.
    pub fn window<'d>(&mut self, ui: &mut Ui, title: &str, data: impl Into<MemoryData<'d>>, base: usize) -> bool {
        self.window_with(ui, title, data, base, MemoryEditorHooks::default())
    }

    pub fn window_with<'d>(&mut self, ui: &mut Ui, title: &str, data: impl Into<MemoryData<'d>>, base: usize, hooks: MemoryEditorHooks<'_>) -> bool {
        let _ = ui;
        self.draw_raw(Some(title), data.into(), base, hooks)
    }

    fn draw_raw(&mut self, title: Option<&str>, data: MemoryData<'_>, base: usize, hooks: MemoryEditorHooks<'_>) -> bool {
        let mut options = self.options.raw();
        let (bytes, size, read, write) = match data {
            MemoryData::Bytes(bytes) => (bytes.as_mut_ptr(), bytes.len(), None, None),
            MemoryData::ReadOnly(bytes) => {
                options.read_only = true;
                // Never written: read only, and no write hook.
                (bytes.as_ptr().cast_mut(), bytes.len(), None, None)
            }
            MemoryData::Source { size, read, write } => {
                options.read_only |= write.is_none();
                // The editor reads and writes only through the hooks.
                (std::ptr::null_mut(), size, Some(read), write)
            }
        };
        let (has_read, has_write) = (read.is_some(), write.is_some());
        let (has_highlight, has_background) = (hooks.highlight.is_some(), hooks.background.is_some());
        let mut state = Callback::new(Hooked { read, write, highlight: hooks.highlight, background: hooks.background });
        let raw_hooks = sys::mqemu_memory_editor_hooks {
            read: has_read.then_some(read_hook as _),
            write: has_write.then_some(write_hook as _),
            highlight: has_highlight.then_some(highlight_hook as _),
            background: has_background.then_some(background_hook as _),
            user: state.data(),
        };
        let any_hook = has_read || has_write || has_highlight || has_background;
        let (t, n) = title.map_or((std::ptr::null(), NO_STRING), s);
        let mut open = true;
        unsafe {
            (self.api.mqemu_memory_editor_draw)(
                self.raw.as_ptr(),
                &mut options,
                t,
                n,
                &mut open,
                bytes,
                size,
                base,
                if any_hook { &raw_hooks } else { std::ptr::null() },
            )
        };
        self.options.update(&options);
        if let Some(panic) = state.take_panic() {
            resume_unwind(panic);
        }
        open
    }

    /// Scrolls to offset `range.start` next draw and highlights `range`
    /// (offsets from the start of the data, not addresses).
    pub fn goto(&mut self, range: Range<usize>) {
        unsafe { (self.api.mqemu_memory_editor_goto)(self.raw.as_ptr(), range.start, range.end) }
    }

    /// The offset of the byte under the mouse in the last draw, if any.
    pub fn hovered(&self) -> Option<usize> {
        let mut offset = 0;
        unsafe { (self.api.mqemu_memory_editor_hovered)(self.raw.as_ptr(), &mut offset) }.then_some(offset)
    }
}

impl Drop for MemoryEditor {
    fn drop(&mut self) {
        unsafe { (self.api.mqemu_memory_editor_delete)(self.raw.as_ptr()) }
    }
}

impl std::fmt::Debug for MemoryEditor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryEditor").field("options", &self.options).finish_non_exhaustive()
    }
}
