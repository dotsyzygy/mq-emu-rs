//! ImGui beyond widgets: ImPlot, draw lists, textures and the game's icons,
//! drag and drop, fonts, docking and the dock builder, and MacroQuest's
//! EQ-styled widgets. The debug window's graphics tabs, and the selftest's
//! exercise window, which draws all of it for a few frames.

use std::cell::Cell;
use std::collections::VecDeque;
use std::time::Instant;

use mq_emu::imgui::{
    Axis, AxisFlags, BarsFlags, Bins, Colormap, Cond, Corners, Dir, DockNodeFlags, DragDropFlags, DragToolFlags, EqTextFlags, Font,
    ButtonFlags, HeatmapFlags, HistogramFlags, ITEM_ICONS, ImageOptions, InfLinesFlags, LegendFlags, LineFlags, Location, Marker, MultiSelectFlags,
    PieChartFlags, PlotFlags, SPELL_ICONS, ScatterFlags, Selection, SpellGemFlags, Split, StairsFlags, SubplotFlags, Texture, TextureAnimation, Ui, WindowFlags,
};
use mq_emu::{GameState, Mq, SpawnType};

use crate::RustTest;

const YELLOW: [f32; 4] = [0.95, 0.85, 0.3, 1.0];
const GREY: [f32; 4] = [0.6, 0.6, 0.6, 1.0];
const HP_RED: [f32; 4] = [0.75, 0.15, 0.15, 1.0];
const MANA_BLUE: [f32; 4] = [0.2, 0.4, 0.85, 1.0];

/// What the drag and drop sample carries.
const ICON_PAYLOAD: &str = "RUSTTEST_ITEM";
/// How many seconds of HP and mana the plots tab keeps.
const HISTORY_SECONDS: f64 = 60.0;
/// The test image's side, in pixels.
const TEST_IMAGE_SIZE: usize = 16;
/// The dock demo's windows.
const DOCKED: &str = "Rust docked window";
const DOCKED_2: &str = "Rust docked window 2";
/// The window the exercise docks with the dock builder.
const EXERCISE_DOCKED: &str = "Rust plugin test: docked by the dock builder";
/// Lines in the exercise's clipped list.
pub const CLIPPED_LINES: usize = 100_000;
/// Spell gems the game's spell gem window has.
const SPELL_GEMS: usize = mq_emu::sys::NUM_SPELL_GEMS;

/// The graphics tabs' state, kept across frames.
pub struct Graphics {
    started: Instant,
    /// The test image, once loaded (`None` after a failed try).
    texture: Option<Option<Texture>>,
    item_icons: Option<TextureAnimation>,
    spell_icons: Option<TextureAnimation>,
    /// (seconds, HP %, mana %), oldest first.
    history: VecDeque<(f64, f32, f32)>,
    drag_x: f64,
    marker: [f64; 2],
    font_size: f32,
    crosshair: bool,
    hud: bool,
    dock_demo: bool,
    /// Lay the dock demo out with the dock builder next frame.
    rebuild_dock: bool,
    /// The plots tab's plots with ImPlot's other mouse buttons.
    reverse_mouse: bool,
    /// Let the images tab's spell gems cast and forget spells.
    gems_act: bool,
    /// What was last dropped on the drop box, or the item link last clicked.
    dropped: Option<String>,
    /// The exercise's multi-select list.
    exercise_selection: Selection,
    /// The selftest's exercise, while it runs.
    pub exercise: Option<Exercise>,
}

impl Default for Graphics {
    fn default() -> Self {
        Graphics {
            started: Instant::now(),
            texture: None,
            item_icons: None,
            spell_icons: None,
            history: VecDeque::new(),
            drag_x: 5.0,
            marker: [0.0, 0.0],
            font_size: 20.0,
            crosshair: false,
            hud: false,
            dock_demo: false,
            rebuild_dock: false,
            reverse_mouse: false,
            gems_act: false,
            dropped: None,
            exercise_selection: Selection::new(),
            exercise: None,
        }
    }
}

/// The selftest's exercise: frames to draw, and what they found.
#[derive(Debug, Default, Clone)]
pub struct Exercise {
    pub frames_left: u32,
    pub frames_drawn: u32,
    /// The test image's size, if it loaded.
    pub texture_size: Option<[f32; 2]>,
    /// Cell sizes of the item and spell icon sheets, if found.
    pub item_icon_cell: Option<[f32; 2]>,
    pub spell_icon_cell: Option<[f32; 2]>,
    /// Whether MacroQuest's three fonts are there.
    pub mq_fonts: bool,
    /// Which EQ font styles are there.
    pub eq_fonts: Vec<u8>,
    pub docking: bool,
    /// Whether the plot drew (it's skipped when the window is collapsed).
    pub plotted: bool,
    /// Whether the dock builder's window ended up docked where it said (None
    /// with docking off).
    pub docked: Option<bool>,
    /// Points ImPlot asked an item's closure for, and whether it called the
    /// axis formatter and custom scale.
    pub getter_points: usize,
    pub formatted: bool,
    pub scaled: bool,
    /// Spell gems that drew (the game has a window for each).
    pub spell_gems: usize,
    /// The most lines of the clipped list drawn in a frame.
    pub clipped_lines: usize,
    /// Whether the custom item was laid out in view and drawn.
    pub custom_item: bool,
}

impl Exercise {
    pub fn new(frames: u32) -> Exercise {
        Exercise { frames_left: frames, ..Exercise::default() }
    }
}

/// A small 24-bit BMP: a gradient with a border, to load as a texture.
fn test_image() -> Vec<u8> {
    let side = TEST_IMAGE_SIZE;
    let row = side * 3; // already a multiple of 4
    let pixels = row * side;
    let mut bmp = Vec::with_capacity(54 + pixels);
    let u32le = |v: usize| (v as u32).to_le_bytes();
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&u32le(54 + pixels));
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&u32le(54));
    bmp.extend_from_slice(&u32le(40));
    bmp.extend_from_slice(&u32le(side));
    bmp.extend_from_slice(&u32le(side));
    bmp.extend_from_slice(&[1, 0, 24, 0]);
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&u32le(pixels));
    bmp.extend_from_slice(&[0; 16]);
    for y in 0..side {
        for x in 0..side {
            let edge = x == 0 || y == 0 || x == side - 1 || y == side - 1;
            let (r, g, b) = if edge { (255, 255, 255) } else { ((x * 16) as u8, (y * 16) as u8, 160) };
            bmp.extend_from_slice(&[b, g, r]);
        }
    }
    bmp
}

impl Graphics {
    /// The test image, written to the temp directory and loaded once.
    fn texture(&mut self, ui: &Ui) -> Option<&Texture> {
        self.texture
            .get_or_insert_with(|| {
                let path = std::env::temp_dir().join("MQ2RustPluginTest.bmp");
                std::fs::write(&path, test_image()).ok()?;
                ui.load_texture(&path.to_string_lossy())
            })
            .as_ref()
    }

    /// The game's icon sheets, found again after the UI reloads.
    fn icons(&mut self, ui: &Ui) {
        if !self.item_icons.as_ref().is_some_and(TextureAnimation::is_current) {
            self.item_icons = ui.find_texture_animation(ITEM_ICONS);
        }
        if !self.spell_icons.as_ref().is_some_and(TextureAnimation::is_current) {
            self.spell_icons = ui.find_texture_animation(SPELL_ICONS);
        }
    }
}

impl RustTest {
    /// Draws the overlays the drawing tab turned on, and the selftest's
    /// exercise window; every frame, open or not.
    pub(crate) fn draw_graphics_overlays(&mut self, mq: &Mq<Self>, ui: &mut Ui) {
        let g = &mut self.graphics;
        if g.crosshair {
            let [x, y] = ui.mouse_pos();
            let draw = ui.foreground_draw_list();
            draw.add_line([x - 12.0, y], [x + 12.0, y], YELLOW, 1.5);
            draw.add_line([x, y - 12.0], [x, y + 12.0], YELLOW, 1.5);
            draw.add_circle([x, y], 8.0, YELLOW, 1.0);
        }
        if g.hud {
            let ([sx, sy], [sw, _]) = ui.screen_rect();
            let text = format!("mq-emu HUD: {} pulses", mq.stats().pulses);
            let draw = ui.background_draw_list();
            let size = ui.calc_text_size(&text);
            let pos = [sx + (sw - size[0]) / 2.0, sy + 40.0];
            draw.add_rect_filled([pos[0] - 6.0, pos[1] - 3.0], [pos[0] + size[0] + 6.0, pos[1] + size[1] + 3.0], [0.0, 0.0, 0.0, 0.6], 4.0);
            draw.add_text(pos, YELLOW, text);
        }
        if g.dock_demo {
            let id = ui.dock_id("rusttest dock space");
            if std::mem::take(&mut g.rebuild_dock) {
                // Before the dock space and its windows draw this frame.
                ui.dock_builder(|dock| {
                    let root = dock.add_dock_space(id, DockNodeFlags::NONE)?;
                    dock.set_node_size(root, [420.0, 240.0]);
                    let (left, right) = dock.split(root, Dir::Left, 0.4)?;
                    dock.dock_window(DOCKED, left);
                    dock.dock_window(DOCKED_2, right);
                    Some(())
                });
            }
            let mut open = true;
            ui.window("Rust dock host").open(&mut open).size([420.0, 260.0], Cond::FirstUseEver).show(|ui| {
                if ui.dock_space(id, [0.0, 0.0], DockNodeFlags::NONE).is_none() {
                    ui.text_wrapped("Docking is off in MacroQuest's overlay settings.");
                }
            });
            for title in [DOCKED, DOCKED_2] {
                ui.window(title).dock(id, Cond::FirstUseEver).show(|ui| {
                    ui.text(format!("Docked: {}", ui.is_window_docked()));
                    ui.text("Drag my tab out to undock me.");
                    if let Some(node) = ui.window_dock_id().and_then(|node| ui.dock_node(node)) {
                        ui.text_disabled(format!("Node {:08X}, {:.0}x{:.0}, {} window(s)", node.id.0, node.size[0], node.size[1], node.windows.len()));
                    }
                });
            }
            g.dock_demo = open;
        }
        if g.exercise.as_ref().is_some_and(|e| e.frames_left > 0) {
            self.exercise_frame(mq, ui);
        }
    }

    /// One frame of the selftest's exercise: every bound graphics feature.
    fn exercise_frame(&mut self, mq: &Mq<Self>, ui: &mut Ui) {
        let in_game = mq.game_state() == GameState::InGame;
        let g = &mut self.graphics;
        let size = g.texture(ui).map(Texture::size);
        g.icons(ui);
        let item_cell = g.item_icons.as_ref().and_then(|a| a.size().1);
        let spell_cell = g.spell_icons.as_ref().and_then(|a| a.size().1);
        let mq_fonts = [Font::Default, Font::Console, Font::Large].into_iter().all(|f| ui.has_font(f));
        let eq_fonts: Vec<u8> = (0..10).filter(|&style| ui.has_font(Font::Eq(style))).collect();
        let docking = ui.is_docking_enabled();
        let mut plotted = false;
        let dock_id = ui.dock_id("rusttest exercise dock space");
        let first_frame = g.exercise.as_ref().is_some_and(|e| e.frames_drawn == 0);
        if first_frame {
            ui.dock_builder(|dock| {
                let root = dock.add_dock_space(dock_id, DockNodeFlags::NO_TAB_BAR)?;
                dock.set_node_size(root, [200.0, 60.0]);
                dock.dock_window(EXERCISE_DOCKED, root);
                Some(())
            });
        }
        let (getter_points, formatted, scaled) = (Cell::new(0), Cell::new(false), Cell::new(false));
        let mut spell_gems = 0;
        let mut clipped_lines = 0;
        let mut custom_item = false;
        let selection = &mut g.exercise_selection;
        ui.window("Rust plugin test: graphics selftest")
            .position([40.0, 40.0], Cond::Appearing)
            .size([520.0, 560.0], Cond::Appearing)
            .flags(WindowFlags::NO_SAVED_SETTINGS | WindowFlags::NO_FOCUS_ON_APPEARING)
            .show(|ui| {
                ui.text("Drawing every graphics feature for a few frames...");
                // First: ImPlot skips a plot scrolled out of view, and the rest is tall.
                let xs: Vec<f64> = (0..50).map(|i| f64::from(i) * 0.2).collect();
                let ys: Vec<f64> = xs.iter().map(|x| x.sin()).collect();
                plotted = ui
                    .plot("exercise plot")
                    .size([-1.0, 180.0])
                    .show(|plot| {
                        plot.setup_axes(Some("x"), Some("sin x"));
                        plot.setup_legend(Location::NorthEast, LegendFlags::NONE);
                        plot.line("line", (&xs, &ys));
                        plot.scatter("scatter", (&xs[..10], &ys[..10]));
                        plot.bars("bars", &[0.2f32, 0.5, 0.3], 0.5);
                        plot.shaded("shaded", (&xs, &ys), 0.0);
                        plot.histogram("histogram", &ys, Bins::Count(8), 0.1, None, HistogramFlags::NONE);
                        plot.pie_chart(&["a", "b"], &[1.0f32, 2.0], [8.0, 0.0], 0.5, Some("%.0f"), 90.0, PieChartFlags::NONE);
                        plot.heatmap("heat", &[0.0f32, 0.5, 1.0, 0.25], 2, None, None, [[5.0, -1.0], [6.0, -0.5]], HeatmapFlags::NONE);
                        plot.drag_line_x(1, &mut g.drag_x, YELLOW, 1.0, DragToolFlags::NONE);
                        plot.tag_x(g.drag_x, YELLOW, None);
                        plot.draw(|draw| draw.add_circle([0.0, 0.0], 4.0, [1.0; 4], 1.0));
                    })
                    .is_some();
                // In view too: a child out of view skips its contents.
                ui.child("exercise clipped list").size([-1.0, 80.0]).show(|ui| {
                    ui.list_clipper(CLIPPED_LINES).show(|ui, visible| {
                        for line in visible {
                            ui.text(format!("Clipped line {line}"));
                            clipped_lines += 1;
                        }
                    });
                });
                // A custom widget: an item, its behavior, a frame and text, and a splitter.
                if let Some(item) = ui.add_item("exercise custom item", [160.0, 24.0]) {
                    let state = ui.button_behavior(&item, ButtonFlags::PRESSED_ON_CLICK);
                    ui.render_frame(item.min, item.max, if state.hovered { YELLOW } else { GREY }, true, 4.0);
                    ui.render_text_clipped(item.min, item.max, "Custom item##hidden", [0.5, 0.5]);
                    ui.draw_list().add_arrow([item.min[0] + 4.0, item.min[1] + 4.0], [1.0; 4], Dir::Right, 0.8);
                    let [x, y] = [item.max[0] + 8.0, item.min[1]];
                    let mut sizes = [40.0, 40.0];
                    ui.splitter("##exercise splitter", Split::Columns, [x + 40.0, y], [x + 44.0, y + 24.0], &mut sizes, [10.0, 10.0]);
                    custom_item = true;
                }
                ui.plot("exercise closures").size([-1.0, 160.0]).show(|plot| {
                    plot.setup_axis_scale_fn(Axis::X1, |x| {
                        scaled.set(true);
                        x.max(0.0).sqrt()
                    }, |x| x * x);
                    plot.setup_axis_format_fn(Axis::Y1, |y| {
                        formatted.set(true);
                        format!("{:.0}%", y * 100.0)
                    });
                    plot.line_fn("from a closure", 50, |i| {
                        getter_points.set(getter_points.get() + 1);
                        let x = i as f64 * 0.5;
                        [x, (x * 0.4).cos() * 0.5 + 0.5]
                    }, LineFlags::NONE);
                    plot.pie_chart_fn(&["a", "b"], &[1.0f64, 3.0], [20.0, 0.5], 0.3, |v| format!("{v} kills"), 90.0, PieChartFlags::NORMALIZE);
                });
                // In view too: a dock space out of view hides what's docked in it.
                ui.dock_space(dock_id, [-1.0, 60.0], DockNodeFlags::NONE);
                if let Some(id) = g.texture.as_ref().and_then(Option::as_ref).and_then(Texture::id) {
                    ui.image(id, [32.0, 32.0]);
                    ui.same_line();
                    ui.image_button("exercise image", id, [24.0, 24.0]);
                }
                if let Some(icons) = g.item_icons.as_mut().filter(|_| in_game) {
                    icons.set_item_icon(500);
                    ui.same_line();
                    ui.texture_animation(icons, Some([32.0, 32.0]), true);
                }
                ui.drag_drop_source_with(DragDropFlags::SOURCE_ALLOW_NULL_ID, |drag| drag.set_payload(ICON_PAYLOAD, b"exercise"));
                for font in [Font::Default, Font::Console, Font::Large].into_iter().chain(eq_fonts.iter().map(|&s| Font::Eq(s))) {
                    ui.with_font(font, |ui| ui.text(format!("{font:?}")));
                }
                ui.with_font_size(28.0, |ui| ui.text("28 px"));
                let draw = ui.draw_list();
                let [x, y] = ui.cursor_screen_pos();
                draw.channels(2, |layers| {
                    layers.set_current(1);
                    layers.add_text([x + 4.0, y + 4.0], [1.0; 4], "over");
                    layers.set_current(0);
                    layers.add_rect_filled([x, y], [x + 60.0, y + 24.0], HP_RED, 3.0);
                });
                let mut path = draw.path();
                path.line_to([x + 70.0, y]).arc_to([x + 90.0, y + 12.0], 12.0, -1.5, 1.5).line_to([x + 70.0, y + 24.0]);
                path.stroke(YELLOW, true, 2.0);
                draw.with_clip_rect([x, y], [x + 200.0, y + 24.0], true, |draw| {
                    draw.add_ngon_filled([x + 130.0, y + 12.0], 12.0, MANA_BLUE, 6);
                    draw.add_bezier_cubic([[x + 150.0, y], [x + 170.0, y + 24.0], [x + 180.0, y], [x + 200.0, y + 24.0]], [1.0; 4], 2.0);
                });
                ui.dummy([200.0, 28.0]);
                ui.drag_drop_target(|drop| drop.accept(ICON_PAYLOAD).map(|p| p.data().len()));
                ui.multi_select(MultiSelectFlags::BOX_SELECT_1D | MultiSelectFlags::SCOPE_RECT, selection, 3, |list, selection| {
                    for index in 0..3 {
                        list.set_next_item_index(index);
                        list.selectable(&format!("Selectable {index}"), selection.contains(index));
                    }
                });
                if in_game {
                    for gem in 0..SPELL_GEMS {
                        if gem > 0 {
                            ui.same_line();
                        }
                        spell_gems += usize::from(ui.with_id(gem, |ui| ui.spell_gem("exercise gem", gem, SpellGemFlags::NONE)).is_some());
                    }
                    ui.item_link_text("An item link", [0.3, 0.8, 1.0, 1.0], None);
                    let [x, y] = ui.cursor_screen_pos();
                    ui.draw_list().add_eq_text(2, "EQ font text", [x, y], [x + 200.0, y + 20.0], YELLOW, EqTextFlags::NO_WRAP);
                    ui.dummy([200.0, 20.0]);
                }
            });
        let docked = docking.then(|| {
            let inside = ui.window(EXERCISE_DOCKED).flags(WindowFlags::NO_SAVED_SETTINGS).show(|ui| ui.window_dock_id()).flatten();
            // The node knows its windows even on a frame the window skips drawing.
            let held = ui.dock_node(dock_id).is_some_and(|node| node.windows.iter().any(|w| w == EXERCISE_DOCKED));
            inside == Some(dock_id) || held
        });
        // Outlines the screen while the exercise runs, over and under every window.
        let ([sx, sy], [sw, sh]) = ui.screen_rect();
        ui.foreground_draw_list().add_rect([sx + 2.0, sy + 2.0], [sx + sw - 2.0, sy + sh - 2.0], YELLOW, 0.0, 2.0);
        ui.background_draw_list().add_text([sx + 8.0, sy + sh - 24.0], YELLOW, "mq-emu graphics selftest");
        if let Some(exercise) = g.exercise.as_mut() {
            exercise.frames_left -= 1;
            exercise.frames_drawn += 1;
            exercise.texture_size = size;
            exercise.item_icon_cell = item_cell;
            exercise.spell_icon_cell = spell_cell;
            exercise.mq_fonts = mq_fonts;
            exercise.eq_fonts = eq_fonts;
            exercise.docking = docking;
            exercise.plotted |= plotted;
            // A docked window may skip a frame as it docks: any frame counts.
            exercise.docked = docked.map(|now| now || exercise.docked == Some(true));
            exercise.getter_points += getter_points.get();
            exercise.formatted |= formatted.get();
            exercise.scaled |= scaled.get();
            exercise.spell_gems = exercise.spell_gems.max(spell_gems);
            exercise.clipped_lines = exercise.clipped_lines.max(clipped_lines);
            exercise.custom_item |= custom_item;
        }
    }

    /// ImPlot: pulse timing, HP and mana over time, and the spawns around you.
    pub(crate) fn plots_tab(&mut self, mq: &Mq<Self>, ui: &mut Ui) {
        let now = self.graphics.started.elapsed().as_secs_f64();
        let g = &mut self.graphics;
        if let Some(me) = mq.me().filter(|_| mq.game_state() == GameState::InGame) {
            if g.history.back().is_none_or(|(t, ..)| now - t >= 0.25) {
                let read = |path| mq.data(path).ok().flatten().and_then(|d| d.as_int()).unwrap_or(0) as f32;
                let mana = read("Me.PctMana");
                g.history.push_back((now, me.hp_percent() as f32, mana));
            }
            while g.history.front().is_some_and(|(t, ..)| now - t > HISTORY_SECONDS) {
                g.history.pop_front();
            }
        }

        let gaps: Vec<f64> = self
            .recent_pulses
            .iter()
            .zip(self.recent_pulses.iter().skip(1))
            .map(|(a, b)| (*b - *a).as_secs_f64() * 1000.0)
            .collect();
        let average = if gaps.is_empty() { 0.0 } else { gaps.iter().sum::<f64>() / gaps.len() as f64 };
        let g = &mut self.graphics;
        ui.checkbox("Pan with the right button, select with the left (these plots only)", &mut g.reverse_mouse);
        ui.same_line();
        ui.plot_input_map_selector("Every plot");
        ui.item_tooltip_text("ImPlot's map for every plot in the game, MacroQuest's and other plugins' too");
        let map = if g.reverse_mouse { ui.reverse_plot_input_map() } else { ui.plot_input_map() };
        ui.with_plot_input_map(&map, |ui| Self::plots(g, mq, ui, &gaps, average));
    }

    fn plots(g: &mut Graphics, mq: &Mq<Self>, ui: &mut Ui, gaps: &[f64], average: f64) {
        ui.subplots("##timing and vitals", 1, 2).size([-1.0, 220.0]).flags(SubplotFlags::NO_RESIZE).show(|grid| {
            grid.plot("Pulse gaps", PlotFlags::NONE, |plot| {
                plot.setup_axes(Some("pulse"), Some("ms"));
                plot.setup_axis_limits(Axis::Y1, 0.0..=60.0, Cond::Once);
                plot.setup_axis_format(Axis::Y1, "%.0f ms");
                plot.next_fill_style(None, Some(0.25));
                plot.line_with("gap", gaps, LineFlags::SHADED);
                plot.inf_lines("average", &[average], InfLinesFlags::HORIZONTAL);
                plot.annotation([gaps.len() as f64, average], YELLOW, [-10.0, -10.0], true, Some(format!("avg {average:.1} ms").as_str()));
            });
            grid.plot("HP and mana", PlotFlags::NONE, |plot| {
                plot.setup_axes(Some("seconds ago"), None);
                plot.setup_axis_limits(Axis::Y1, 0.0..=105.0, Cond::Always);
                plot.setup_axis(Axis::X1, Some("seconds ago"), AxisFlags::AUTO_FIT);
                // Labels and points from closures: no copies of the history.
                let newest = g.history.back().map_or(0.0, |h| h.0);
                plot.setup_axis_format_fn(Axis::X1, move |t| format!("{:.0}", newest - t));
                plot.setup_axis_format_fn(Axis::Y1, |pct| match pct {
                    p if p >= 100.0 => "full".to_owned(),
                    p if p <= 0.0 => "empty".to_owned(),
                    p => format!("{p:.0}%"),
                });
                let history = &g.history;
                plot.next_line_style(Some(HP_RED), Some(2.0));
                plot.line_fn("HP", history.len(), |i| [history[i].0, f64::from(history[i].1)], LineFlags::NONE);
                plot.next_line_style(Some(MANA_BLUE), Some(2.0));
                plot.line_fn("Mana", history.len(), |i| [history[i].0, f64::from(history[i].2)], LineFlags::NONE);
            });
        });

        // The spawns around you, as a radar you can drop a marker on.
        let Some(me) = mq.me().filter(|_| mq.game_state() == GameState::InGame) else {
            ui.text_disabled("The spawn plots need you in game.");
            return;
        };
        let here = me.position();
        let (mut xs, mut ys, mut npc_x, mut npc_y) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut counts = [0f64; 3];
        for spawn in mq.spawns().filter(|s| s.id() != me.id()) {
            let p = spawn.position();
            let (dx, dy) = (f64::from(p.x - here.x), f64::from(p.y - here.y));
            if dx.hypot(dy) > 300.0 {
                continue;
            }
            match spawn.spawn_type() {
                SpawnType::Npc => {
                    npc_x.push(dx);
                    npc_y.push(dy);
                    counts[1] += 1.0;
                }
                SpawnType::Player => {
                    xs.push(dx);
                    ys.push(dy);
                    counts[0] += 1.0;
                }
                _ => counts[2] += 1.0,
            }
        }
        ui.subplots("##spawns", 1, 3).size([-1.0, 300.0]).show(|grid| {
            grid.plot("Within 300 (drag the marker)", PlotFlags::EQUAL, |plot| {
                plot.setup_axis_limits(Axis::X1, -300.0..=300.0, Cond::Once);
                plot.setup_axis_limits(Axis::Y1, -300.0..=300.0, Cond::Once);
                plot.next_marker_style(Marker::Circle, Some(4.0), Some([0.3, 0.8, 0.3, 1.0]), None, None);
                plot.scatter_with("players", (&xs, &ys), ScatterFlags::NO_CLIP);
                plot.next_marker_style(Marker::Diamond, Some(4.0), Some(HP_RED), None, None);
                plot.scatter("NPCs", (&npc_x, &npc_y));
                plot.next_marker_style(Marker::Cross, Some(8.0), None, Some(2.0), Some([1.0; 4]));
                plot.scatter("you", (&[0.0][..], &[0.0][..]));
                let drag = plot.drag_point(1, &mut g.marker, YELLOW, 6.0, DragToolFlags::NONE);
                let [mx, my] = g.marker;
                plot.annotation(g.marker, YELLOW, [10.0, -10.0], true, Some(format!("{:.0} away", mx.hypot(my)).as_str()));
                if drag.hovered {
                    plot.tag_x(mx, YELLOW, None);
                    plot.tag_y(my, YELLOW, None);
                }
            });
            grid.plot("By type", PlotFlags::NONE, |plot| {
                plot.setup_axis_ticks(Axis::X1, &[0.0, 1.0, 2.0], Some(&["players", "NPCs", "other"]), false);
                // Room for the bars' full width, not just their centers.
                plot.setup_axis_limits(Axis::X1, -0.5..=2.5, Cond::Always);
                let tallest = counts.iter().copied().fold(1.0, f64::max);
                plot.setup_axis_limits(Axis::Y1, 0.0..=tallest * 1.15, Cond::Always);
                plot.bars_with("count", &counts, 0.6, BarsFlags::NONE);
            });
            // A pie needs equal units on both axes, or it's squashed.
            grid.plot("Share", PlotFlags::EQUAL | PlotFlags::NO_MOUSE_TEXT, |plot| {
                plot.setup_axis(Axis::X1, None, AxisFlags::NO_DECORATIONS);
                plot.setup_axis(Axis::Y1, None, AxisFlags::NO_DECORATIONS);
                plot.setup_axis_limits(Axis::X1, 0.0..=1.0, Cond::Always);
                plot.setup_axis_limits(Axis::Y1, 0.0..=1.0, Cond::Always);
                let total: f64 = counts.iter().sum();
                plot.pie_chart_fn(&["players", "NPCs", "other"], &counts, [0.5, 0.5], 0.4, |n| {
                    if total > 0.0 { format!("{n:.0} ({:.0}%)", n / total * 100.0) } else { String::new() }
                }, 90.0, PieChartFlags::NORMALIZE);
            });
        });
        // Distances on a square-root scale: the near spawns get the room.
        let mut distances: Vec<f64> = (xs.iter().zip(&ys)).chain(npc_x.iter().zip(&npc_y)).map(|(x, y)| x.hypot(*y)).collect();
        distances.sort_by(f64::total_cmp);
        ui.plot("Spawn distances (square-root scale)").size([-1.0, 160.0]).show(|plot| {
            plot.setup_axis_scale_fn(Axis::X1, |d| d.max(0.0).sqrt(), |s| s * s);
            plot.setup_axis_limits(Axis::X1, 0.0..=300.0, Cond::Always);
            plot.setup_axis_format_fn(Axis::Y1, |n| format!("{n:.0}"));
            plot.stairs_fn("spawns within", distances.len(), |i| [distances[i], (i + 1) as f64], StairsFlags::NONE);
        });
        ui.with_colormap(Colormap::VIRIDIS, |ui| ui.colormap_scale("Viridis", 0.0..=100.0, [0.0, 200.0], "%.0f", None));
    }

    /// Draw lists on a canvas, and the foreground and background overlays.
    pub(crate) fn drawing_tab(&mut self, ui: &mut Ui) {
        let g = &mut self.graphics;
        ui.checkbox("Crosshair at the mouse (foreground)", &mut g.crosshair);
        ui.checkbox("HUD text (background)", &mut g.hud);
        let size = [ui.content_region_avail()[0].max(100.0), 220.0];
        let [x, y] = ui.cursor_screen_pos();
        let clicked = ui.invisible_button("canvas", size);
        let hovered = ui.is_item_hovered();
        let t = ui.time() as f32;
        let draw = ui.draw_list();
        draw.add_rect_filled_multicolor([x, y], [x + size[0], y + size[1]], [
            [0.1, 0.1, 0.2, 1.0],
            [0.2, 0.1, 0.1, 1.0],
            [0.1, 0.2, 0.1, 1.0],
            [0.1, 0.1, 0.1, 1.0],
        ]);
        draw.with_clip_rect([x, y], [x + size[0], y + size[1]], true, |draw| {
            let c = [x + 60.0, y + 60.0];
            draw.add_circle(c, 40.0, [1.0; 4], 2.0);
            draw.add_line(c, [c[0] + 40.0 * t.cos(), c[1] + 40.0 * t.sin()], YELLOW, 3.0);
            draw.add_ngon_filled([x + 160.0, y + 60.0], 35.0, MANA_BLUE, 5);
            draw.add_ellipse([x + 260.0, y + 60.0], [40.0, 20.0], [0.3, 0.9, 0.3, 1.0], t, 2.0);
            draw.add_triangle_filled([[x + 330.0, y + 90.0], [x + 360.0, y + 30.0], [x + 390.0, y + 90.0]], HP_RED);
            draw.add_rect_corners([x + 20.0, y + 120.0], [x + 120.0, y + 200.0], [1.0; 4], 12.0, Corners::TOP_LEFT | Corners::BOTTOM_RIGHT, 2.0);
            let star: Vec<[f32; 2]> = (0..10)
                .map(|i| {
                    let a = i as f32 * std::f32::consts::PI / 5.0 - std::f32::consts::FRAC_PI_2;
                    let r = if i % 2 == 0 { 40.0 } else { 16.0 };
                    [x + 190.0 + r * a.cos(), y + 160.0 + r * a.sin()]
                })
                .collect();
            draw.add_concave_poly_filled(&star, YELLOW);
            let mut path = draw.path();
            path.line_to([x + 260.0, y + 200.0])
                .bezier_cubic_to([[x + 280.0, y + 110.0], [x + 340.0, y + 210.0], [x + 380.0, y + 120.0]])
                .arc_to([x + 380.0, y + 160.0], 40.0, -std::f32::consts::FRAC_PI_2, std::f32::consts::FRAC_PI_2);
            path.stroke([1.0, 0.5, 1.0, 1.0], false, 3.0);
            draw.channels(2, |layers| {
                // Drawn first, on top.
                layers.set_current(1);
                layers.add_text([x + size[0] - 140.0, y + 10.0], [1.0; 4], "channel 1 (on top)");
                layers.set_current(0);
                layers.add_rect_filled([x + size[0] - 150.0, y + 5.0], [x + size[0] - 5.0, y + 30.0], [0.4, 0.2, 0.6, 1.0], 4.0);
            });
            if hovered {
                let m = ui.mouse_pos();
                draw.add_circle_filled(m, 6.0, [1.0, 1.0, 1.0, 0.5]);
            }
        });
        if clicked {
            g.crosshair = !g.crosshair;
        }
        ui.text_disabled("Click the canvas to toggle the crosshair.");
    }

    /// Textures, the game's icons, and dragging icons onto a box.
    pub(crate) fn images_tab(&mut self, mq: &Mq<Self>, ui: &mut Ui) {
        let in_game = mq.game_state() == GameState::InGame;
        let g = &mut self.graphics;
        ui.separator_text("A texture from a file");
        match g.texture(ui).and_then(|t| t.id().map(|id| (id, t.size()))) {
            Some((id, size)) => {
                ui.image(id, [64.0, 64.0]);
                ui.same_line();
                ui.image_with(id, [64.0, 64.0], ImageOptions { uv: [[0.0, 0.0], [0.5, 0.5]], background: GREY, tint: [1.0, 0.6, 0.6, 1.0] });
                ui.same_line();
                ui.image_button("image button", id, [32.0, 32.0]);
                ui.text(format!("{}x{} BMP written to the temp directory", size[0], size[1]));
            }
            None => ui.text_disabled("The test image didn't load."),
        }

        ui.separator_text("The game's icons");
        g.icons(ui);
        if !in_game {
            ui.text_disabled("Icons need you in game.");
            return;
        }
        let Some(items) = g.item_icons.as_mut() else {
            ui.text_disabled("No item icon sheet (A_DragItem).");
            return;
        };
        ui.text("Your worn items (drag one onto the box):");
        if let Some(inventory) = mq.inventory() {
            for (index, (slot, item)) in inventory.worn().enumerate() {
                if index % 11 != 0 {
                    ui.same_line();
                }
                items.set_item_icon(item.icon());
                ui.with_id(index, |ui| {
                    ui.texture_animation(items, Some([32.0, 32.0]), true);
                    ui.item_tooltip_text(format!("{slot}: {}", item.name()));
                    let name = item.name();
                    ui.drag_drop_source_with(DragDropFlags::SOURCE_ALLOW_NULL_ID, |drag| {
                        drag.set_payload(ICON_PAYLOAD, name.as_bytes());
                        drag.text(format!("Dragging {name}"));
                    });
                });
            }
        }
        if let Some(spells) = g.spell_icons.as_mut() {
            ui.text("Your spell gems:");
            for (index, (gem, spell)) in mq.gems().enumerate() {
                if index > 0 {
                    ui.same_line();
                }
                spells.set_cell(spell.icon());
                ui.with_id(1000 + index, |ui| {
                    ui.texture_animation(spells, Some([28.0, 28.0]), false);
                    ui.item_tooltip_text(format!("Gem {}: {}", gem + 1, spell.name()));
                });
            }
        }
        ui.button_sized("Drop an item here", [200.0, 50.0]);
        if let Some(name) = ui.drag_drop_target(|drop| drop.accept(ICON_PAYLOAD).and_then(|p| p.as_str().map(str::to_owned))).flatten() {
            g.dropped = Some(name);
        }
        ui.same_line();
        ui.text(g.dropped.as_deref().map_or("Nothing dropped yet.".to_owned(), |n| format!("Dropped: {n}")));

        ui.separator_text("MacroQuest's EQ-styled widgets");
        ui.checkbox("Let the gems cast (left click) and forget spells (right click)", &mut g.gems_act);
        let flags = if g.gems_act { SpellGemFlags::ALLOW_SPELL_CAST | SpellGemFlags::ALLOW_UNMEMORIZE } else { SpellGemFlags::NONE };
        let mut clicked = None;
        for gem in 0..SPELL_GEMS {
            if gem > 0 {
                ui.same_line();
            }
            if ui.with_id(gem, |ui| ui.spell_gem("gem", gem, flags)) == Some(true) {
                clicked = Some(gem);
            }
            ui.item_tooltip_text(format!("Gem {}", gem + 1));
        }
        if let Some(gem) = clicked {
            g.dropped = Some(format!("gem {} clicked", gem + 1));
        }
        if let Some(inventory) = mq.inventory() {
            for (slot, item) in inventory.worn().take(4) {
                if ui.item_link_text(&format!("{slot}: {}", item.name()), [0.3, 0.8, 1.0, 1.0], None) {
                    g.dropped = Some(format!("link to {}", item.name()));
                }
            }
        }
        let [x, y] = ui.cursor_screen_pos();
        let draw = ui.draw_list();
        for style in 0..4u8 {
            let top = y + f32::from(style) * 18.0;
            draw.add_eq_text(style, &format!("EQ font style {style}, laid out by the game"), [x, top], [x + 400.0, top + 18.0], YELLOW, EqTextFlags::NO_WRAP);
        }
        ui.dummy([400.0, 72.0]);
    }

    /// MacroQuest's and EQ's fonts, sizes, and docking.
    pub(crate) fn fonts_tab(&mut self, ui: &mut Ui) {
        let g = &mut self.graphics;
        ui.separator_text("MacroQuest's fonts");
        for font in [Font::Default, Font::Console, Font::Large] {
            ui.with_font(font, |ui| ui.text(format!("{font:?}: The quick brown fox \u{f0c9} \u{f013}")));
        }
        ui.separator_text("EverQuest's fonts");
        let mut any = false;
        for style in 0..10 {
            if ui.has_font(Font::Eq(style)) {
                any = true;
                ui.with_font(Font::Eq(style), |ui| ui.text(format!("EQ font style {style}: The quick brown fox")));
            }
        }
        if !any {
            ui.text_disabled("No EQ fonts (they come with the game's UI).");
        }
        ui.separator_text("Sizes");
        ui.set_next_item_width(200.0);
        ui.slider_format("Size", &mut g.font_size, 8.0..=48.0, "%.0f px");
        let size = g.font_size;
        ui.with_font_size(size, |ui| ui.text("Text at the chosen size"));
        ui.with_font_sized(Font::Large, size, |ui| ui.text("The large font at the chosen size"));
        ui.text(format!("Current font: {:.0} px (base {:.0} px)", ui.font_size(), ui.font_size_base()));

        ui.separator_text("Docking");
        ui.text(format!("Docking is {}.", if ui.is_docking_enabled() { "on" } else { "off (MacroQuest's overlay settings)" }));
        ui.checkbox("Show a dock space with windows docked in it", &mut g.dock_demo);
        ui.same_line();
        if ui.button("Lay it out with the dock builder") {
            g.dock_demo = true;
            g.rebuild_dock = true;
        }
        ui.item_tooltip_text("Splits the dock space in two and docks a window in each half");
        ui.text(format!("This window docked: {}", ui.is_window_docked()));
    }
}
