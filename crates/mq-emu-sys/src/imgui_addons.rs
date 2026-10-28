//! MacroQuest's ImGui add-ons, from `shim/imgui_addons.cpp`: its text editor
//! (`mq::imgui::TextEditor`, syntax highlighting, undo), the memory editor
//! (`MemoryEditor`, a hex view) and the file dialog (ImGuiFileDialog). All
//! three are in `imgui.dll` (the memory editor is header-only, so the shim
//! compiles MacroQuest's copy), so like [`imgui`](crate::imgui) they run
//! headlessly; conventions as there.
//!
//! Each is an object made here and freed with its `_delete`. Drawing calls
//! (`_render`, `_draw`, `_display`) may only be made inside an ImGui frame;
//! the rest at any time.
//!
//! Text of any length comes back through a [`mqemu_string_sink`]: the shim
//! calls it with the text, during the call only.

use core::ffi::{c_char, c_void};
use core::marker::{PhantomData, PhantomPinned};

use crate::imgui::ImFont;

/// MacroQuest's `mq::imgui::TextEditor`.
#[repr(C)]
pub struct TextEditor {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// ImGui club's `MemoryEditor`, as MacroQuest ships it.
#[repr(C)]
pub struct MemoryEditor {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// An ImGuiFileDialog context, with what the shim keeps for its side pane.
#[repr(C)]
pub struct mqemu_file_dialog {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// Receives text: `len` bytes at `text` (not NUL-terminated), valid during the call.
pub type mqemu_string_sink = unsafe extern "C" fn(user: *mut c_void, text: *const c_char, len: usize);

/// Receives one file of a file dialog's selection: its name and its full path.
pub type mqemu_selection_sink =
    unsafe extern "C" fn(user: *mut c_void, name: *const c_char, name_len: usize, path: *const c_char, path_len: usize);

/// Strings as parallel arrays of pointers and lengths.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct mqemu_strings {
    pub texts: *const *const c_char,
    pub lens: *const usize,
    pub count: usize,
}

/// A language for the text editor to highlight (`LanguageDefinition`), for
/// [`mqemu_text_editor_set_language_definition`].
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct mqemu_text_editor_language {
    pub name: *const c_char,
    pub name_len: usize,
    /// Words shown as `PaletteIndex_Keyword`.
    pub keywords: mqemu_strings,
    /// Names shown as `PaletteIndex_KnownIdentifier`, with the declaration
    /// shown when hovered (`declarations`, as many).
    pub identifiers: mqemu_strings,
    pub declarations: mqemu_strings,
    /// Names after the preprocessor character, shown as `PaletteIndex_PreprocIdentifier`.
    pub preproc_identifiers: mqemu_strings,
    pub preproc_declarations: mqemu_strings,
    pub comment_start: *const c_char,
    pub comment_start_len: usize,
    pub comment_end: *const c_char,
    pub comment_end_len: usize,
    pub single_line_comment: *const c_char,
    pub single_line_comment_len: usize,
    pub preproc_char: c_char,
    pub auto_indentation: bool,
    /// ECMAScript regular expressions, each with the `PaletteIndex_` its
    /// matches are shown in (`token_palette`, as many).
    pub token_regexes: mqemu_strings,
    pub token_palette: *const i32,
    pub case_sensitive: bool,
}

/// The memory editor's settings (its `Opt*` fields and the like). The user
/// can change some from its options menu: [`mqemu_memory_editor_draw`]
/// writes them back.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct mqemu_memory_editor_options {
    pub read_only: bool,
    /// Bytes per row (1 or more).
    pub cols: i32,
    pub show_options: bool,
    pub show_data_preview: bool,
    pub show_hexii: bool,
    pub show_ascii: bool,
    pub grey_out_zeroes: bool,
    pub upper_case_hex: bool,
    /// Extra space every this many columns (0 for none).
    pub mid_cols_count: i32,
    /// Address digits shown (0 for as many as the last address needs).
    pub addr_digits_count: i32,
    pub footer_extra_height: f32,
    pub highlight_color: u32,
}

/// Functions the memory editor calls while it draws, each optional: with
/// `read`, it reads bytes only through it (the data pointer may be null),
/// and with `write` writes only through it. Offsets are from the start of
/// the data. None may unwind.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct mqemu_memory_editor_hooks {
    pub read: Option<unsafe extern "C" fn(data: *const u8, offset: usize, user: *mut c_void) -> u8>,
    pub write: Option<unsafe extern "C" fn(data: *mut u8, offset: usize, value: u8, user: *mut c_void)>,
    pub highlight: Option<unsafe extern "C" fn(data: *const u8, offset: usize, user: *mut c_void) -> bool>,
    /// A byte's background (transparent for none).
    pub background: Option<unsafe extern "C" fn(data: *const u8, offset: usize, user: *mut c_void) -> u32>,
    pub user: *mut c_void,
}

/// Draws a file dialog's side pane, inside the dialog: `filter` is the
/// filter chosen; `*can_continue` (true to start with) false disables OK.
pub type mqemu_file_dialog_pane_fn = unsafe extern "C" fn(user: *mut c_void, filter: *const c_char, filter_len: usize, can_continue: *mut bool);

function_table! {
    // ----- text editor -----
    pub fn mqemu_text_editor_new() -> *mut TextEditor;
    pub fn mqemu_text_editor_delete(editor: *mut TextEditor);
    /// Draws it, in a child window `title` of `w` x `h` (0 for the space
    /// left) unless it's set to ignore the child.
    pub fn mqemu_text_editor_render(editor: *mut TextEditor, title: *const c_char, title_len: usize, w: f32, h: f32, border: bool);

    /// `language` a `MQEMU_LANGUAGE_` value.
    pub fn mqemu_text_editor_set_language(editor: *mut TextEditor, language: i32);
    /// False if a token regex doesn't compile, with why through `error`;
    /// nothing changes then.
    pub fn mqemu_text_editor_set_language_definition(
        editor: *mut TextEditor,
        language: *const mqemu_text_editor_language,
        error: mqemu_string_sink,
        user: *mut c_void,
    ) -> bool;
    pub fn mqemu_text_editor_language_name(editor: *const TextEditor, sink: mqemu_string_sink, user: *mut c_void);
    /// `PaletteIndex_Max` colors (`ImU32`).
    pub fn mqemu_text_editor_palette(editor: *const TextEditor, out: *mut u32);
    pub fn mqemu_text_editor_set_palette(editor: *mut TextEditor, palette: *const u32);
    /// `which` a `MQEMU_PALETTE_` value; `PaletteIndex_Max` colors.
    pub fn mqemu_text_editor_builtin_palette(which: i32, out: *mut u32);
    /// Lines as numbered on screen (from 1) and messages, as many; a line's
    /// message shows when hovered.
    pub fn mqemu_text_editor_set_error_markers(editor: *mut TextEditor, lines: *const i32, messages: mqemu_strings);
    /// Lines as numbered on screen (from 1).
    pub fn mqemu_text_editor_set_breakpoints(editor: *mut TextEditor, lines: *const i32, count: usize);

    pub fn mqemu_text_editor_set_text(editor: *mut TextEditor, text: *const c_char, len: usize);
    /// Shown in `color` as is, not highlighted.
    pub fn mqemu_text_editor_set_text_colored(editor: *mut TextEditor, text: *const c_char, len: usize, color: u32);
    pub fn mqemu_text_editor_set_text_lines(editor: *mut TextEditor, lines: mqemu_strings);
    pub fn mqemu_text_editor_insert_text(editor: *mut TextEditor, text: *const c_char, len: usize);
    pub fn mqemu_text_editor_insert_text_colored(editor: *mut TextEditor, text: *const c_char, len: usize, color: u32);
    pub fn mqemu_text_editor_text(editor: *const TextEditor, sink: mqemu_string_sink, user: *mut c_void);
    /// Calls `sink` once per line.
    pub fn mqemu_text_editor_text_lines(editor: *const TextEditor, sink: mqemu_string_sink, user: *mut c_void);
    pub fn mqemu_text_editor_selected_text(editor: *const TextEditor, sink: mqemu_string_sink, user: *mut c_void);
    pub fn mqemu_text_editor_current_line_text(editor: *const TextEditor, sink: mqemu_string_sink, user: *mut c_void);
    pub fn mqemu_text_editor_total_lines(editor: *const TextEditor) -> i32;

    pub fn mqemu_text_editor_is_overwrite(editor: *const TextEditor) -> bool;
    pub fn mqemu_text_editor_is_read_only(editor: *const TextEditor) -> bool;
    pub fn mqemu_text_editor_set_read_only(editor: *mut TextEditor, value: bool);
    /// Since the last render started.
    pub fn mqemu_text_editor_is_text_changed(editor: *const TextEditor) -> bool;
    pub fn mqemu_text_editor_is_cursor_position_changed(editor: *const TextEditor) -> bool;
    pub fn mqemu_text_editor_set_render_cursor(editor: *mut TextEditor, value: bool);
    pub fn mqemu_text_editor_set_render_line_numbers(editor: *mut TextEditor, value: bool);
    pub fn mqemu_text_editor_is_colorizer_enabled(editor: *const TextEditor) -> bool;
    pub fn mqemu_text_editor_set_colorizer_enabled(editor: *mut TextEditor, value: bool);
    pub fn mqemu_text_editor_set_handle_mouse_inputs(editor: *mut TextEditor, value: bool);
    pub fn mqemu_text_editor_is_handle_keyboard_inputs(editor: *const TextEditor) -> bool;
    pub fn mqemu_text_editor_set_handle_keyboard_inputs(editor: *mut TextEditor, value: bool);
    pub fn mqemu_text_editor_is_imgui_child_ignored(editor: *const TextEditor) -> bool;
    pub fn mqemu_text_editor_set_imgui_child_ignored(editor: *mut TextEditor, value: bool);
    pub fn mqemu_text_editor_is_showing_whitespace(editor: *const TextEditor) -> bool;
    pub fn mqemu_text_editor_set_show_whitespace(editor: *mut TextEditor, value: bool);
    pub fn mqemu_text_editor_is_showing_short_tab_glyphs(editor: *const TextEditor) -> bool;
    pub fn mqemu_text_editor_set_show_short_tab_glyphs(editor: *mut TextEditor, value: bool);
    pub fn mqemu_text_editor_tab_size(editor: *const TextEditor) -> i32;
    /// Clamped to 1 to 32.
    pub fn mqemu_text_editor_set_tab_size(editor: *mut TextEditor, value: i32);

    /// Coordinates are a line and a column (0-based, tabs counted as the
    /// columns they take).
    pub fn mqemu_text_editor_cursor_position(editor: *const TextEditor, line: *mut i32, column: *mut i32);
    pub fn mqemu_text_editor_set_cursor_position(editor: *mut TextEditor, line: i32, column: i32);
    pub fn mqemu_text_editor_move_up(editor: *mut TextEditor, amount: i32, select: bool);
    pub fn mqemu_text_editor_move_down(editor: *mut TextEditor, amount: i32, select: bool);
    pub fn mqemu_text_editor_move_left(editor: *mut TextEditor, amount: i32, select: bool, word_mode: bool);
    pub fn mqemu_text_editor_move_right(editor: *mut TextEditor, amount: i32, select: bool, word_mode: bool);
    pub fn mqemu_text_editor_move_top(editor: *mut TextEditor, select: bool);
    pub fn mqemu_text_editor_move_bottom(editor: *mut TextEditor, select: bool);
    pub fn mqemu_text_editor_move_home(editor: *mut TextEditor, select: bool);
    pub fn mqemu_text_editor_move_end(editor: *mut TextEditor, select: bool);
    pub fn mqemu_text_editor_set_selection_start(editor: *mut TextEditor, line: i32, column: i32);
    pub fn mqemu_text_editor_set_selection_end(editor: *mut TextEditor, line: i32, column: i32);
    /// `mode` a `SelectionMode_` value.
    pub fn mqemu_text_editor_set_selection(editor: *mut TextEditor, start_line: i32, start_column: i32, end_line: i32, end_column: i32, mode: i32);
    /// Start and end, equal when nothing is selected.
    pub fn mqemu_text_editor_selection(editor: *const TextEditor, start_line: *mut i32, start_column: *mut i32, end_line: *mut i32, end_column: *mut i32);
    pub fn mqemu_text_editor_select_word_under_cursor(editor: *mut TextEditor);
    pub fn mqemu_text_editor_select_all(editor: *mut TextEditor);
    pub fn mqemu_text_editor_has_selection(editor: *const TextEditor) -> bool;

    /// Through ImGui's clipboard.
    pub fn mqemu_text_editor_copy(editor: *mut TextEditor);
    pub fn mqemu_text_editor_cut(editor: *mut TextEditor);
    pub fn mqemu_text_editor_paste(editor: *mut TextEditor);
    pub fn mqemu_text_editor_delete_forward(editor: *mut TextEditor);
    pub fn mqemu_text_editor_can_undo(editor: *const TextEditor) -> bool;
    pub fn mqemu_text_editor_can_redo(editor: *const TextEditor) -> bool;
    pub fn mqemu_text_editor_undo(editor: *mut TextEditor, steps: i32);
    pub fn mqemu_text_editor_redo(editor: *mut TextEditor, steps: i32);

    // ----- memory editor -----
    pub fn mqemu_memory_editor_new() -> *mut MemoryEditor;
    pub fn mqemu_memory_editor_delete(editor: *mut MemoryEditor);
    /// Draws `size` bytes at `data`, numbered from `base`, with `*options`
    /// (written back with the user's changes): in a window `title` (with a
    /// close button, which sets `*open` false) or, with no title
    /// (`title_len` [`NO_STRING`](crate::imgui::NO_STRING)), in the current
    /// one. `hooks` may be null.
    pub fn mqemu_memory_editor_draw(
        editor: *mut MemoryEditor,
        options: *mut mqemu_memory_editor_options,
        title: *const c_char,
        title_len: usize,
        open: *mut bool,
        data: *mut u8,
        size: usize,
        base: usize,
        hooks: *const mqemu_memory_editor_hooks,
    );
    /// Scrolls to offset `min` next draw and highlights `min` up to `max`.
    pub fn mqemu_memory_editor_goto(editor: *mut MemoryEditor, min: usize, max: usize);
    /// Whether the mouse was over a byte last draw, and its offset.
    pub fn mqemu_memory_editor_hovered(editor: *const MemoryEditor, offset: *mut usize) -> bool;

    // ----- file dialog -----
    pub fn mqemu_file_dialog_new() -> *mut mqemu_file_dialog;
    pub fn mqemu_file_dialog_delete(dialog: *mut mqemu_file_dialog);
    /// Opens it as `key` (shown by [`mqemu_file_dialog_display`] with that
    /// key). `filters` absent ([`NO_STRING`](crate::imgui::NO_STRING)) picks a
    /// directory. Up to `max_selection` files (0 for any number); `flags`
    /// `ImGuiFileDialogFlags_`; a side pane `pane_width` wide if that's more than 0.
    pub fn mqemu_file_dialog_open(
        dialog: *mut mqemu_file_dialog,
        key: *const c_char,
        key_len: usize,
        title: *const c_char,
        title_len: usize,
        filters: *const c_char,
        filters_len: usize,
        path: *const c_char,
        path_len: usize,
        file_name: *const c_char,
        file_name_len: usize,
        max_selection: i32,
        flags: i32,
        pane_width: f32,
    );
    /// Draws it if it's open as `key`; true once it closes (OK or cancel).
    /// Its side pane, if opened with one, is drawn by `pane` (which may be
    /// absent) during the call.
    pub fn mqemu_file_dialog_display(
        dialog: *mut mqemu_file_dialog,
        key: *const c_char,
        key_len: usize,
        window_flags: i32,
        min_w: f32,
        min_h: f32,
        max_w: f32,
        max_h: f32,
        pane: Option<mqemu_file_dialog_pane_fn>,
        user: *mut c_void,
    ) -> bool;
    pub fn mqemu_file_dialog_close(dialog: *mut mqemu_file_dialog);
    /// Whether it closed with OK.
    pub fn mqemu_file_dialog_is_ok(dialog: *const mqemu_file_dialog) -> bool;
    /// For `key`, or any key if that's absent.
    pub fn mqemu_file_dialog_is_opened(dialog: *const mqemu_file_dialog, key: *const c_char, key_len: usize) -> bool;
    pub fn mqemu_file_dialog_was_opened_this_frame(dialog: *const mqemu_file_dialog, key: *const c_char, key_len: usize) -> bool;
    /// The files chosen; `mode` an `IGFD_ResultMode_` value.
    pub fn mqemu_file_dialog_selection(dialog: *mut mqemu_file_dialog, mode: i32, sink: mqemu_selection_sink, user: *mut c_void);
    pub fn mqemu_file_dialog_file_path_name(dialog: *mut mqemu_file_dialog, mode: i32, sink: mqemu_string_sink, user: *mut c_void);
    pub fn mqemu_file_dialog_current_file_name(dialog: *mut mqemu_file_dialog, mode: i32, sink: mqemu_string_sink, user: *mut c_void);
    pub fn mqemu_file_dialog_current_path(dialog: *mut mqemu_file_dialog, sink: mqemu_string_sink, user: *mut c_void);
    pub fn mqemu_file_dialog_current_filter(dialog: *mut mqemu_file_dialog, sink: mqemu_string_sink, user: *mut c_void);
    /// How files matching `criteria` look (`flags` `IGFD_FileStyle` values):
    /// `color` (RGBA floats), `icon` text before the name, `font` (may be null).
    pub fn mqemu_file_dialog_set_file_style(
        dialog: *mut mqemu_file_dialog,
        flags: i32,
        criteria: *const c_char,
        criteria_len: usize,
        color: *const f32,
        icon: *const c_char,
        icon_len: usize,
        font: *mut ImFont,
    );
    /// False if there's no style for `criteria`.
    pub fn mqemu_file_dialog_file_style(
        dialog: *mut mqemu_file_dialog,
        flags: i32,
        criteria: *const c_char,
        criteria_len: usize,
        color: *mut f32,
        icon: mqemu_string_sink,
        user: *mut c_void,
        font: *mut *mut ImFont,
    ) -> bool;
    pub fn mqemu_file_dialog_clear_file_styles(dialog: *mut mqemu_file_dialog);
}

/// For tests: the text editor's own undo and redo, which lose their place
/// (see `UndoStarts` in shim/imgui_addons.cpp).
#[doc(hidden)]
pub mod testing {
    use super::TextEditor;

    unsafe extern "C" {
        pub fn mqemu_text_editor_test_undo_raw(editor: *mut TextEditor, steps: i32);
        pub fn mqemu_text_editor_test_redo_raw(editor: *mut TextEditor, steps: i32);
    }
}

/// [`mqemu_text_editor_set_language`]'s built-in languages.
pub const MQEMU_LANGUAGE_CPLUSPLUS: i32 = 0;
pub const MQEMU_LANGUAGE_HLSL: i32 = 1;
pub const MQEMU_LANGUAGE_GLSL: i32 = 2;
pub const MQEMU_LANGUAGE_C: i32 = 3;
pub const MQEMU_LANGUAGE_SQL: i32 = 4;
pub const MQEMU_LANGUAGE_LUA: i32 = 5;
pub const MQEMU_LANGUAGE_PLAIN_TEXT: i32 = 6;

/// [`mqemu_text_editor_builtin_palette`]'s palettes.
pub const MQEMU_PALETTE_DARK: i32 = 0;
pub const MQEMU_PALETTE_LIGHT: i32 = 1;
pub const MQEMU_PALETTE_RETRO_BLUE: i32 = 2;

macro_rules! addon_constants {
    ($list:ident, $header:literal; $($name:ident = $value:expr,)*) => {
        $( pub const $name: i32 = $value; )*

        #[doc = concat!("Every constant in this group with its name, for checking against `", $header, "`.")]
        #[doc(hidden)]
        pub const $list: &[(&str, i32)] = &[$( (stringify!($name), $name) ),*];
    };
}

// From MacroQuest's src/imgui/ImGuiTextEditor.h, where they're `enum class`
// values (`PaletteIndex::Keyword` here is `PaletteIndex_Keyword`).
// tests/imgui_constants.rs checks these and the file dialog's.
addon_constants! {
    TEXT_EDITOR_CONSTANTS, "imgui/ImGuiTextEditor.h";
    PaletteIndex_Default = 0,
    PaletteIndex_Keyword = 1,
    PaletteIndex_Number = 2,
    PaletteIndex_String = 3,
    PaletteIndex_CharLiteral = 4,
    PaletteIndex_Punctuation = 5,
    PaletteIndex_Preprocessor = 6,
    PaletteIndex_Identifier = 7,
    PaletteIndex_KnownIdentifier = 8,
    PaletteIndex_PreprocIdentifier = 9,
    PaletteIndex_Comment = 10,
    PaletteIndex_MultiLineComment = 11,
    PaletteIndex_Background = 12,
    PaletteIndex_Cursor = 13,
    PaletteIndex_Selection = 14,
    PaletteIndex_ErrorMarker = 15,
    PaletteIndex_Breakpoint = 16,
    PaletteIndex_LineNumber = 17,
    PaletteIndex_CurrentLineFill = 18,
    PaletteIndex_CurrentLineFillInactive = 19,
    PaletteIndex_CurrentLineEdge = 20,
    // How many colors a palette has.
    PaletteIndex_Max = 21,
    SelectionMode_Normal = 0,
    SelectionMode_Word = 1,
    SelectionMode_Line = 2,
}

// From src/imgui/ImGuiFileDialog.h.
addon_constants! {
    FILE_DIALOG_CONSTANTS, "imgui/ImGuiFileDialog.h";
    IGFD_FileStyle_None = 0,
    IGFD_FileStyleByTypeFile = 1 << 0,
    IGFD_FileStyleByTypeDir = 1 << 1,
    IGFD_FileStyleByTypeLink = 1 << 2,
    IGFD_FileStyleByExtention = 1 << 3,
    IGFD_FileStyleByFullName = 1 << 4,
    IGFD_FileStyleByContainedInFullName = 1 << 5,
    ImGuiFileDialogFlags_None = 0,
    ImGuiFileDialogFlags_ConfirmOverwrite = 1 << 0,
    ImGuiFileDialogFlags_DontShowHiddenFiles = 1 << 1,
    ImGuiFileDialogFlags_DisableCreateDirectoryButton = 1 << 2,
    ImGuiFileDialogFlags_HideColumnType = 1 << 3,
    ImGuiFileDialogFlags_HideColumnSize = 1 << 4,
    ImGuiFileDialogFlags_HideColumnDate = 1 << 5,
    ImGuiFileDialogFlags_NoDialog = 1 << 6,
    ImGuiFileDialogFlags_ReadOnlyFileNameField = 1 << 7,
    ImGuiFileDialogFlags_CaseInsensitiveExtention = 1 << 8,
    ImGuiFileDialogFlags_Modal = 1 << 9,
    ImGuiFileDialogFlags_DisableThumbnailMode = 1 << 10,
    ImGuiFileDialogFlags_DisableBookmarkMode = 1 << 11,
    ImGuiFileDialogFlags_DisableQuickPathSelection = 1 << 12,
    ImGuiFileDialogFlags_Default = 0x209,
    IGFD_ResultMode_AddIfNoFileExt = 0,
    IGFD_ResultMode_OverwriteFileExt = 1,
    IGFD_ResultMode_KeepInputFile = 2,
}
