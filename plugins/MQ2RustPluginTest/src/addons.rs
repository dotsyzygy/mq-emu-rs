//! MacroQuest's other EQ UI drawing (UI texture files, pieces of them,
//! screen pieces) and its ImGui add-ons (the text editor, memory editor and
//! file dialog): the debug window's add-ons tab, and the selftest's add-ons
//! window, drawn during the graphics exercise.

use std::cell::Cell;

use mq_emu::imgui::{
    Cond, FileDialog, FileDialogOpen, ITEM_ICONS, Language, MemoryData, MemoryEditor, MemoryEditorHooks, ResultMode, ScreenPiece,
    ScreenPieceKind, TextEditor, TextureAnimation, Ui, UiImageOptions, UiTexture, WindowFlags,
};

/// A UI texture file every RoF2 UI has, and a screen piece of its inventory window.
pub const UI_TEXTURE: &str = "AttackIndicator.tga";
pub const SCREEN_PIECE: &str = "Inv_Seperator1";
const EXERCISE_WINDOW: &str = "Rust plugin test: add-ons selftest";
const EDITOR_TEXT: &str = "-- mq-emu text editor\nlocal x = 42\nprint(x)\n";
const DIALOG_KEY: &str = "rusttest exercise dialog";
const YELLOW: [f32; 4] = [0.95, 0.85, 0.3, 1.0];

/// The add-ons' state, kept across frames (each made on first use).
#[derive(Default)]
pub struct Addons {
    ui_texture: Option<UiTexture>,
    screen_piece: Option<ScreenPiece>,
    item_icons: Option<TextureAnimation>,
    editor: Option<TextEditor>,
    memory: Option<MemoryEditor>,
    dialog: Option<FileDialog>,
    /// Bytes for the memory editor to show and edit.
    bytes: Vec<u8>,
    /// The tab's file dialog, and what it last chose.
    tab_dialog: Option<FileDialog>,
    chosen: Option<String>,
    show_memory_window: bool,
}

/// What the add-ons window found while the selftest ran.
#[derive(Debug, Default, Clone)]
pub struct AddonsExercise {
    /// The UI texture's size, if the game loaded it.
    pub ui_texture_size: Option<[f32; 2]>,
    /// Whether it drew as an item and on a draw list, and a piece of it.
    pub ui_texture_drawn: bool,
    /// Whether the item icons' frame drew as a texture piece.
    pub frame_drawn: bool,
    /// The screen piece's kind, and whether it drew.
    pub screen_piece: Option<(ScreenPieceKind, bool)>,
    /// What the text editor got wrong (empty if nothing).
    pub editor_wrong: String,
    pub editor_rendered: bool,
    /// Bytes the memory editor read through a closure.
    pub memory_reads: usize,
    /// Whether the file dialog drew its pane.
    pub dialog_pane: bool,
}

impl Addons {
    /// The game's resources, found again after the UI reloads.
    fn find(&mut self, ui: &Ui) {
        if !self.ui_texture.as_ref().is_some_and(UiTexture::is_current) {
            self.ui_texture = ui.find_ui_texture(UI_TEXTURE);
        }
        if !self.screen_piece.as_ref().is_some_and(ScreenPiece::is_current) {
            self.screen_piece = ui.find_screen_piece(SCREEN_PIECE);
        }
        if !self.item_icons.as_ref().is_some_and(TextureAnimation::is_current) {
            self.item_icons = ui.find_texture_animation(ITEM_ICONS);
        }
    }

    /// One frame of the selftest's add-ons window: every add-on, small.
    pub fn exercise_frame(&mut self, ui: &mut Ui, in_game: bool, found: &mut AddonsExercise) {
        if in_game {
            self.find(ui);
        }
        let editor = self.editor.get_or_insert_with(|| {
            let mut editor = ui.new_text_editor();
            editor.set_language(Language::Lua);
            editor.set_text(EDITOR_TEXT);
            editor
        });
        let memory = self.memory.get_or_insert_with(|| ui.new_memory_editor());
        let dialog = self.dialog.get_or_insert_with(|| {
            let mut dialog = ui.new_file_dialog();
            dialog.open(DIALOG_KEY, FileDialogOpen { title: "Rust plugin test: file dialog", pane_width: Some(100.0), ..Default::default() });
            dialog
        });
        let (ui_texture, screen_piece, item_icons) = (&self.ui_texture, &self.screen_piece, &self.item_icons);
        let reads = Cell::new(0);
        ui.window(EXERCISE_WINDOW)
            .position([580.0, 40.0], Cond::Appearing)
            .size([420.0, 520.0], Cond::Appearing)
            .flags(WindowFlags::NO_SAVED_SETTINGS | WindowFlags::NO_FOCUS_ON_APPEARING)
            .show(|ui| {
                // The game's drawing first, so it's in view.
                if let Some(texture) = ui_texture {
                    found.ui_texture_size = Some(texture.size());
                    let item = ui.ui_texture(texture, None, UiImageOptions { size: Some([32.0, 32.0]), border: Some(YELLOW), ..Default::default() });
                    ui.same_line();
                    let [w, h] = texture.size();
                    let part = ui.ui_texture(texture, Some([[0.0, 0.0], [w / 2.0, h / 2.0]]), UiImageOptions::default());
                    let piece = texture.piece([0.0, 0.0], [w / 2.0, h]).is_some_and(|p| {
                        ui.same_line();
                        ui.texture_piece(&p, UiImageOptions::default())
                    });
                    let [x, y] = ui.cursor_screen_pos();
                    let listed = ui.draw_list().add_ui_texture(texture, [x, y], None, UiImageOptions { size: Some([24.0, 24.0]), ..Default::default() });
                    ui.dummy([24.0, 24.0]);
                    found.ui_texture_drawn |= item && part && piece && listed;
                }
                if let Some(frame) = item_icons.as_ref().and_then(|icons| icons.frame(0)) {
                    found.frame_drawn |= ui.texture_piece(&frame, UiImageOptions { size: Some([40.0, 40.0]), ..Default::default() });
                }
                if let Some(piece) = screen_piece {
                    let [x, y] = ui.cursor_screen_pos();
                    let drawn = ui.draw_list().add_screen_piece(piece, [x, y], [x + 200.0, y + 300.0]);
                    ui.dummy([200.0, 8.0]);
                    let kind = piece.kind();
                    found.screen_piece = Some((kind, found.screen_piece.is_some_and(|(_, d)| d) || drawn));
                }
                editor.render(ui, "exercise editor", [-1.0, 120.0], true);
                found.editor_rendered = true;
                let mut read = |offset: usize| {
                    reads.set(reads.get() + 1);
                    offset as u8
                };
                ui.child("exercise memory").size([-1.0, 140.0]).show(|ui| {
                    memory.draw_with(ui, MemoryData::Source { size: 64, read: &mut read, write: None }, 0x1000, MemoryEditorHooks::default());
                });
            });
        let mut pane_drawn = false;
        let mut pane = |ui: &mut Ui, filter: &str| {
            ui.text(format!("Filter: {filter}"));
            pane_drawn = true;
            true
        };
        dialog.display_with(ui, DIALOG_KEY, WindowFlags::NO_SAVED_SETTINGS, [360.0, 240.0], [f32::MAX; 2], Some(&mut pane));
        found.dialog_pane |= pane_drawn;
        found.memory_reads = found.memory_reads.max(reads.get());
        let mut wrong = Vec::new();
        if editor.text() != EDITOR_TEXT {
            wrong.push(format!("its text came back as {:?}", editor.text()));
        }
        if editor.language_name() != "Lua" {
            wrong.push(format!("its language is {:?}", editor.language_name()));
        }
        found.editor_wrong = wrong.join("; ");
    }

    /// The selftest is done with its objects: close the dialog.
    pub fn exercise_done(&mut self) {
        if let Some(dialog) = self.dialog.as_mut() {
            dialog.close();
        }
        self.dialog = None;
    }

    /// The debug window's tab: each add-on to try by hand.
    pub fn tab(&mut self, ui: &mut Ui) {
        self.find(ui);
        ui.separator_text("The game's UI texture files and screen pieces");
        match &self.ui_texture {
            Some(texture) => {
                ui.text(format!("{} ({:.0}x{:.0}):", texture.name(), texture.size()[0], texture.size()[1]));
                ui.ui_texture(texture, None, UiImageOptions { border: Some(YELLOW), ..Default::default() });
            }
            None => ui.text_disabled(format!("{UI_TEXTURE} isn't loaded (no UI?)")),
        }
        if let Some(frame) = self.item_icons.as_ref().and_then(|icons| icons.frame(0)) {
            let [[x0, y0], [x1, y1]] = frame.rect();
            ui.text(format!("{ITEM_ICONS} frame 0 is ({x0:.0}, {y0:.0})-({x1:.0}, {y1:.0}) of {}:", frame.texture().map_or_else(String::new, |t| t.name())));
            ui.texture_piece(&frame, UiImageOptions { size: Some([128.0, 128.0]), ..Default::default() });
        }
        match &self.screen_piece {
            Some(piece) => {
                ui.text(format!("{SCREEN_PIECE}: {:?}, drawn by MacroQuest: {}", piece.kind(), piece.draws()));
                let [x, y] = ui.cursor_screen_pos();
                ui.draw_list().add_screen_piece(piece, [x, y], [x + 300.0, y + 300.0]);
                ui.dummy([300.0, 12.0]);
            }
            None => ui.text_disabled(format!("No screen piece {SCREEN_PIECE}")),
        }

        ui.separator_text("Text editor");
        let editor = self.editor.get_or_insert_with(|| {
            let mut editor = ui.new_text_editor();
            editor.set_language(Language::Lua);
            editor.set_text(EDITOR_TEXT);
            editor
        });
        let position = editor.cursor_position();
        ui.text(format!("{} lines, cursor at {}:{}, {}", editor.line_count(), position.line + 1, position.column + 1, editor.language_name()));
        editor.render(ui, "tab editor", [-1.0, 160.0], true);

        ui.separator_text("Memory editor");
        if self.bytes.is_empty() {
            self.bytes = (0..=255).collect();
        }
        let memory = self.memory.get_or_insert_with(|| ui.new_memory_editor());
        ui.checkbox("In a window of its own", &mut self.show_memory_window);
        if self.show_memory_window {
            self.show_memory_window = memory.window(ui, "Rust plugin test: memory editor", &mut self.bytes[..], 0);
        } else {
            ui.child("tab memory").size([-1.0, 200.0]).show(|ui| memory.draw(ui, &mut self.bytes[..], 0));
        }

        ui.separator_text("File dialog");
        let dialog = self.tab_dialog.get_or_insert_with(|| ui.new_file_dialog());
        if ui.button("Choose a file...") {
            dialog.open("tab", FileDialogOpen { title: "Choose a file", filters: Some("Lua{.lua},Settings{.ini},All{.*}"), ..Default::default() });
        }
        if dialog.display(ui, "tab") {
            self.chosen = Some(if dialog.is_ok() { dialog.file_path_name(ResultMode::KeepInputFile) } else { "(cancelled)".into() });
            dialog.close();
        }
        if let Some(chosen) = &self.chosen {
            ui.same_line();
            ui.text(chosen);
        }
    }
}
