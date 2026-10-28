//! Drives MacroQuest's ImGui add-ons (shim/imgui_addons.cpp) against the real
//! imgui.dll headlessly. The text editor is a C++ class whose inline methods
//! the shim compiles: values set through imgui.dll's methods and read back
//! through inline ones (and the other way) show the two agree on its layout.

use core::ffi::{c_char, c_void};
use std::sync::Mutex;

use mq_emu_sys::imgui::testing::*;
use mq_emu_sys::imgui::*;
use mq_emu_sys::imgui_addons::*;

static IMGUI: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    let guard = IMGUI.lock().unwrap_or_else(|e| e.into_inner());
    unsafe { mqemu_imgui_test_init() };
    take_error();
    guard
}

fn frame(draw: impl FnOnce()) -> Result<(), String> {
    assert!(unsafe { mqemu_imgui_test_new_frame(0.0, 0.0, false) }, "NewFrame threw: {:?}", take_error());
    draw();
    let mut error = [0 as c_char; 512];
    if unsafe { mqemu_imgui_test_end_frame(error.as_mut_ptr(), error.len()) } { Ok(()) } else { Err(text(&error)) }
}

fn text(buffer: &[c_char]) -> String {
    let bytes: Vec<u8> = buffer.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn take_error() -> Option<String> {
    let mut error = [0 as c_char; 512];
    unsafe { mqemu_imgui_take_error(error.as_mut_ptr(), error.len()) }.then(|| text(&error))
}

fn s(text: &str) -> (*const c_char, usize) {
    (text.as_ptr().cast(), text.len())
}

fn begin(name: &str) {
    let (p, n) = s(name);
    unsafe {
        mqemu_imgui_set_next_window_pos(10.0, 10.0, ImGuiCond_Always, 0.0, 0.0);
        mqemu_imgui_set_next_window_size(600.0, 500.0, ImGuiCond_Always);
        mqemu_imgui_begin(p, n, core::ptr::null_mut(), ImGuiWindowFlags_NoSavedSettings);
    }
}

unsafe extern "C" fn push(user: *mut c_void, text: *const c_char, len: usize) {
    let out = unsafe { &mut *user.cast::<Vec<String>>() };
    let bytes = if len == 0 { &[][..] } else { unsafe { std::slice::from_raw_parts(text.cast::<u8>(), len) } };
    out.push(String::from_utf8_lossy(bytes).into_owned());
}

/// What `call` sends to a sink, one string per call.
fn collect(call: impl FnOnce(mqemu_string_sink, *mut c_void)) -> Vec<String> {
    let mut out = Vec::<String>::new();
    call(push, (&raw mut out).cast());
    out
}

fn one(call: impl FnOnce(mqemu_string_sink, *mut c_void)) -> String {
    let mut out = collect(call);
    assert_eq!(out.len(), 1, "sent {out:?}");
    out.remove(0)
}

struct Strings {
    ptrs: Vec<*const c_char>,
    lens: Vec<usize>,
}

impl Strings {
    fn new(texts: &[&str]) -> Strings {
        let (ptrs, lens) = texts.iter().map(|t| s(t)).unzip();
        Strings { ptrs, lens }
    }

    fn raw(&self) -> mqemu_strings {
        mqemu_strings { texts: self.ptrs.as_ptr(), lens: self.lens.as_ptr(), count: self.ptrs.len() }
    }
}

struct Editor(*mut TextEditor);

impl Drop for Editor {
    fn drop(&mut self) {
        unsafe { mqemu_text_editor_delete(self.0) }
    }
}

fn editor() -> Editor {
    let raw = unsafe { mqemu_text_editor_new() };
    assert!(!raw.is_null());
    Editor(raw)
}

fn editor_text(e: &Editor) -> String {
    one(|sink, user| unsafe { mqemu_text_editor_text(e.0, sink, user) })
}

#[test]
fn text_editor_fields_read_inline_match_what_imgui_dll_set() {
    let _imgui = lock();
    let e = editor();
    unsafe {
        // Each pair: an exported setter (imgui.dll writes the field), an
        // inline getter (the shim reads it at its own offset).
        mqemu_text_editor_set_tab_size(e.0, 7);
        assert_eq!(mqemu_text_editor_tab_size(e.0), 7);
        mqemu_text_editor_set_read_only(e.0, true);
        assert!(mqemu_text_editor_is_read_only(e.0));
        mqemu_text_editor_set_colorizer_enabled(e.0, false);
        assert!(!mqemu_text_editor_is_colorizer_enabled(e.0));
        mqemu_text_editor_set_language(e.0, MQEMU_LANGUAGE_LUA);
        assert_eq!(one(|sink, user| mqemu_text_editor_language_name(e.0, sink, user)), "Lua");
        mqemu_text_editor_set_language(e.0, MQEMU_LANGUAGE_SQL);
        assert_eq!(one(|sink, user| mqemu_text_editor_language_name(e.0, sink, user)), "SQL");

        let mut light = [0u32; PaletteIndex_Max as usize];
        mqemu_text_editor_builtin_palette(MQEMU_PALETTE_LIGHT, light.as_mut_ptr());
        let mut dark = [0u32; PaletteIndex_Max as usize];
        mqemu_text_editor_builtin_palette(MQEMU_PALETTE_DARK, dark.as_mut_ptr());
        assert_ne!(light, dark);
        mqemu_text_editor_set_palette(e.0, light.as_ptr());
        let mut read = [0u32; PaletteIndex_Max as usize];
        mqemu_text_editor_palette(e.0, read.as_mut_ptr());
        assert_eq!(read, light);

        // Inline setters, read inline: these only show the fields are distinct.
        mqemu_text_editor_set_show_whitespace(e.0, false);
        assert!(!mqemu_text_editor_is_showing_whitespace(e.0));
        mqemu_text_editor_set_show_short_tab_glyphs(e.0, true);
        assert!(mqemu_text_editor_is_showing_short_tab_glyphs(e.0));
        mqemu_text_editor_set_handle_keyboard_inputs(e.0, false);
        assert!(!mqemu_text_editor_is_handle_keyboard_inputs(e.0));
        mqemu_text_editor_set_imgui_child_ignored(e.0, true);
        assert!(mqemu_text_editor_is_imgui_child_ignored(e.0));
        assert_eq!(mqemu_text_editor_tab_size(e.0), 7, "an inline setter wrote over the tab size");
    }
}

#[test]
fn text_editor_text_round_trips_as_one_string_and_as_lines() {
    let _imgui = lock();
    let e = editor();
    unsafe {
        let (p, n) = s("first\r\nsecond\nthird");
        mqemu_text_editor_set_text(e.0, p, n);
        assert_eq!(mqemu_text_editor_total_lines(e.0), 3);
        assert_eq!(editor_text(&e), "first\nsecond\nthird", "carriage returns are dropped");
        assert_eq!(collect(|sink, user| mqemu_text_editor_text_lines(e.0, sink, user)), ["first", "second", "third"]);
        assert!(mqemu_text_editor_is_text_changed(e.0));

        let lines = Strings::new(&["a", "", "ccc"]);
        mqemu_text_editor_set_text_lines(e.0, lines.raw());
        assert_eq!(editor_text(&e), "a\n\nccc");
        let (p, n) = s("");
        mqemu_text_editor_set_text(e.0, p, n);
        assert_eq!(mqemu_text_editor_total_lines(e.0), 1);
        assert_eq!(editor_text(&e), "");
    }
}

#[test]
fn text_editor_cursor_selection_and_undo() {
    let _imgui = lock();
    let e = editor();
    unsafe {
        let (p, n) = s("one\n\ttwo three");
        mqemu_text_editor_set_text(e.0, p, n);
        // A tab takes the columns to the next tab stop (4 by default).
        mqemu_text_editor_set_cursor_position(e.0, 1, 6);
        let (mut line, mut column) = (0, 0);
        mqemu_text_editor_cursor_position(e.0, &mut line, &mut column);
        assert_eq!((line, column), (1, 6));
        // Past the end of the text: clamped, as the editor's own getter does.
        mqemu_text_editor_set_cursor_position(e.0, 1, 99);
        mqemu_text_editor_cursor_position(e.0, &mut line, &mut column);
        assert_eq!((line, column), (1, 4 + "two three".len() as i32));
        assert_eq!(one(|sink, user| mqemu_text_editor_current_line_text(e.0, sink, user)), "\ttwo three");

        assert!(!mqemu_text_editor_has_selection(e.0));
        mqemu_text_editor_set_selection(e.0, 1, 4, 1, 7, SelectionMode_Normal);
        assert!(mqemu_text_editor_has_selection(e.0));
        assert_eq!(one(|sink, user| mqemu_text_editor_selected_text(e.0, sink, user)), "two");
        let mut selection = [0; 4];
        let [a, b, c, d] = &mut selection;
        mqemu_text_editor_selection(e.0, a, b, c, d);
        assert_eq!(selection, [1, 4, 1, 7]);
        mqemu_text_editor_set_selection(e.0, 0, 1, 0, 1, SelectionMode_Word);
        assert_eq!(one(|sink, user| mqemu_text_editor_selected_text(e.0, sink, user)), "one");

        assert!(!mqemu_text_editor_can_undo(e.0), "setting the text clears the undo buffer");
        mqemu_text_editor_set_selection(e.0, 1, 4, 1, 8, SelectionMode_Normal);
        mqemu_text_editor_delete_forward(e.0);
        assert_eq!(editor_text(&e), "one\n\tthree");
        assert!(mqemu_text_editor_can_undo(e.0));
        mqemu_text_editor_undo(e.0, 1);
        assert_eq!(editor_text(&e), "one\n\ttwo three");
        assert!(mqemu_text_editor_can_redo(e.0));
        mqemu_text_editor_redo(e.0, 1);
        assert_eq!(editor_text(&e), "one\n\tthree");
        mqemu_text_editor_select_all(e.0);
        mqemu_text_editor_delete_forward(e.0);
        assert_eq!(editor_text(&e), "");

        // Read only stops the user, not code; an insert isn't an undo step.
        mqemu_text_editor_set_read_only(e.0, true);
        let (p, n) = s("inserted");
        mqemu_text_editor_insert_text(e.0, p, n);
        assert_eq!(editor_text(&e), "inserted");
        assert!(!mqemu_text_editor_can_undo(e.0), "read only allows no undo");
        mqemu_text_editor_set_read_only(e.0, false);
    }
}

/// Deletes "two " from "one two three" and returns the editor.
fn editor_with_a_deletion() -> Editor {
    let e = editor();
    unsafe {
        let (p, n) = s("one two three");
        mqemu_text_editor_set_text(e.0, p, n);
        mqemu_text_editor_set_selection(e.0, 0, 4, 0, 8, SelectionMode_Normal);
        mqemu_text_editor_delete_forward(e.0);
    }
    assert_eq!(editor_text(&e), "one three");
    e
}

#[test]
fn the_editors_own_redo_does_nothing_after_an_undo() {
    // MacroQuest's UndoRecord::Undo moves the record's start, so the redo
    // deletes an empty range. If this starts failing, MacroQuest fixed it and
    // UndoStarts can go.
    let _imgui = lock();
    let e = editor_with_a_deletion();
    unsafe {
        mq_emu_sys::imgui_addons::testing::mqemu_text_editor_test_undo_raw(e.0, 1);
        assert_eq!(editor_text(&e), "one two three");
        mq_emu_sys::imgui_addons::testing::mqemu_text_editor_test_redo_raw(e.0, 1);
    }
    assert_eq!(editor_text(&e), "one two three", "the redo deleted nothing");
}

#[test]
fn the_editors_own_undo_removes_nothing_after_a_redo() {
    // The same in reverse: Redo moves an insert's start.
    let _imgui = lock();
    let e = editor();
    let (t, tn) = s("editor");
    let render = |down: bool| unsafe {
        assert!(mqemu_imgui_test_new_frame(100.0, 100.0, down));
        begin("raw undo");
        mqemu_text_editor_render(e.0, t, tn, 0.0, 0.0, false);
        mqemu_imgui_end();
        let mut error = [0 as c_char; 512];
        assert!(mqemu_imgui_test_end_frame(error.as_mut_ptr(), error.len()), "{}", text(&error));
    };
    // A frame for ImGui to know the window, then a click to focus the editor.
    render(false);
    render(true);
    render(false);
    unsafe { mqemu_imgui_test_type(c"!".as_ptr()) };
    render(false);
    render(false);
    assert_eq!(editor_text(&e), "!", "typing is an undo step");
    use mq_emu_sys::imgui_addons::testing::*;
    unsafe {
        mqemu_text_editor_test_undo_raw(e.0, 1);
        assert_eq!(editor_text(&e), "");
        mqemu_text_editor_test_redo_raw(e.0, 1);
        assert_eq!(editor_text(&e), "!");
        mqemu_text_editor_test_undo_raw(e.0, 1);
    }
    assert_eq!(editor_text(&e), "!", "the undo removed nothing");
}

#[test]
fn undo_and_redo_keep_working_through_repeated_cycles() {
    let _imgui = lock();
    let e = editor_with_a_deletion();
    unsafe {
        for _ in 0..3 {
            mqemu_text_editor_undo(e.0, 1);
            assert_eq!(editor_text(&e), "one two three");
            mqemu_text_editor_redo(e.0, 1);
            assert_eq!(editor_text(&e), "one three");
        }
        // A second deletion, undone and redone with the first.
        mqemu_text_editor_set_selection(e.0, 0, 0, 0, 4, SelectionMode_Normal);
        mqemu_text_editor_delete_forward(e.0);
        assert_eq!(editor_text(&e), "three");
        for _ in 0..2 {
            mqemu_text_editor_undo(e.0, 2);
            assert_eq!(editor_text(&e), "one two three");
            mqemu_text_editor_redo(e.0, 2);
            assert_eq!(editor_text(&e), "three");
        }
    }
}

#[test]
fn undo_from_the_keyboard_inside_render_keeps_its_place_too() {
    let _imgui = lock();
    let e = editor_with_a_deletion();
    let (t, tn) = s("editor");
    let render = || {
        frame(|| unsafe {
            begin("undo keys");
            mqemu_text_editor_render(e.0, t, tn, 0.0, 0.0, false);
            mqemu_imgui_end();
        })
        .unwrap()
    };
    // Focus the editor (its child window) with a click once ImGui knows the
    // window, then ctrl+Z, ctrl+Y.
    render();
    unsafe {
        assert!(mqemu_imgui_test_new_frame(100.0, 100.0, true));
        begin("undo keys");
        mqemu_text_editor_render(e.0, t, tn, 0.0, 0.0, false);
        mqemu_imgui_end();
        let mut error = [0 as c_char; 512];
        assert!(mqemu_imgui_test_end_frame(error.as_mut_ptr(), error.len()), "{}", text(&error));
    }
    render();
    let press = |letter: u8| {
        unsafe {
            mqemu_imgui_test_modifiers(true, false);
            mqemu_imgui_test_letter_key(letter as c_char, true);
        }
        render();
        unsafe { mqemu_imgui_test_letter_key(letter as c_char, false) };
        render();
        unsafe { mqemu_imgui_test_modifiers(false, false) };
        render();
    };
    for _ in 0..2 {
        press(b'Z');
        assert_eq!(editor_text(&e), "one two three", "after ctrl+Z");
        press(b'Y');
        assert_eq!(editor_text(&e), "one three", "after ctrl+Y");
    }
    // Typed text is an undo step too, whose start the redo moves.
    unsafe {
        mqemu_text_editor_set_cursor_position(e.0, 0, 9);
        mqemu_imgui_test_type(c"!".as_ptr());
    }
    render();
    render();
    assert_eq!(editor_text(&e), "one three!");
    for _ in 0..2 {
        press(b'Z');
        assert_eq!(editor_text(&e), "one three", "after ctrl+Z");
        press(b'Y');
        assert_eq!(editor_text(&e), "one three!", "after ctrl+Y");
    }
}

#[test]
fn text_editor_renders_in_a_balanced_frame_with_markers_and_colored_text() {
    let _imgui = lock();
    let e = editor();
    unsafe {
        let (p, n) = s("local x = 1 -- a comment\nprint(x)\n");
        mqemu_text_editor_set_text(e.0, p, n);
        mqemu_text_editor_set_language(e.0, MQEMU_LANGUAGE_LUA);
        let messages = Strings::new(&["oops"]);
        mqemu_text_editor_set_error_markers(e.0, [1].as_ptr(), messages.raw());
        mqemu_text_editor_set_breakpoints(e.0, [0, 1].as_ptr(), 2);
        let (t, tn) = s("editor");
        for _ in 0..2 {
            frame(|| {
                begin("text editor");
                mqemu_text_editor_render(e.0, t, tn, 0.0, 0.0, true);
                mqemu_imgui_end();
            })
            .unwrap();
        }
        assert!(!mqemu_text_editor_is_text_changed(e.0), "rendering resets it");
        assert_eq!(take_error(), None);

        // Colored text keeps its color whatever the language.
        let (p, n) = s("red\nlines");
        mqemu_text_editor_set_text_colored(e.0, p, n, 0xff0000ff);
        mqemu_text_editor_insert_text_colored(e.0, p, n, 0xff00ff00);
        assert_eq!(editor_text(&e), "red\nlinesred\nlines");
        frame(|| {
            begin("text editor");
            mqemu_text_editor_render(e.0, t, tn, 300.0, 200.0, false);
            mqemu_imgui_end();
        })
        .unwrap();
    }
}

#[test]
fn a_custom_language_highlights_and_a_bad_regex_is_refused() {
    let _imgui = lock();
    let e = editor();
    let keywords = Strings::new(&["spawn", "target"]);
    let identifiers = Strings::new(&["Me"]);
    let declarations = Strings::new(&["the TLO for yourself"]);
    let none = Strings::new(&[]);
    let good = Strings::new(&["[0-9]+", "\\$\\{[^}]*\\}"]);
    let palette = [PaletteIndex_Number, PaletteIndex_Preprocessor];
    let (name, name_len) = s("macro");
    let (comment, comment_len) = s("|");
    let (empty, empty_len) = s("");
    let mut language = mqemu_text_editor_language {
        name,
        name_len,
        keywords: keywords.raw(),
        identifiers: identifiers.raw(),
        declarations: declarations.raw(),
        preproc_identifiers: none.raw(),
        preproc_declarations: none.raw(),
        comment_start: empty,
        comment_start_len: empty_len,
        comment_end: empty,
        comment_end_len: empty_len,
        single_line_comment: comment,
        single_line_comment_len: comment_len,
        preproc_char: b'#' as c_char,
        auto_indentation: true,
        token_regexes: good.raw(),
        token_palette: palette.as_ptr(),
        case_sensitive: false,
    };
    unsafe {
        let errors = collect(|sink, user| assert!(mqemu_text_editor_set_language_definition(e.0, &language, sink, user)));
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(one(|sink, user| mqemu_text_editor_language_name(e.0, sink, user)), "macro");

        let bad = Strings::new(&["[0-9]+", "(unclosed"]);
        language.token_regexes = bad.raw();
        let (other, other_len) = s("other");
        language.name = other;
        language.name_len = other_len;
        let errors = collect(|sink, user| assert!(!mqemu_text_editor_set_language_definition(e.0, &language, sink, user)));
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("token regex 1 ((unclosed): "), "{errors:?}");
        assert_eq!(one(|sink, user| mqemu_text_editor_language_name(e.0, sink, user)), "macro", "a refused language changes nothing");
        assert_eq!(take_error(), None, "a bad regex is refused, not thrown");

        let (p, n) = s("/target ${Me.Name} 42 | a comment");
        mqemu_text_editor_set_text(e.0, p, n);
        let (t, tn) = s("editor");
        frame(|| {
            begin("text editor");
            mqemu_text_editor_render(e.0, t, tn, 0.0, 0.0, false);
            mqemu_imgui_end();
        })
        .unwrap();
    }
}

fn default_options() -> mqemu_memory_editor_options {
    mqemu_memory_editor_options {
        read_only: false,
        cols: 16,
        show_options: true,
        show_data_preview: false,
        show_hexii: false,
        show_ascii: true,
        grey_out_zeroes: true,
        upper_case_hex: true,
        mid_cols_count: 8,
        addr_digits_count: 0,
        footer_extra_height: 0.0,
        highlight_color: 0x32ffffff,
    }
}

struct Hooked {
    reads: usize,
    highlights: usize,
    backgrounds: usize,
}

unsafe extern "C" fn read_hook(_: *const u8, offset: usize, user: *mut c_void) -> u8 {
    unsafe { (*user.cast::<Hooked>()).reads += 1 };
    offset as u8
}

unsafe extern "C" fn highlight_hook(_: *const u8, offset: usize, user: *mut c_void) -> bool {
    unsafe { (*user.cast::<Hooked>()).highlights += 1 };
    offset % 2 == 0
}

unsafe extern "C" fn background_hook(_: *const u8, _: usize, user: *mut c_void) -> u32 {
    unsafe { (*user.cast::<Hooked>()).backgrounds += 1 };
    0
}

#[test]
fn memory_editor_draws_contents_and_a_window_with_and_without_hooks() {
    let _imgui = lock();
    unsafe {
        let editor = mqemu_memory_editor_new();
        let mut data: Vec<u8> = (0..=255).collect();
        let mut options = default_options();
        options.show_data_preview = true;
        frame(|| {
            begin("memory");
            mqemu_memory_editor_draw(editor, &mut options, core::ptr::null(), NO_STRING, core::ptr::null_mut(), data.as_mut_ptr(), data.len(), 0x1000, core::ptr::null());
            mqemu_imgui_end();
        })
        .unwrap();
        assert_eq!(options, mqemu_memory_editor_options { show_data_preview: true, ..default_options() }, "nothing the user changed");

        // A source with no memory behind it: every byte comes from the hook.
        let mut hooked = Hooked { reads: 0, highlights: 0, backgrounds: 0 };
        let hooks = mqemu_memory_editor_hooks {
            read: Some(read_hook),
            write: None,
            highlight: Some(highlight_hook),
            background: Some(background_hook),
            user: (&raw mut hooked).cast(),
        };
        let mut open = true;
        let (t, tn) = s("memory window");
        options.read_only = true;
        frame(|| mqemu_memory_editor_draw(editor, &mut options, t, tn, &mut open, core::ptr::null_mut(), 64, 0, &hooks)).unwrap();
        assert!(open);
        assert!(hooked.reads >= 64, "read {} bytes", hooked.reads);
        assert!(hooked.highlights > 0);
        assert_eq!(take_error(), None);

        // Highlighting takes precedence over background colors.
        mqemu_memory_editor_goto(editor, 4, 8);
        let reads = hooked.reads;
        frame(|| mqemu_memory_editor_draw(editor, &mut options, t, tn, &mut open, core::ptr::null_mut(), 64, 0, &hooks)).unwrap();
        assert!(hooked.reads > reads);
        let mut offset = 99;
        assert!(!mqemu_memory_editor_hovered(editor, &mut offset), "the mouse is at the corner");

        // An empty buffer draws too.
        frame(|| {
            begin("memory");
            mqemu_memory_editor_draw(editor, &mut options, core::ptr::null(), NO_STRING, core::ptr::null_mut(), data.as_mut_ptr(), 0, 0, core::ptr::null());
            mqemu_imgui_end();
        })
        .unwrap();
        mqemu_memory_editor_delete(editor);
    }
}

struct Dialog(*mut mqemu_file_dialog);

impl Drop for Dialog {
    fn drop(&mut self) {
        unsafe { mqemu_file_dialog_delete(self.0) }
    }
}

struct Pane {
    calls: usize,
    filters: Vec<String>,
}

unsafe extern "C" fn pane(user: *mut c_void, filter: *const c_char, len: usize, can_continue: *mut bool) {
    let pane = unsafe { &mut *user.cast::<Pane>() };
    pane.calls += 1;
    pane.filters.push(String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(filter.cast(), len) }).into_owned());
    unsafe { *can_continue = false };
}

#[test]
fn file_dialog_opens_displays_with_its_pane_and_closes() {
    let _imgui = lock();
    let dialog = Dialog(unsafe { mqemu_file_dialog_new() });
    assert!(!dialog.0.is_null());
    let dir = std::env::temp_dir();
    let dir = dir.to_str().unwrap();
    let (key, key_len) = s("ChooseFile");
    let (title, title_len) = s("Choose a file");
    let (filters, filters_len) = s(".txt,.ini");
    let (path, path_len) = s(dir);
    let (name, name_len) = s("settings.ini");
    unsafe {
        assert!(!mqemu_file_dialog_is_opened(dialog.0, core::ptr::null(), NO_STRING));
        mqemu_file_dialog_open(dialog.0, key, key_len, title, title_len, filters, filters_len, path, path_len, name, name_len, 1, 0, 150.0);
        assert!(mqemu_file_dialog_is_opened(dialog.0, core::ptr::null(), NO_STRING));
        assert!(mqemu_file_dialog_is_opened(dialog.0, key, key_len));
        let (other, other_len) = s("Other");
        assert!(!mqemu_file_dialog_is_opened(dialog.0, other, other_len));

        let mut state = Pane { calls: 0, filters: Vec::new() };
        for _ in 0..2 {
            frame(|| {
                let closed = mqemu_file_dialog_display(dialog.0, key, key_len, 0, 300.0, 200.0, f32::MAX, f32::MAX, Some(pane), (&raw mut state).cast());
                assert!(!closed);
                // Another key isn't shown.
                assert!(!mqemu_file_dialog_display(dialog.0, other, other_len, 0, 0.0, 0.0, f32::MAX, f32::MAX, None, core::ptr::null_mut()));
            })
            .unwrap();
        }
        assert!(state.calls > 0, "the pane wasn't drawn");
        assert_eq!(state.filters.last().map(String::as_str), Some(".txt"));
        // Without a pane function the pane is left empty.
        frame(|| {
            mqemu_file_dialog_display(dialog.0, key, key_len, 0, 300.0, 200.0, f32::MAX, f32::MAX, None, core::ptr::null_mut());
        })
        .unwrap();
        assert_eq!(take_error(), None);

        let current = one(|sink, user| mqemu_file_dialog_current_path(dialog.0, sink, user));
        assert_eq!(current.trim_end_matches('\\').to_lowercase(), dir.trim_end_matches('\\').to_lowercase());
        assert_eq!(one(|sink, user| mqemu_file_dialog_current_filter(dialog.0, sink, user)), ".txt");
        assert_eq!(one(|sink, user| mqemu_file_dialog_current_file_name(dialog.0, IGFD_ResultMode_KeepInputFile, sink, user)), "settings.ini");
        let full = one(|sink, user| mqemu_file_dialog_file_path_name(dialog.0, IGFD_ResultMode_KeepInputFile, sink, user));
        assert!(full.to_lowercase().ends_with("\\settings.ini"), "{full}");

        mqemu_file_dialog_close(dialog.0);
        assert!(!mqemu_file_dialog_is_opened(dialog.0, core::ptr::null(), NO_STRING));
        assert!(!mqemu_file_dialog_is_ok(dialog.0));
    }
}

unsafe extern "C" fn push_pair(user: *mut c_void, name: *const c_char, name_len: usize, path: *const c_char, path_len: usize) {
    let out = unsafe { &mut *user.cast::<Vec<(String, String)>>() };
    let text = |p: *const c_char, n: usize| String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(p.cast::<u8>(), n) }).into_owned();
    out.push((text(name, name_len), text(path, path_len)));
}

#[test]
fn file_dialog_selection_and_file_styles() {
    let _imgui = lock();
    let dialog = Dialog(unsafe { mqemu_file_dialog_new() });
    unsafe {
        let mut selection = Vec::<(String, String)>::new();
        mqemu_file_dialog_selection(dialog.0, IGFD_ResultMode_KeepInputFile, push_pair, (&raw mut selection).cast());
        assert!(selection.is_empty(), "nothing chosen: {selection:?}");
        assert_eq!(one(|sink, user| mqemu_file_dialog_current_path(dialog.0, sink, user)), "");

        let (criteria, criteria_len) = s(".lua");
        let (icon, icon_len) = s("[L]");
        mqemu_file_dialog_set_file_style(dialog.0, IGFD_FileStyleByExtention, criteria, criteria_len, [1.0, 0.5, 0.0, 1.0].as_ptr(), icon, icon_len, core::ptr::null_mut());
        let mut color = [0.0f32; 4];
        let mut font = core::ptr::dangling_mut::<ImFont>();
        let mut icons = Vec::<String>::new();
        assert!(mqemu_file_dialog_file_style(dialog.0, IGFD_FileStyleByExtention, criteria, criteria_len, color.as_mut_ptr(), push, (&raw mut icons).cast(), &mut font));
        assert_eq!(color, [1.0, 0.5, 0.0, 1.0]);
        assert_eq!(icons, ["[L]"]);
        assert!(font.is_null());

        mqemu_file_dialog_clear_file_styles(dialog.0);
        assert!(!mqemu_file_dialog_file_style(dialog.0, IGFD_FileStyleByExtention, criteria, criteria_len, color.as_mut_ptr(), push, (&raw mut icons).cast(), &mut font));
        assert_eq!(icons.len(), 1, "no icon sent for a missing style");
    }
}
