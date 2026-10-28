//! MacroQuest's text editor (`mq::imgui::TextEditor`): a code editor with
//! syntax highlighting, line numbers, undo, error markers and breakpoints.

use std::ffi::{c_char, c_void};
use std::ops::{Index, IndexMut, Range};
use std::ptr::NonNull;

use mq_emu_sys::imgui_addons as sys;

use super::draw::{Vec2, pack_color};
use super::plot::{Callback, Kept};
use super::{Color, Ui, s};

/// Appends what the shim sends to the `String` at `user`.
pub(super) unsafe extern "C" fn append_string(user: *mut c_void, text: *const c_char, len: usize) {
    let out = unsafe { &mut *user.cast::<String>() };
    if len > 0 {
        out.push_str(&String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(text.cast::<u8>(), len) }));
    }
}

/// Pushes each string the shim sends onto the `Vec<String>` at `user`.
unsafe extern "C" fn push_string(user: *mut c_void, text: *const c_char, len: usize) {
    let mut line = String::new();
    unsafe { append_string((&raw mut line).cast(), text, len) };
    unsafe { (*user.cast::<Vec<String>>()).push(line) };
}

/// A string the shim sends to `call`'s sink.
pub(super) fn received(call: impl FnOnce(sys::mqemu_string_sink, *mut c_void)) -> String {
    let mut out = String::new();
    call(append_string, (&raw mut out).cast());
    out
}

/// Strings as the shim's parallel arrays, borrowing them.
pub(super) struct Strings<'s> {
    texts: Vec<*const c_char>,
    lens: Vec<usize>,
    _strings: std::marker::PhantomData<&'s str>,
}

impl<'s> Strings<'s> {
    pub(super) fn new(strings: impl IntoIterator<Item = &'s str>) -> Strings<'s> {
        let (texts, lens) = strings.into_iter().map(s).unzip();
        Strings { texts, lens, _strings: std::marker::PhantomData }
    }

    pub(super) fn raw(&self) -> sys::mqemu_strings {
        sys::mqemu_strings { texts: self.texts.as_ptr(), lens: self.lens.as_ptr(), count: self.texts.len() }
    }
}

/// A place in the text: a line and a column, both from 0. Columns count
/// what's shown, so a tab takes the columns up to the next tab stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct TextPosition {
    pub line: usize,
    pub column: usize,
}

impl TextPosition {
    pub const fn new(line: usize, column: usize) -> TextPosition {
        TextPosition { line, column }
    }

    fn raw(self) -> (i32, i32) {
        (clamp_i32(self.line), clamp_i32(self.column))
    }

    fn from_raw(line: i32, column: i32) -> TextPosition {
        TextPosition { line: usize::try_from(line).unwrap_or(0), column: usize::try_from(column).unwrap_or(0) }
    }
}

fn clamp_i32(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// The languages the editor knows (MacroQuest's `LanguageDefinition::*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Language {
    CPlusPlus = sys::MQEMU_LANGUAGE_CPLUSPLUS,
    Hlsl = sys::MQEMU_LANGUAGE_HLSL,
    Glsl = sys::MQEMU_LANGUAGE_GLSL,
    C = sys::MQEMU_LANGUAGE_C,
    Sql = sys::MQEMU_LANGUAGE_SQL,
    Lua = sys::MQEMU_LANGUAGE_LUA,
    /// No highlighting.
    PlainText = sys::MQEMU_LANGUAGE_PLAIN_TEXT,
}

/// What a palette color is for (`PaletteIndex`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum PaletteIndex {
    Default = sys::PaletteIndex_Default,
    Keyword = sys::PaletteIndex_Keyword,
    Number = sys::PaletteIndex_Number,
    String = sys::PaletteIndex_String,
    CharLiteral = sys::PaletteIndex_CharLiteral,
    Punctuation = sys::PaletteIndex_Punctuation,
    Preprocessor = sys::PaletteIndex_Preprocessor,
    Identifier = sys::PaletteIndex_Identifier,
    /// A [`LanguageDefinition::identifiers`] name.
    KnownIdentifier = sys::PaletteIndex_KnownIdentifier,
    PreprocIdentifier = sys::PaletteIndex_PreprocIdentifier,
    Comment = sys::PaletteIndex_Comment,
    MultiLineComment = sys::PaletteIndex_MultiLineComment,
    Background = sys::PaletteIndex_Background,
    Cursor = sys::PaletteIndex_Cursor,
    Selection = sys::PaletteIndex_Selection,
    ErrorMarker = sys::PaletteIndex_ErrorMarker,
    Breakpoint = sys::PaletteIndex_Breakpoint,
    LineNumber = sys::PaletteIndex_LineNumber,
    CurrentLineFill = sys::PaletteIndex_CurrentLineFill,
    CurrentLineFillInactive = sys::PaletteIndex_CurrentLineFillInactive,
    CurrentLineEdge = sys::PaletteIndex_CurrentLineEdge,
}

impl PaletteIndex {
    /// Every one, in palette order.
    pub const ALL: [PaletteIndex; PALETTE_LEN] = {
        use PaletteIndex::*;
        [
            Default,
            Keyword,
            Number,
            String,
            CharLiteral,
            Punctuation,
            Preprocessor,
            Identifier,
            KnownIdentifier,
            PreprocIdentifier,
            Comment,
            MultiLineComment,
            Background,
            Cursor,
            Selection,
            ErrorMarker,
            Breakpoint,
            LineNumber,
            CurrentLineFill,
            CurrentLineFillInactive,
            CurrentLineEdge,
        ]
    };
}

const PALETTE_LEN: usize = sys::PaletteIndex_Max as usize;

/// The editor's built-in palettes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum PaletteKind {
    /// The default.
    Dark = sys::MQEMU_PALETTE_DARK,
    Light = sys::MQEMU_PALETTE_LIGHT,
    RetroBlue = sys::MQEMU_PALETTE_RETRO_BLUE,
}

/// The editor's colors, one for each [`PaletteIndex`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette(pub [Color; PALETTE_LEN]);

impl Palette {
    fn from_raw(raw: &[u32; PALETTE_LEN]) -> Palette {
        Palette(raw.map(unpack_color))
    }

    fn raw(&self) -> [u32; PALETTE_LEN] {
        self.0.map(pack_color)
    }
}

impl Index<PaletteIndex> for Palette {
    type Output = Color;

    fn index(&self, index: PaletteIndex) -> &Color {
        &self.0[index as usize]
    }
}

impl IndexMut<PaletteIndex> for Palette {
    fn index_mut(&mut self, index: PaletteIndex) -> &mut Color {
        &mut self.0[index as usize]
    }
}

/// An `ImU32` color as RGBA floats; [`pack_color`] gives it back exactly.
fn unpack_color(packed: u32) -> Color {
    [0, 8, 16, 24].map(|shift| ((packed >> shift) & 0xff) as f32 / 255.0)
}

/// Finds the token at or after the start of `text` (the rest of a line):
/// its byte range in `text` and its color, or `None` to leave it to the
/// token regexes. The editor moves on to the end of the token, or one byte
/// on for `None` (or an empty range), so `text` can start inside a
/// multi-byte character: then it's called with the `str` that follows it.
pub type Tokenizer = fn(text: &str) -> Option<(Range<usize>, PaletteIndex)>;

/// A language of your own to highlight ([`TextEditor::set_language_definition`]).
#[derive(Debug, Clone)]
pub struct LanguageDefinition {
    /// What [`TextEditor::language_name`] reports.
    pub name: String,
    /// Words shown as [`PaletteIndex::Keyword`].
    pub keywords: Vec<String>,
    /// Names shown as [`PaletteIndex::KnownIdentifier`], each with what's
    /// shown when it's hovered.
    pub identifiers: Vec<(String, String)>,
    /// Names after [`preproc_char`](Self::preproc_char), shown as
    /// [`PaletteIndex::PreprocIdentifier`], each with its hover text.
    pub preproc_identifiers: Vec<(String, String)>,
    /// Multi-line comments, from this to [`comment_end`](Self::comment_end)
    /// (both empty for none).
    pub comment_start: String,
    pub comment_end: String,
    /// The rest of a line after this is a comment (empty for none).
    pub single_line_comment: String,
    /// A line starting with this is [`PaletteIndex::Preprocessor`].
    pub preproc_char: u8,
    /// A new line starts as indented as the one before.
    pub auto_indentation: bool,
    /// How tokens are found: ECMAScript regular expressions, tried in
    /// order at each place, each with the color of what it matches.
    pub token_regexes: Vec<(String, PaletteIndex)>,
    /// Finds tokens before the regexes are tried, as the built-in languages
    /// do. Much faster: the editor colors 10,000 lines a frame with one,
    /// but only 10 with regexes alone. An [`PaletteIndex::Identifier`]
    /// token is still checked against the keywords and identifiers.
    pub tokenizer: Option<Tokenizer>,
    /// Whether keywords and identifiers must match case.
    pub case_sensitive: bool,
}

impl Default for LanguageDefinition {
    /// Nothing highlighted, as the editor's own defaults.
    fn default() -> Self {
        LanguageDefinition {
            name: String::new(),
            keywords: Vec::new(),
            identifiers: Vec::new(),
            preproc_identifiers: Vec::new(),
            comment_start: String::new(),
            comment_end: String::new(),
            single_line_comment: String::new(),
            preproc_char: b'#',
            auto_indentation: true,
            token_regexes: Vec::new(),
            tokenizer: None,
            case_sensitive: true,
        }
    }
}

/// How [`TextEditor::set_selection`] widens a selection (`SelectionMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(i32)]
pub enum SelectionMode {
    /// As given.
    #[default]
    Normal = sys::SelectionMode_Normal,
    /// Out to whole words.
    Word = sys::SelectionMode_Word,
    /// Out to whole lines.
    Line = sys::SelectionMode_Line,
}

/// MacroQuest's code editor ([`Ui::new_text_editor`]): syntax highlighting
/// for the [`Language`]s it knows or one of your own, line numbers, undo and
/// redo, the clipboard, error markers and breakpoints. Keep it between
/// frames and [`render`](TextEditor::render) it each one.
///
/// Its undo steps are what the user does (typing, cutting, pasting,
/// [`delete`](TextEditor::delete)); text set or inserted from code isn't
/// one, and [`set_text`](TextEditor::set_text) forgets them. Inserting
/// while there are undo steps leaves them pointing at the old text, as the
/// editor does.
pub struct TextEditor {
    raw: NonNull<sys::TextEditor>,
    api: &'static sys::Api,
    /// The editor's own getter reports the keyboard setting instead.
    handle_mouse: bool,
    /// The language's tokenizer, called while rendering.
    tokenizer: Option<Tokenizer>,
}

impl Ui {
    /// A text editor, empty, highlighting nothing until given a language, in
    /// the dark palette. Make it once and keep it.
    pub fn new_text_editor(&self) -> TextEditor {
        let raw = NonNull::new(unsafe { (self.addons.mqemu_text_editor_new)() }).expect("TextEditor's constructor threw");
        TextEditor { raw, api: self.addons, handle_mouse: true, tokenizer: None }
    }
}

impl TextEditor {
    fn ptr(&self) -> *mut sys::TextEditor {
        self.raw.as_ptr()
    }

    /// Draws it, in a child window `id` of `size` (0 for the room left on
    /// that axis, negative for that much less), framed if `border`; with
    /// [`set_ignore_child`](TextEditor::set_ignore_child), in the current
    /// window instead. A panic in the language's tokenizer comes out after
    /// it's drawn.
    pub fn render(&mut self, ui: &mut Ui, id: &str, size: Vec2, border: bool) {
        let _ = ui;
        let (i, n) = s(id);
        let mut tokenizer = self.tokenizer.map(Callback::new);
        let (call, user) = match tokenizer.as_mut() {
            Some(callback) => (Some(tokenize as sys::mqemu_tokenizer_fn), callback.data()),
            None => (None, std::ptr::null_mut()),
        };
        unsafe { (self.api.mqemu_text_editor_render)(self.ptr(), i, n, size[0], size[1], border, call, user) }
        if let Some(panic) = tokenizer.as_mut().and_then(Kept::take_panic) {
            std::panic::resume_unwind(panic);
        }
    }

    // ----- language and colors -----

    pub fn set_language(&mut self, language: Language) {
        self.tokenizer = None;
        unsafe { (self.api.mqemu_text_editor_set_language)(self.ptr(), language as i32) }
    }

    /// Highlights `language`. Err with why if one of its token regexes
    /// doesn't compile; the language stays as it was then.
    pub fn set_language_definition(&mut self, language: &LanguageDefinition) -> Result<(), String> {
        let keywords = Strings::new(language.keywords.iter().map(String::as_str));
        let identifiers = Strings::new(language.identifiers.iter().map(|(name, _)| name.as_str()));
        let declarations = Strings::new(language.identifiers.iter().map(|(_, text)| text.as_str()));
        let preproc_identifiers = Strings::new(language.preproc_identifiers.iter().map(|(name, _)| name.as_str()));
        let preproc_declarations = Strings::new(language.preproc_identifiers.iter().map(|(_, text)| text.as_str()));
        let token_regexes = Strings::new(language.token_regexes.iter().map(|(regex, _)| regex.as_str()));
        let token_palette: Vec<i32> = language.token_regexes.iter().map(|(_, index)| *index as i32).collect();
        let (name, name_len) = s(&language.name);
        let (comment_start, comment_start_len) = s(&language.comment_start);
        let (comment_end, comment_end_len) = s(&language.comment_end);
        let (single_line_comment, single_line_comment_len) = s(&language.single_line_comment);
        let raw = sys::mqemu_text_editor_language {
            name,
            name_len,
            keywords: keywords.raw(),
            identifiers: identifiers.raw(),
            declarations: declarations.raw(),
            preproc_identifiers: preproc_identifiers.raw(),
            preproc_declarations: preproc_declarations.raw(),
            comment_start,
            comment_start_len,
            comment_end,
            comment_end_len,
            single_line_comment,
            single_line_comment_len,
            preproc_char: language.preproc_char as c_char,
            auto_indentation: language.auto_indentation,
            token_regexes: token_regexes.raw(),
            token_palette: token_palette.as_ptr(),
            case_sensitive: language.case_sensitive,
            tokenizer: language.tokenizer.is_some(),
        };
        let mut error = String::new();
        let set = unsafe { (self.api.mqemu_text_editor_set_language_definition)(self.ptr(), &raw, append_string, (&raw mut error).cast()) };
        if !set {
            return Err(error);
        }
        self.tokenizer = language.tokenizer;
        Ok(())
    }

    /// The name of the language it highlights (`"Lua"`, say).
    pub fn language_name(&self) -> String {
        received(|sink, user| unsafe { (self.api.mqemu_text_editor_language_name)(self.ptr(), sink, user) })
    }

    pub fn palette(&self) -> Palette {
        let mut raw = [0; PALETTE_LEN];
        unsafe { (self.api.mqemu_text_editor_palette)(self.ptr(), raw.as_mut_ptr()) };
        Palette::from_raw(&raw)
    }

    pub fn set_palette(&mut self, palette: &Palette) {
        unsafe { (self.api.mqemu_text_editor_set_palette)(self.ptr(), palette.raw().as_ptr()) }
    }

    /// One of the editor's own palettes, to set or start from.
    pub fn builtin_palette(&self, kind: PaletteKind) -> Palette {
        let mut raw = [0; PALETTE_LEN];
        unsafe { (self.api.mqemu_text_editor_builtin_palette)(kind as i32, raw.as_mut_ptr()) };
        Palette::from_raw(&raw)
    }

    /// Marks lines (from 0, as [`TextPosition::line`]) as errors, shown in
    /// [`PaletteIndex::ErrorMarker`] with the message when hovered. Replaces
    /// the markers before; none clears them.
    pub fn set_error_markers<'m>(&mut self, markers: impl IntoIterator<Item = (usize, &'m str)>) {
        let (lines, messages): (Vec<i32>, Vec<&str>) = markers.into_iter().map(|(line, message)| (shown_line(line), message)).unzip();
        let messages = Strings::new(messages);
        unsafe { (self.api.mqemu_text_editor_set_error_markers)(self.ptr(), lines.as_ptr(), messages.raw()) }
    }

    /// Marks lines (from 0) as breakpoints, in [`PaletteIndex::Breakpoint`].
    /// Replaces those before.
    pub fn set_breakpoints(&mut self, lines: impl IntoIterator<Item = usize>) {
        let lines: Vec<i32> = lines.into_iter().map(shown_line).collect();
        unsafe { (self.api.mqemu_text_editor_set_breakpoints)(self.ptr(), lines.as_ptr(), lines.len()) }
    }

    // ----- text -----

    /// Replaces the text (carriage returns dropped) and forgets the undo steps.
    pub fn set_text(&mut self, text: &str) {
        let (t, n) = s(text);
        unsafe { (self.api.mqemu_text_editor_set_text)(self.ptr(), t, n) }
    }

    /// [`set_text`](TextEditor::set_text) in `color`, not highlighted: for
    /// logs and the like.
    pub fn set_text_colored(&mut self, text: &str, color: Color) {
        let (t, n) = s(text);
        unsafe { (self.api.mqemu_text_editor_set_text_colored)(self.ptr(), t, n, pack_color(color)) }
    }

    pub fn set_lines<S: AsRef<str>>(&mut self, lines: &[S]) {
        let lines = Strings::new(lines.iter().map(AsRef::as_ref));
        unsafe { (self.api.mqemu_text_editor_set_text_lines)(self.ptr(), lines.raw()) }
    }

    /// Inserts `text` at the cursor, replacing nothing, and moves the cursor
    /// after it. Even when [read only](TextEditor::set_read_only), which
    /// only stops the user; not an undo step.
    pub fn insert_text(&mut self, text: &str) {
        let (t, n) = s(text);
        unsafe { (self.api.mqemu_text_editor_insert_text)(self.ptr(), t, n) }
    }

    /// [`insert_text`](TextEditor::insert_text) in `color`, not highlighted.
    pub fn insert_text_colored(&mut self, text: &str, color: Color) {
        let (t, n) = s(text);
        unsafe { (self.api.mqemu_text_editor_insert_text_colored)(self.ptr(), t, n, pack_color(color)) }
    }

    /// All of it, lines joined with `\n`.
    pub fn text(&self) -> String {
        received(|sink, user| unsafe { (self.api.mqemu_text_editor_text)(self.ptr(), sink, user) })
    }

    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::<String>::new();
        unsafe { (self.api.mqemu_text_editor_text_lines)(self.ptr(), push_string, (&raw mut lines).cast()) };
        lines
    }

    /// What's selected (empty for nothing).
    pub fn selected_text(&self) -> String {
        received(|sink, user| unsafe { (self.api.mqemu_text_editor_selected_text)(self.ptr(), sink, user) })
    }

    /// The cursor's line.
    pub fn current_line_text(&self) -> String {
        received(|sink, user| unsafe { (self.api.mqemu_text_editor_current_line_text)(self.ptr(), sink, user) })
    }

    /// Lines it has (at least 1: empty text is one empty line).
    pub fn line_count(&self) -> usize {
        usize::try_from(unsafe { (self.api.mqemu_text_editor_total_lines)(self.ptr()) }).unwrap_or(0)
    }

    /// Whether the text changed (from code or the user) since the last
    /// [`render`](TextEditor::render) started.
    pub fn is_text_changed(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_is_text_changed)(self.ptr()) }
    }

    /// Whether the cursor moved during the last render.
    pub fn is_cursor_position_changed(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_is_cursor_position_changed)(self.ptr()) }
    }

    /// Whether typing overwrites (the Insert key toggles it).
    pub fn is_overwrite(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_is_overwrite)(self.ptr()) }
    }

    // ----- settings -----

    pub fn is_read_only(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_is_read_only)(self.ptr()) }
    }

    /// Stops the user changing the text (and undoing); code still can.
    pub fn set_read_only(&mut self, value: bool) {
        unsafe { (self.api.mqemu_text_editor_set_read_only)(self.ptr(), value) }
    }

    pub fn is_colorizer_enabled(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_is_colorizer_enabled)(self.ptr()) }
    }

    /// Highlighting on or off.
    pub fn set_colorizer_enabled(&mut self, value: bool) {
        unsafe { (self.api.mqemu_text_editor_set_colorizer_enabled)(self.ptr(), value) }
    }

    /// Whether the cursor is drawn (on by default).
    pub fn set_show_cursor(&mut self, value: bool) {
        unsafe { (self.api.mqemu_text_editor_set_render_cursor)(self.ptr(), value) }
    }

    /// Whether line numbers are drawn (on by default).
    pub fn set_show_line_numbers(&mut self, value: bool) {
        unsafe { (self.api.mqemu_text_editor_set_render_line_numbers)(self.ptr(), value) }
    }

    pub fn handles_mouse(&self) -> bool {
        self.handle_mouse
    }

    /// Whether it reacts to the mouse (on by default).
    pub fn set_handle_mouse(&mut self, value: bool) {
        self.handle_mouse = value;
        unsafe { (self.api.mqemu_text_editor_set_handle_mouse_inputs)(self.ptr(), value) }
    }

    pub fn handles_keyboard(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_is_handle_keyboard_inputs)(self.ptr()) }
    }

    /// Whether it reacts to the keyboard when focused (on by default).
    pub fn set_handle_keyboard(&mut self, value: bool) {
        unsafe { (self.api.mqemu_text_editor_set_handle_keyboard_inputs)(self.ptr(), value) }
    }

    pub fn ignores_child(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_is_imgui_child_ignored)(self.ptr()) }
    }

    /// Draws in the current window rather than a child window of its own.
    pub fn set_ignore_child(&mut self, value: bool) {
        unsafe { (self.api.mqemu_text_editor_set_imgui_child_ignored)(self.ptr(), value) }
    }

    pub fn shows_whitespace(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_is_showing_whitespace)(self.ptr()) }
    }

    /// Shows spaces and tabs as dots and arrows (on by default).
    pub fn set_show_whitespace(&mut self, value: bool) {
        unsafe { (self.api.mqemu_text_editor_set_show_whitespace)(self.ptr(), value) }
    }

    pub fn shows_short_tab_glyphs(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_is_showing_short_tab_glyphs)(self.ptr()) }
    }

    /// Draws tabs' arrows short rather than across the tab.
    pub fn set_show_short_tab_glyphs(&mut self, value: bool) {
        unsafe { (self.api.mqemu_text_editor_set_show_short_tab_glyphs)(self.ptr(), value) }
    }

    pub fn tab_size(&self) -> usize {
        usize::try_from(unsafe { (self.api.mqemu_text_editor_tab_size)(self.ptr()) }).unwrap_or(1)
    }

    /// Columns between tab stops, 1 to 32 (4 by default).
    pub fn set_tab_size(&mut self, value: usize) {
        unsafe { (self.api.mqemu_text_editor_set_tab_size)(self.ptr(), clamp_i32(value.clamp(1, 32))) }
    }

    // ----- cursor and selection -----

    pub fn cursor_position(&self) -> TextPosition {
        let (mut line, mut column) = (0, 0);
        unsafe { (self.api.mqemu_text_editor_cursor_position)(self.ptr(), &mut line, &mut column) };
        TextPosition::from_raw(line, column)
    }

    /// Moves the cursor (kept inside the text) and scrolls to it.
    pub fn set_cursor_position(&mut self, position: TextPosition) {
        let (line, column) = position.raw();
        unsafe { (self.api.mqemu_text_editor_set_cursor_position)(self.ptr(), line, column) }
    }

    /// The cursor moves as the arrow keys move it; with `select`, as with shift held.
    pub fn move_up(&mut self, lines: usize, select: bool) {
        unsafe { (self.api.mqemu_text_editor_move_up)(self.ptr(), clamp_i32(lines), select) }
    }

    pub fn move_down(&mut self, lines: usize, select: bool) {
        unsafe { (self.api.mqemu_text_editor_move_down)(self.ptr(), clamp_i32(lines), select) }
    }

    /// `by_word` moves a word at a time, as with ctrl held.
    pub fn move_left(&mut self, amount: usize, select: bool, by_word: bool) {
        unsafe { (self.api.mqemu_text_editor_move_left)(self.ptr(), clamp_i32(amount), select, by_word) }
    }

    pub fn move_right(&mut self, amount: usize, select: bool, by_word: bool) {
        unsafe { (self.api.mqemu_text_editor_move_right)(self.ptr(), clamp_i32(amount), select, by_word) }
    }

    /// To the start of the text.
    pub fn move_top(&mut self, select: bool) {
        unsafe { (self.api.mqemu_text_editor_move_top)(self.ptr(), select) }
    }

    /// To the end of the text.
    pub fn move_bottom(&mut self, select: bool) {
        unsafe { (self.api.mqemu_text_editor_move_bottom)(self.ptr(), select) }
    }

    /// To the start of the line.
    pub fn move_home(&mut self, select: bool) {
        unsafe { (self.api.mqemu_text_editor_move_home)(self.ptr(), select) }
    }

    /// To the end of the line.
    pub fn move_end(&mut self, select: bool) {
        unsafe { (self.api.mqemu_text_editor_move_end)(self.ptr(), select) }
    }

    /// The selection's start and end, equal when nothing is selected.
    pub fn selection(&self) -> (TextPosition, TextPosition) {
        let (mut start_line, mut start_column, mut end_line, mut end_column) = (0, 0, 0, 0);
        unsafe { (self.api.mqemu_text_editor_selection)(self.ptr(), &mut start_line, &mut start_column, &mut end_line, &mut end_column) };
        (TextPosition::from_raw(start_line, start_column), TextPosition::from_raw(end_line, end_column))
    }

    pub fn set_selection_start(&mut self, position: TextPosition) {
        let (line, column) = position.raw();
        unsafe { (self.api.mqemu_text_editor_set_selection_start)(self.ptr(), line, column) }
    }

    pub fn set_selection_end(&mut self, position: TextPosition) {
        let (line, column) = position.raw();
        unsafe { (self.api.mqemu_text_editor_set_selection_end)(self.ptr(), line, column) }
    }

    /// Selects from `start` to `end` (in either order), widened as `mode` says.
    pub fn set_selection(&mut self, start: TextPosition, end: TextPosition, mode: SelectionMode) {
        let ((start_line, start_column), (end_line, end_column)) = (start.raw(), end.raw());
        unsafe { (self.api.mqemu_text_editor_set_selection)(self.ptr(), start_line, start_column, end_line, end_column, mode as i32) }
    }

    pub fn select_word_under_cursor(&mut self) {
        unsafe { (self.api.mqemu_text_editor_select_word_under_cursor)(self.ptr()) }
    }

    pub fn select_all(&mut self) {
        unsafe { (self.api.mqemu_text_editor_select_all)(self.ptr()) }
    }

    pub fn has_selection(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_has_selection)(self.ptr()) }
    }

    // ----- editing -----

    /// Copies the selection (or the cursor's line) to the clipboard.
    pub fn copy(&mut self) {
        unsafe { (self.api.mqemu_text_editor_copy)(self.ptr()) }
    }

    /// Cuts the selection to the clipboard (not when read only).
    pub fn cut(&mut self) {
        unsafe { (self.api.mqemu_text_editor_cut)(self.ptr()) }
    }

    /// Pastes from the clipboard over the selection (not when read only).
    pub fn paste(&mut self) {
        unsafe { (self.api.mqemu_text_editor_paste)(self.ptr()) }
    }

    /// Deletes the selection, or the character after the cursor, as the
    /// Delete key does (not when read only).
    pub fn delete(&mut self) {
        unsafe { (self.api.mqemu_text_editor_delete_forward)(self.ptr()) }
    }

    pub fn can_undo(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_can_undo)(self.ptr()) }
    }

    pub fn can_redo(&self) -> bool {
        unsafe { (self.api.mqemu_text_editor_can_redo)(self.ptr()) }
    }

    /// Undoes up to `steps` (none when read only).
    pub fn undo(&mut self, steps: usize) {
        unsafe { (self.api.mqemu_text_editor_undo)(self.ptr(), clamp_i32(steps)) }
    }

    pub fn redo(&mut self, steps: usize) {
        unsafe { (self.api.mqemu_text_editor_redo)(self.ptr(), clamp_i32(steps)) }
    }
}

/// A line from 0 as the editor numbers markers: from 1, as shown.
fn shown_line(line: usize) -> i32 {
    clamp_i32(line.saturating_add(1))
}

impl Drop for TextEditor {
    fn drop(&mut self) {
        unsafe { (self.api.mqemu_text_editor_delete)(self.ptr()) }
    }
}

impl std::fmt::Debug for TextEditor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextEditor")
            .field("language", &self.language_name())
            .field("lines", &self.line_count())
            .field("cursor", &self.cursor_position())
            .finish_non_exhaustive()
    }
}

/// The language's [`Tokenizer`], called by the editor while it renders.
unsafe extern "C" fn tokenize(user: *mut c_void, text: *const c_char, len: usize, start: *mut usize, end: *mut usize, palette: *mut i32) -> bool {
    let state = unsafe { &mut *user.cast::<Callback<Tokenizer>>() };
    let bytes = unsafe { std::slice::from_raw_parts(text.cast::<u8>(), len) };
    // Past the bytes of a character the editor stepped into.
    let skip = bytes.iter().take_while(|&&b| b & 0xC0 == 0x80).count();
    let Ok(rest) = std::str::from_utf8(&bytes[skip..]) else { return false };
    let Some((range, index)) = state.run(None, |tokenizer| tokenizer(rest)) else { return false };
    if range.start >= range.end || range.end > rest.len() {
        return false;
    }
    unsafe {
        *start = skip + range.start;
        *end = skip + range.end;
        *palette = index as i32;
    }
    true
}
