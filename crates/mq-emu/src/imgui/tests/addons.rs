//! MacroQuest's ImGui add-ons through the safe API, against the real
//! imgui.dll: Rust values round-trip, closures are called during the draw
//! only, and their panics come back out after it.

use std::cell::Cell;

use super::*;

#[test]
fn text_editor_round_trips_text_settings_and_positions() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let mut editor = ui.new_text_editor();
    assert_eq!(editor.language_name(), "", "a new editor has no language");
    assert_eq!(editor.palette(), editor.builtin_palette(PaletteKind::Dark));

    editor.set_lines(&["first", "\tsecond"]);
    assert_eq!(editor.text(), "first\n\tsecond");
    assert_eq!(editor.lines(), ["first", "\tsecond"]);
    assert_eq!(editor.line_count(), 2);
    editor.set_cursor_position(TextPosition::new(1, 5));
    assert_eq!(editor.cursor_position(), TextPosition::new(1, 5));
    assert_eq!(editor.current_line_text(), "\tsecond");
    editor.set_selection(TextPosition::new(0, 2), TextPosition::new(0, 0), SelectionMode::Normal);
    assert_eq!(editor.selection(), (TextPosition::new(0, 0), TextPosition::new(0, 2)), "the start comes first");
    assert_eq!(editor.selected_text(), "fi");
    editor.set_selection(TextPosition::new(1, 6), TextPosition::new(1, 6), SelectionMode::Line);
    assert_eq!(editor.selected_text(), "\tsecond");

    editor.set_tab_size(99);
    assert_eq!(editor.tab_size(), 32, "clamped");
    editor.set_tab_size(0);
    assert_eq!(editor.tab_size(), 1);
    editor.set_handle_mouse(false);
    editor.set_handle_keyboard(true);
    assert!(!editor.handles_mouse(), "the editor's own getter would report the keyboard's");
    assert!(editor.handles_keyboard());
    editor.set_read_only(true);
    editor.set_ignore_child(true);
    editor.set_show_whitespace(false);
    editor.set_show_short_tab_glyphs(true);
    editor.set_colorizer_enabled(false);
    assert!(editor.is_read_only() && editor.ignores_child() && !editor.shows_whitespace() && editor.shows_short_tab_glyphs() && !editor.is_colorizer_enabled());

    let mut palette = editor.builtin_palette(PaletteKind::RetroBlue);
    palette[PaletteIndex::Keyword] = [1.0, 0.0, 0.5, 1.0];
    editor.set_palette(&palette);
    assert_eq!(editor.palette().0.map(pack_color), palette.0.map(pack_color));
    assert_eq!(editor.palette()[PaletteIndex::Keyword], [1.0, 0.0, 128.0 / 255.0, 1.0], "colors are bytes");
    assert_eq!(PaletteIndex::ALL.iter().map(|i| *i as usize).collect::<Vec<_>>(), (0..21).collect::<Vec<_>>());
}

#[test]
fn text_editor_languages_of_your_own_and_bad_regexes() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let mut editor = ui.new_text_editor();
    editor.set_language(Language::Lua);
    assert_eq!(editor.language_name(), "Lua");
    let mut macro_language = LanguageDefinition {
        name: "MQ macro".into(),
        keywords: vec!["/if".into(), "/goto".into()],
        identifiers: vec![("Me".into(), "Your character".into())],
        single_line_comment: "|".into(),
        token_regexes: vec![("[0-9]+".into(), PaletteIndex::Number), ("\\$\\{[^}]*\\}".into(), PaletteIndex::Preprocessor)],
        case_sensitive: false,
        ..Default::default()
    };
    assert_eq!(editor.set_language_definition(&macro_language), Ok(()));
    assert_eq!(editor.language_name(), "MQ macro");
    macro_language.name = "broken".into();
    macro_language.token_regexes.push(("[unclosed".into(), PaletteIndex::String));
    let error = editor.set_language_definition(&macro_language).unwrap_err();
    assert!(error.starts_with("token regex 2 ([unclosed): "), "{error}");
    assert_eq!(editor.language_name(), "MQ macro");
    assert_eq!(take_error(), None);

    editor.set_text("/if (${Me.Level} > 10) /goto :done | done\n/echo 1");
    editor.set_error_markers([(1, "unknown command")]);
    editor.set_breakpoints([0]);
    let rendered = frame(|ui| test_window(ui, |ui| editor.render(ui, "macro", [0.0, 200.0], true)));
    assert_eq!(rendered, Ok(Some(())));
    assert!(!editor.is_text_changed(), "rendering resets it");
    editor.insert_text_colored("!", [1.0, 0.0, 0.0, 1.0]);
    assert!(editor.is_text_changed());
}

#[test]
fn text_editor_undo_and_redo_from_rust_keep_their_place() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let mut editor = ui.new_text_editor();
    editor.set_text("alpha beta gamma");
    editor.set_selection(TextPosition::new(0, 6), TextPosition::new(0, 11), SelectionMode::Normal);
    editor.delete();
    assert_eq!(editor.text(), "alpha gamma");
    for _ in 0..3 {
        editor.undo(1);
        assert_eq!(editor.text(), "alpha beta gamma");
        editor.redo(1);
        assert_eq!(editor.text(), "alpha gamma");
    }
    editor.set_read_only(true);
    assert!(!editor.can_undo(), "read only stops undo");
}

#[test]
fn memory_editor_options_come_back_and_read_only_data_stays_read_only() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let mut memory = ui.new_memory_editor();
    assert_eq!(memory.options, MemoryEditorOptions::default());
    memory.options.columns = 0;
    let mut bytes = [0u8; 64];
    let shown = [1u8, 2, 3];
    let result = frame(|ui| {
        test_window(ui, |ui| {
            memory.draw(ui, &mut bytes, 0x4000);
            ui.child("read only").size([0.0, 200.0]).show(|ui| memory.draw(ui, &shown, 0));
        })
    });
    assert_eq!(result, Ok(Some(())));
    assert_eq!(memory.options.columns, 1, "at least one column");
    assert!(!memory.options.read_only, "drawing read-only data doesn't change the setting");
    assert_eq!(memory.hovered(), None);
    memory.goto(8..16);
    let still_open = frame(|ui| memory.window(ui, "bytes", &mut bytes, 0)).unwrap();
    assert!(still_open);
}

#[test]
fn memory_editor_sources_and_hooks_are_called_while_it_draws() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let mut memory = ui.new_memory_editor();
    let (reads, highlights, backgrounds) = (Cell::new(0), Cell::new(0), Cell::new(0));
    let mut read = |offset: usize| {
        reads.set(reads.get() + 1);
        offset as u8
    };
    let mut highlight = |offset: usize| {
        highlights.set(highlights.get() + 1);
        offset < 4
    };
    let mut background = |_: usize| {
        backgrounds.set(backgrounds.get() + 1);
        Some([0.0, 0.0, 1.0, 0.5])
    };
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let data = MemoryData::Source { size: 48, read: &mut read, write: None };
            memory.draw_with(ui, data, 0, MemoryEditorHooks { highlight: Some(&mut highlight), background: Some(&mut background) });
        })
    });
    assert_eq!(result, Ok(Some(())));
    assert!(reads.get() >= 48, "every byte, once for hex and once for ASCII: {}", reads.get());
    assert!(highlights.get() > 0 && backgrounds.get() > 0);
    let after = (reads.get(), highlights.get());
    frame(|ui| test_window(ui, |ui| memory.draw(ui, &[0u8; 4], 0))).unwrap();
    assert_eq!((reads.get(), highlights.get()), after, "the closures were for that draw only");
}

#[test]
fn a_panic_in_a_memory_editor_hook_comes_out_after_the_draw() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let mut memory = ui.new_memory_editor();
    let calls = Cell::new(0);
    let result = frame(|ui| {
        test_window(ui, |ui| {
            catch_unwind(AssertUnwindSafe(|| {
                let mut read = |_: usize| {
                    calls.set(calls.get() + 1);
                    panic!("bad byte")
                };
                memory.draw(ui, MemoryData::Source { size: 32, read: &mut read, write: None }, 0);
            }))
            .map_err(|p| p.downcast_ref::<&str>().copied())
        })
    });
    assert_eq!(result, Ok(Some(Err(Some("bad byte")))), "the frame ended cleanly, then the panic came out");
    assert_eq!(calls.get(), 1, "not called again after it panicked");
}

#[test]
fn file_dialog_shows_its_pane_through_the_frames_ui() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let mut dialog = ui.new_file_dialog();
    assert!(!dialog.is_open(None));
    let dir = std::env::temp_dir();
    let open = FileDialogOpen { title: "Pick", filters: Some("Settings{.ini},All{.*}"), path: dir.to_str().unwrap(), pane_width: Some(120.0), ..Default::default() };
    dialog.open("pick", open);
    assert!(dialog.is_open(Some("pick")) && dialog.is_open(None) && !dialog.is_open(Some("other")));
    let filters = std::cell::RefCell::new(Vec::new());
    for _ in 0..2 {
        let closed = frame(|ui| {
            let mut pane = |ui: &mut Ui, filter: &str| {
                filters.borrow_mut().push(filter.to_owned());
                ui.text("in the pane");
                ui.button("pane button");
                false
            };
            dialog.display_with(ui, "pick", WindowFlags::NONE, [400.0, 300.0], [f32::MAX; 2], Some(&mut pane))
        });
        assert_eq!(closed, Ok(false));
    }
    assert_eq!(filters.borrow().last().map(String::as_str), Some(".ini"), "the chosen group's extensions");
    assert!(dialog.was_displayed_this_frame(Some("pick")) && dialog.was_displayed_this_frame(None));
    assert!(!dialog.was_displayed_this_frame(Some("other")));
    assert_eq!(dialog.current_filter(), "Settings");
    assert_eq!(dialog.selection(ResultMode::KeepInputFile), []);
    assert_eq!(take_error(), None);
    dialog.close();
    assert!(!dialog.is_open(None));
    assert!(!dialog.is_ok());
}

#[test]
fn a_panic_in_a_file_dialog_pane_comes_out_after_the_display() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let mut dialog = ui.new_file_dialog();
    dialog.open("pick", FileDialogOpen { pane_width: Some(100.0), ..Default::default() });
    let result = frame(|ui| {
        catch_unwind(AssertUnwindSafe(|| {
            let mut pane = |_: &mut Ui, _: &str| -> bool { panic!("pane") };
            dialog.display_with(ui, "pick", WindowFlags::NONE, [300.0, 200.0], [f32::MAX; 2], Some(&mut pane))
        }))
        .is_err()
    });
    assert_eq!(result, Ok(true), "the dialog's window still ended");
}

#[test]
fn file_styles_round_trip_with_their_fonts() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let mut dialog = ui.new_file_dialog();
    assert_eq!(dialog.file_style(FileStyleFlags::BY_EXTENSION, ".lua"), None);
    let windows = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_owned());
    let arial = frame(|ui| ui.load_font(&format!(r"{windows}\Fonts\arial.ttf"), 18.0)).unwrap().expect("Windows has Arial");
    let style = FileStyle { color: [0.0, 1.0, 0.0, 1.0], icon: "[lua]".into(), font: Some(Font::Loaded(arial)) };
    dialog.set_file_style(FileStyleFlags::BY_EXTENSION, ".lua", &style);
    let plain = FileStyle { color: [1.0; 4], icon: String::new(), font: None };
    dialog.set_file_style(FileStyleFlags::BY_TYPE_DIR, "", &plain);
    assert_eq!(dialog.file_style(FileStyleFlags::BY_EXTENSION, ".lua"), Some(style));
    assert_eq!(dialog.file_style(FileStyleFlags::BY_TYPE_DIR, ""), Some(plain));
    dialog.clear_file_styles();
    assert_eq!(dialog.file_style(FileStyleFlags::BY_EXTENSION, ".lua"), None);
}
