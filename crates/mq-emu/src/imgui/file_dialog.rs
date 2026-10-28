//! The file dialog MacroQuest ships (ImGuiFileDialog): choosing files to
//! open or save, or a directory, in an ImGui window.

use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::panic::resume_unwind;
use std::ptr::NonNull;

use mq_emu_sys::imgui as imgui_sys;
use mq_emu_sys::imgui_addons as addons_sys;
use mq_emu_sys::imgui_mq as mq_sys;

use super::draw::Vec2;
use super::plot::{Callback, Kept};
use super::text_editor::{append_string, received};
use super::{Color, Font, Ui, WindowFlags, opt, s};

flags! {
    /// How a [`FileDialog`] behaves (`ImGuiFileDialogFlags_`).
    FileDialogFlags {
        /// Asks before choosing a file that exists (for saving).
        CONFIRM_OVERWRITE = ImGuiFileDialogFlags_ConfirmOverwrite,
        /// Hides files starting with a dot.
        DONT_SHOW_HIDDEN_FILES = ImGuiFileDialogFlags_DontShowHiddenFiles,
        DISABLE_CREATE_DIRECTORY_BUTTON = ImGuiFileDialogFlags_DisableCreateDirectoryButton,
        HIDE_COLUMN_TYPE = ImGuiFileDialogFlags_HideColumnType,
        HIDE_COLUMN_SIZE = ImGuiFileDialogFlags_HideColumnSize,
        HIDE_COLUMN_DATE = ImGuiFileDialogFlags_HideColumnDate,
        /// Drawn in the current window (between your own begin and end)
        /// rather than one of its own.
        NO_DIALOG = ImGuiFileDialogFlags_NoDialog,
        /// For opening: the file name can only be picked, not typed.
        READ_ONLY_FILE_NAME_FIELD = ImGuiFileDialogFlags_ReadOnlyFileNameField,
        /// Filters match extensions whatever their case.
        CASE_INSENSITIVE_EXTENSION = ImGuiFileDialogFlags_CaseInsensitiveExtention,
        /// A modal popup, blocking the rest of ImGui.
        MODAL = ImGuiFileDialogFlags_Modal,
        DISABLE_THUMBNAIL_MODE = ImGuiFileDialogFlags_DisableThumbnailMode,
        DISABLE_BOOKMARK_MODE = ImGuiFileDialogFlags_DisableBookmarkMode,
        DISABLE_QUICK_PATH_SELECTION = ImGuiFileDialogFlags_DisableQuickPathSelection,
        /// What the dialog's own examples use: confirm overwrite, modal, no type column.
        DEFAULT = ImGuiFileDialogFlags_Default,
    }
    in addons_sys
}

flags! {
    /// Which files a [`FileStyle`] applies to (`IGFD_FileStyleFlags_`),
    /// with the criteria it's set with.
    FileStyleFlags {
        /// Every file (no criteria).
        BY_TYPE_FILE = IGFD_FileStyleByTypeFile,
        /// Every directory.
        BY_TYPE_DIR = IGFD_FileStyleByTypeDir,
        /// Every link.
        BY_TYPE_LINK = IGFD_FileStyleByTypeLink,
        /// Files with the extension (`".lua"`).
        BY_EXTENSION = IGFD_FileStyleByExtention,
        /// The file or directory with exactly this name.
        BY_FULL_NAME = IGFD_FileStyleByFullName,
        /// Names containing it.
        BY_CONTAINED_IN_FULL_NAME = IGFD_FileStyleByContainedInFullName,
    }
    in addons_sys
}

/// How a chosen file's name meets the chosen filter's extension (`IGFD_ResultMode_`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(i32)]
pub enum ResultMode {
    /// Adds the filter's extension if the name has none (for saving).
    #[default]
    AddIfNoFileExt = addons_sys::IGFD_ResultMode_AddIfNoFileExt,
    /// Replaces the name's extension with the filter's.
    OverwriteFileExt = addons_sys::IGFD_ResultMode_OverwriteFileExt,
    /// The name as it is.
    KeepInputFile = addons_sys::IGFD_ResultMode_KeepInputFile,
}

/// What [`FileDialog::open`] shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FileDialogOpen<'a> {
    pub title: &'a str,
    /// Which files are listed, in ImGuiFileDialog's syntax: extensions
    /// (`".lua,.ini"`), named groups (`"Lua{.lua},Settings{.ini,.cfg}"`),
    /// `".*"` for everything, or regexes in parentheses. The user picks one
    /// from a list. None chooses a directory instead of files.
    pub filters: Option<&'a str>,
    /// The directory it starts in (relative to the game's directory).
    pub path: &'a str,
    /// The file name it starts with.
    pub file_name: &'a str,
    /// How many files may be chosen; 0 for any number.
    pub max_selection: usize,
    pub flags: FileDialogFlags,
    /// A side pane this wide, drawn by [`FileDialog::display_with`]'s closure.
    pub pane_width: Option<f32>,
}

impl Default for FileDialogOpen<'_> {
    /// One file, of any kind, from the game's directory.
    fn default() -> Self {
        FileDialogOpen { title: "", filters: Some(".*"), path: ".", file_name: "", max_selection: 1, flags: FileDialogFlags::NONE, pane_width: None }
    }
}

/// How files a [`FileDialog`] lists look ([`FileDialog::set_file_style`]).
#[derive(Debug, Clone, PartialEq)]
pub struct FileStyle {
    pub color: Color,
    /// Text (or an icon from the font) shown before the name.
    pub icon: String,
    /// The font they're listed in, if not the dialog's.
    pub font: Option<Font>,
}

/// One file the user chose.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectedFile {
    /// Its name with its extension.
    pub name: String,
    /// Its whole path.
    pub path: String,
}

/// A file dialog ([`Ui::new_file_dialog`]). [`open`](FileDialog::open) it
/// under a key, then [`display`](FileDialog::display) that key every frame:
/// it returns true once, when the user chooses (OK) or cancels; read
/// [`is_ok`](FileDialog::is_ok) and the choice, then
/// [`close`](FileDialog::close) it.
///
/// ```no_run
/// # use mq_emu::imgui::{FileDialog, FileDialogOpen, ResultMode, Ui};
/// # fn draw(ui: &mut Ui, dialog: &mut FileDialog) {
/// if ui.button("Load...") {
///     dialog.open("load", FileDialogOpen { title: "Load settings", filters: Some(".ini"), ..Default::default() });
/// }
/// if dialog.display(ui, "load") {
///     if dialog.is_ok() {
///         let path = dialog.file_path_name(ResultMode::KeepInputFile);
///         // load it...
///     }
///     dialog.close();
/// }
/// # }
/// ```
pub struct FileDialog {
    raw: NonNull<addons_sys::mqemu_file_dialog>,
    api: &'static addons_sys::Api,
    imgui: &'static imgui_sys::Api,
    mq: &'static mq_sys::Api,
    /// The fonts styles were set with, to give back as [`Font`]s.
    fonts: HashMap<(i32, String), Font>,
}

impl Ui {
    pub fn new_file_dialog(&self) -> FileDialog {
        let raw = NonNull::new(unsafe { (self.addons.mqemu_file_dialog_new)() }).expect("ImGuiFileDialog's constructor threw");
        FileDialog { raw, api: self.addons, imgui: self.api, mq: self.mq, fonts: HashMap::new() }
    }
}

/// Draws a file dialog's side pane ([`FileDialog::display_with`]).
pub type DrawPane<'p> = dyn FnMut(&mut Ui, &str) -> bool + 'p;

/// A side pane's closure, and the frame's [`Ui`] for it.
struct Pane<'p> {
    ui: *mut Ui,
    draw: &'p mut DrawPane<'p>,
}

unsafe extern "C" fn draw_pane(user: *mut c_void, filter: *const c_char, len: usize, can_continue: *mut bool) {
    let state = unsafe { &mut *user.cast::<Callback<Pane<'_>>>() };
    let filter = if len == 0 { String::new() } else { String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(filter.cast::<u8>(), len) }).into_owned() };
    // The Ui is display_with's, which is lent to the dialog for the call.
    let ok = state.run(true, |pane| (pane.draw)(unsafe { &mut *pane.ui }, &filter));
    unsafe { *can_continue = ok };
}

unsafe extern "C" fn push_selected(user: *mut c_void, name: *const c_char, name_len: usize, path: *const c_char, path_len: usize) {
    let text = |p: *const c_char, n: usize| {
        if n == 0 { String::new() } else { String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p.cast::<u8>(), n) }).into_owned() }
    };
    let file = SelectedFile { name: text(name, name_len), path: text(path, path_len) };
    unsafe { (*user.cast::<Vec<SelectedFile>>()).push(file) };
}

impl FileDialog {
    fn ptr(&self) -> *mut addons_sys::mqemu_file_dialog {
        self.raw.as_ptr()
    }

    /// Opens it as `key`, replacing what it had open.
    pub fn open(&mut self, key: &str, open: FileDialogOpen<'_>) {
        let (k, kn) = s(key);
        let (t, tn) = s(open.title);
        let (f, fl) = opt(open.filters);
        let (p, pn) = s(open.path);
        let (n, nn) = s(open.file_name);
        let max = i32::try_from(open.max_selection).unwrap_or(i32::MAX);
        let pane = open.pane_width.unwrap_or(0.0);
        unsafe { (self.api.mqemu_file_dialog_open)(self.ptr(), k, kn, t, tn, f, fl, p, pn, n, nn, max, open.flags.bits(), pane) }
    }

    /// Draws it if it's open as `key`, in a window that can't collapse.
    /// True once the user has chosen or cancelled.
    pub fn display(&mut self, ui: &mut Ui, key: &str) -> bool {
        self.display_with(ui, key, WindowFlags::NO_COLLAPSE, [0.0, 0.0], [f32::MAX, f32::MAX], None)
    }

    /// [`display`](FileDialog::display) with its window's flags and size
    /// limits, and a closure drawing its side pane (if it was opened with
    /// one): it gets the chosen filter's extensions (`".ini"` for
    /// `"Settings{.ini}"`) and returns whether OK may be pressed.
    pub fn display_with(
        &mut self,
        ui: &mut Ui,
        key: &str,
        window_flags: WindowFlags,
        min_size: Vec2,
        max_size: Vec2,
        pane: Option<&mut DrawPane<'_>>,
    ) -> bool {
        let (k, kn) = s(key);
        let has_pane = pane.is_some();
        let mut ignore = |_: &mut Ui, _: &str| true;
        let mut state = Callback::new(Pane { ui: std::ptr::from_mut(ui), draw: pane.unwrap_or(&mut ignore) });
        let closed = unsafe {
            (self.api.mqemu_file_dialog_display)(
                self.ptr(),
                k,
                kn,
                window_flags.bits(),
                min_size[0],
                min_size[1],
                max_size[0],
                max_size[1],
                has_pane.then_some(draw_pane as _),
                state.data(),
            )
        };
        if let Some(panic) = state.take_panic() {
            resume_unwind(panic);
        }
        closed
    }

    /// Closes it, whether or not the user chose.
    pub fn close(&mut self) {
        unsafe { (self.api.mqemu_file_dialog_close)(self.ptr()) }
    }

    /// Whether it closed with OK (rather than cancel).
    pub fn is_ok(&self) -> bool {
        unsafe { (self.api.mqemu_file_dialog_is_ok)(self.ptr()) }
    }

    /// Whether it's open as `key`, or as anything for None.
    pub fn is_open(&self, key: Option<&str>) -> bool {
        let (k, kn) = opt(key);
        unsafe { (self.api.mqemu_file_dialog_is_opened)(self.ptr(), k, kn) }
    }

    /// Whether it was displayed this frame, as `key` or as anything for None.
    pub fn was_displayed_this_frame(&self, key: Option<&str>) -> bool {
        let (k, kn) = opt(key);
        unsafe { (self.api.mqemu_file_dialog_was_opened_this_frame)(self.ptr(), k, kn) }
    }

    /// The files chosen (for opening).
    pub fn selection(&self, mode: ResultMode) -> Vec<SelectedFile> {
        let mut files = Vec::<SelectedFile>::new();
        unsafe { (self.api.mqemu_file_dialog_selection)(self.ptr(), mode as i32, push_selected, (&raw mut files).cast()) };
        files
    }

    /// The directory and file name chosen, as one path (for saving).
    pub fn file_path_name(&self, mode: ResultMode) -> String {
        received(|sink, user| unsafe { (self.api.mqemu_file_dialog_file_path_name)(self.ptr(), mode as i32, sink, user) })
    }

    /// The file name typed or chosen.
    pub fn current_file_name(&self, mode: ResultMode) -> String {
        received(|sink, user| unsafe { (self.api.mqemu_file_dialog_current_file_name)(self.ptr(), mode as i32, sink, user) })
    }

    /// The directory it's showing (the one chosen, for directories).
    pub fn current_path(&self) -> String {
        received(|sink, user| unsafe { (self.api.mqemu_file_dialog_current_path)(self.ptr(), sink, user) })
    }

    /// The filter chosen, as it's written in [`FileDialogOpen::filters`].
    pub fn current_filter(&self) -> String {
        received(|sink, user| unsafe { (self.api.mqemu_file_dialog_current_filter)(self.ptr(), sink, user) })
    }

    /// How the files `flags` and `criteria` pick look (criteria are
    /// ignored for the `BY_TYPE_*` flags).
    pub fn set_file_style(&mut self, flags: FileStyleFlags, criteria: &str, style: &FileStyle) {
        let (c, cn) = s(criteria);
        let (i, n) = s(&style.icon);
        let font = style.font.map_or(std::ptr::null_mut(), |f| f.raw(self.imgui, self.mq));
        unsafe { (self.api.mqemu_file_dialog_set_file_style)(self.ptr(), flags.bits(), c, cn, style.color.as_ptr(), i, n, font) };
        let key = (flags.bits(), criteria.to_owned());
        match style.font {
            Some(font) => self.fonts.insert(key, font),
            None => self.fonts.remove(&key),
        };
    }

    /// The style set for `flags` and `criteria`, if any.
    pub fn file_style(&self, flags: FileStyleFlags, criteria: &str) -> Option<FileStyle> {
        let (c, cn) = s(criteria);
        let mut color = [0.0; 4];
        let mut icon = String::new();
        let mut font = std::ptr::null_mut();
        let found = unsafe {
            (self.api.mqemu_file_dialog_file_style)(self.ptr(), flags.bits(), c, cn, color.as_mut_ptr(), append_string, (&raw mut icon).cast(), &mut font)
        };
        let font = (!font.is_null()).then(|| self.fonts.get(&(flags.bits(), criteria.to_owned())).copied()).flatten();
        found.then_some(FileStyle { color, icon, font })
    }

    /// Forgets every style set.
    pub fn clear_file_styles(&mut self) {
        unsafe { (self.api.mqemu_file_dialog_clear_file_styles)(self.ptr()) };
        self.fonts.clear();
    }
}

impl Drop for FileDialog {
    fn drop(&mut self) {
        unsafe { (self.api.mqemu_file_dialog_delete)(self.ptr()) }
    }
}

impl std::fmt::Debug for FileDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileDialog").field("open", &self.is_open(None)).finish_non_exhaustive()
    }
}
