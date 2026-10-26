//! The debug window (`/rusttest window`): the latest selftest's results, the
//! runtime's stats, what mq-emu sees of the game, and a sampler of every
//! ImGui widget mq-emu binds, to try them by hand (plots, drawing, images,
//! fonts and docking are in graphics.rs).
//!
//! Buttons don't act mid-frame: they run `/rusttest` commands, which
//! MacroQuest delivers on the next pulse.

use std::time::Instant;

use mq_emu::imgui::{Cond, PlotOptions, StyleColor, StyleVar, TableColumnFlags, TableFlags, TreeNodeFlags, Ui, WindowFlags};
use mq_emu::{GameState, Mq, SpawnType};

use crate::checks::{Counts, Outcome};
use crate::{COMMAND, RustTest};

const GREEN: [f32; 4] = [0.35, 0.85, 0.35, 1.0];
const RED: [f32; 4] = [0.95, 0.35, 0.35, 1.0];
const YELLOW: [f32; 4] = [0.95, 0.85, 0.3, 1.0];
const GREY: [f32; 4] = [0.6, 0.6, 0.6, 1.0];
const HP_RED: [f32; 4] = [0.75, 0.15, 0.15, 1.0];
const MANA_BLUE: [f32; 4] = [0.2, 0.4, 0.85, 1.0];

/// The most spawns the game tab lists.
const MAX_LISTED_SPAWNS: usize = 50;

/// The window's own state, kept across frames.
pub struct DebugWindow {
    pub open: bool,
    /// `on_update_imgui` calls, open or not.
    pub frames: u64,
    /// Frames the window's contents were drawn.
    pub drawn: u64,
    failures_only: bool,
    spawn_range: f32,
    sampler: Sampler,
}

impl Default for DebugWindow {
    fn default() -> Self {
        DebugWindow { open: false, frames: 0, drawn: 0, failures_only: false, spawn_range: 100.0, sampler: Sampler::default() }
    }
}

/// Values the widgets tab edits.
struct Sampler {
    checked: bool,
    fraction: f32,
    count: i32,
    speed: f32,
    radio: i32,
    choice: usize,
    name: String,
    notes: String,
    color: [f32; 4],
    clicks: u32,
    selected: Option<usize>,
}

impl Default for Sampler {
    fn default() -> Self {
        Sampler {
            checked: true,
            fraction: 0.4,
            count: 3,
            speed: 1.5,
            radio: 1,
            choice: 0,
            name: "Rusty".into(),
            notes: "Line one\nLine two".into(),
            color: [0.4, 0.7, 1.0, 1.0],
            clicks: 0,
            selected: None,
        }
    }
}

impl RustTest {
    pub(crate) fn draw_window(&mut self, mq: &Mq<Self>, ui: &mut Ui) {
        self.window.frames += 1;
        let mut open = self.window.open;
        ui.window("Rust plugin test")
            .open(&mut open)
            .size([560.0, 480.0], Cond::FirstUseEver)
            .flags(WindowFlags::MENU_BAR)
            .show(|ui| {
                self.window.drawn += 1;
                self.menu_bar(mq, ui);
                ui.tab_bar("tabs", |tabs| {
                    tabs.tab("Selftest", |ui| self.selftest_tab(mq, ui));
                    tabs.tab("Runtime", |ui| self.runtime_tab(mq, ui));
                    tabs.tab("Game", |ui| self.game_tab(mq, ui));
                    tabs.tab("Widgets", |ui| self.widgets_tab(ui));
                    tabs.tab("Plots", |ui| self.plots_tab(mq, ui));
                    tabs.tab("Drawing", |ui| self.drawing_tab(ui));
                    tabs.tab("Images", |ui| self.images_tab(mq, ui));
                    tabs.tab("Fonts & docking", |ui| self.fonts_tab(ui));
                });
            });
        self.window.open = open;
    }

    fn menu_bar(&mut self, mq: &Mq<Self>, ui: &mut Ui) {
        ui.menu_bar(|ui| {
            ui.menu("Test", |ui| {
                if ui.menu_item_with("Run selftest", Some(&format!("{COMMAND} selftest")), false, self.selftest.is_none()) {
                    mq.command(format!("{COMMAND} selftest"));
                }
                if ui.menu_item_with("Stop", Some(&format!("{COMMAND} stop")), false, true) {
                    mq.command(format!("{COMMAND} stop"));
                }
                ui.separator();
                if ui.menu_item("Close window") {
                    self.window.open = false;
                }
            });
        });
    }

    fn selftest_tab(&mut self, mq: &Mq<Self>, ui: &mut Ui) {
        let running = self.selftest.is_some();
        ui.disabled(running, |ui| {
            if ui.button("Run selftest") {
                mq.command(format!("{COMMAND} selftest"));
            }
        });
        ui.same_line();
        ui.disabled(!running, |ui| {
            if ui.button("Stop") {
                mq.command(format!("{COMMAND} stop"));
            }
        });
        ui.same_line();
        ui.checkbox("Failures only", &mut self.window.failures_only);

        let log = self.selftest_log.borrow();
        let mut total = Counts::default();
        for (_, entries) in &log.sections {
            total.add(Counts::of(entries));
        }
        match (log.started, log.took) {
            (None, _) => ui.text_disabled("Not run yet."),
            (Some(started), None) if running => ui.text(format!("Running... {:.1}s", started.elapsed().as_secs_f64())),
            (Some(_), None) => ui.text_colored(YELLOW, "Stopped before it finished."),
            (Some(_), Some(took)) => ui.text(format!("Finished in {:.2}s.", took.as_secs_f64())),
        }
        if log.started.is_some() {
            ui.same_line();
            ui.text_colored(GREEN, format!("{} passed", total.passed));
            ui.same_line();
            ui.text_colored(if total.failed > 0 { RED } else { GREY }, format!("{} failed", total.failed));
            ui.same_line();
            ui.text_colored(if total.skipped > 0 { YELLOW } else { GREY }, format!("{} skipped", total.skipped));
        }
        ui.separator();

        ui.child("results").show(|ui| {
            for (index, (title, entries)) in log.sections.iter().enumerate() {
                let counts = Counts::of(entries);
                if self.window.failures_only && counts.failed == 0 {
                    continue;
                }
                // "###" keeps the header's ID (and open state) as its counts change.
                let label = format!("{}. {title}  ({}/{})###section{index}", index + 1, counts.passed, entries.len());
                let color = match counts {
                    _ if entries.is_empty() => GREY,
                    Counts { failed: 1.., .. } => RED,
                    Counts { skipped: 1.., .. } => YELLOW,
                    _ => GREEN,
                };
                let open = ui.with_style_color(StyleColor::Text, color, |ui| ui.collapsing_header(&label));
                if !open {
                    continue;
                }
                ui.with_id(index, |ui| {
                    ui.table("checks", 3).flags(TableFlags::ROW_BG | TableFlags::BORDERS_INNER_V | TableFlags::RESIZABLE).show(|table| {
                        table.setup_column_with("", TableColumnFlags::WIDTH_FIXED, 40.0);
                        table.setup_column_with("Check", TableColumnFlags::WIDTH_STRETCH, 3.0);
                        table.setup_column_with("Detail", TableColumnFlags::WIDTH_STRETCH, 2.0);
                        for entry in entries {
                            if self.window.failures_only && entry.outcome != Outcome::Fail {
                                continue;
                            }
                            table.next_row();
                            table.next_column();
                            match entry.outcome {
                                Outcome::Pass => table.text_colored(GREEN, "PASS"),
                                Outcome::Fail => table.text_colored(RED, "FAIL"),
                                Outcome::Skip => table.text_colored(YELLOW, "SKIP"),
                            }
                            table.item_tooltip_text(format!("at {:.2}s", entry.at.as_secs_f64()));
                            table.next_column();
                            table.text_wrapped(&entry.label);
                            table.next_column();
                            table.text_wrapped(&entry.detail);
                        }
                    });
                });
            }
        });
    }

    fn runtime_tab(&mut self, mq: &Mq<Self>, ui: &mut Ui) {
        let stats = mq.stats();
        let ms = |d: std::time::Duration| format!("{:.3} ms", d.as_secs_f64() * 1000.0);
        ui.table("stats", 2).flags(TableFlags::ROW_BG | TableFlags::BORDERS_INNER_V).show(|table| {
            table.setup_column_with("Counter", TableColumnFlags::WIDTH_FIXED, 200.0);
            table.setup_column("Value");
            let rows: [(&str, String); 11] = [
                ("Pulses", stats.pulses.to_string()),
                ("Average pulse", ms(stats.average_pulse())),
                ("Slowest pulse", ms(stats.slowest_pulse)),
                ("Re-entrant events queued", stats.queued.to_string()),
                ("Re-entrant events dropped", stats.dropped.to_string()),
                ("Feedback cap hits", stats.cap_hits.to_string()),
                ("Panics caught", stats.panics.to_string()),
                ("ImGui errors", stats.imgui_errors.to_string()),
                ("Tasks running", mq.task_count().to_string()),
                ("Spawns added / removed", format!("{} / {}", self.spawns_added, self.spawns_removed)),
                ("Chat lines seen", self.chat_lines.to_string()),
            ];
            for (name, value) in rows {
                table.row(&[name, value.as_str()]);
            }
        });
        if ui.button("Reset stats") {
            mq.reset_stats();
        }

        ui.separator_text("Frames");
        let gaps: Vec<f32> = self
            .recent_pulses
            .iter()
            .zip(self.recent_pulses.iter().skip(1))
            .map(|(a, b): (&Instant, &Instant)| (*b - *a).as_secs_f32() * 1000.0)
            .collect();
        let average = if gaps.is_empty() { 0.0 } else { gaps.iter().sum::<f32>() / gaps.len() as f32 };
        let overlay = format!("avg {average:.1} ms");
        ui.set_next_item_width(-f32::MIN_POSITIVE);
        ui.plot_lines_with("##pulse gaps", &gaps, PlotOptions { scale: Some(0.0..=50.0), size: [0.0, 80.0], overlay: Some(&overlay) });
        ui.text(format!(
            "Time between the last {} pulses; ImGui: {:.0} fps, frame {}",
            gaps.len(),
            ui.framerate(),
            ui.frame_count()
        ));
        ui.text(format!("This window drew {} of {} ImGui frames.", self.window.drawn, self.window.frames));
    }

    fn game_tab(&mut self, mq: &Mq<Self>, ui: &mut Ui) {
        let state = mq.game_state();
        let me = mq.me().filter(|_| state == GameState::InGame);
        // Labels on the left: ImGui's own LabelText puts them after the value.
        ui.table("me", 2).flags(TableFlags::ROW_BG | TableFlags::BORDERS_INNER_V).show(|table| {
            table.setup_column_with("", TableColumnFlags::WIDTH_FIXED, 100.0);
            table.setup_column("");
            table.row(&["Game state".to_owned(), format!("{state:?}")]);
            let Some(me) = &me else { return };
            let here = me.position();
            table.row(&["Me".to_owned(), format!("{} (level {})", me.name(), me.level())]);
            table.row(&["Position".to_owned(), format!("{:.1}, {:.1}, {:.1}", here.y, here.x, here.z)]);
            table.row(&["Heading".to_owned(), format!("{:.1}°", me.heading().degrees())]);
            let mut bar = |label: &str, current: i32, max: i32, color: [f32; 4]| {
                table.next_row();
                table.next_column();
                table.text(label);
                table.next_column();
                let fraction = if max > 0 { current as f32 / max as f32 } else { 0.0 };
                table.with_style_color(StyleColor::PlotHistogram, color, |ui| ui.progress_bar_with(fraction, [-f32::MIN_POSITIVE, 0.0], Some("")));
                // ImGui puts a bar's own overlay where the fill ends (and with
                // none, shows a percentage); center ours instead.
                let text = format!("{current} / {max} ({:.0}%)", fraction * 100.0);
                let ((min, max), size) = (table.item_rect(), table.calc_text_size(&text));
                let pos = [(min[0] + max[0] - size[0]) / 2.0, (min[1] + max[1] - size[1]) / 2.0];
                table.draw_list().add_text(pos, table.style_color(StyleColor::Text), text);
            };
            bar("HP", me.hp_current(), me.hp_max(), HP_RED);
            // Your own mana as ${Me} reads it, from the character rather than
            // the spawn's mana fields (Spawn::mana_*, which were empty here).
            let read = |path| mq.data(path).ok().flatten().and_then(|d| d.as_int()).unwrap_or(0) as i32;
            let (mana, max_mana) = (read("Me.CurrentMana"), read("Me.MaxMana"));
            if max_mana > 0 {
                bar("Mana", mana, max_mana, MANA_BLUE);
            }
            let target = match mq.target() {
                Some(t) => format!("{} (level {}, {:.1} away, {}% HP)", t.name(), t.level(), t.distance_to(me), t.hp_percent()),
                None => "none".to_owned(),
            };
            table.row(&["Target".to_owned(), target]);
            if let Some(zone) = mq.zone() {
                let id = zone.id().map_or("?".to_owned(), |id| id.to_string());
                table.row(&["Zone".to_owned(), format!("{} ({}, ID {id}, {:?})", zone.name(), zone.short_name(), zone.zone_type())]);
            }
            let pet = mq.pet().map_or("none".to_owned(), |p| format!("{} (level {}, {}% HP)", p.name(), p.level(), p.hp_percent()));
            table.row(&["Pet".to_owned(), pet]);
        });
        let Some(me) = me else {
            ui.text_disabled(if state == GameState::InGame { "No character." } else { "Spawn details need to be in game." });
            return;
        };
        let here = me.position();

        if ui.collapsing_header("Buffs and spells") {
            for buff in mq.buffs() {
                let ticks = buff.ticks().map_or("permanent".to_owned(), |t| format!("{t} ticks"));
                ui.bullet_text(format!("{} (level {}, {ticks})", buff.name(), buff.level()));
            }
            for song in mq.songs() {
                ui.bullet_text(format!("{} (song)", song.name()));
            }
            ui.separator();
            for (gem, spell) in mq.gems() {
                ui.text(format!("Gem {}: {} ({} mana)", gem + 1, spell.name(), spell.mana_cost()));
            }
            ui.text_disabled(format!("{} spells in the book", mq.spell_book().count()));
        }
        if ui.collapsing_header("Inventory") {
            if let Some(inventory) = mq.inventory() {
                for (slot, item) in inventory.worn() {
                    ui.text(format!("{slot}: {}", item.name()));
                }
                for (slot, bag) in inventory.bags() {
                    let label = format!("{slot}: {}", bag.name());
                    if !bag.is_container() {
                        ui.text(label);
                        continue;
                    }
                    ui.tree_node(&format!("{label} ({}/{} free)", bag.contents().free(), bag.contents().capacity()), |ui| {
                        for (i, item) in bag.contents().iter() {
                            ui.text(format!("{}: {} x{}", i + 1, item.name(), item.stack()));
                        }
                    });
                }
                if let Some(item) = inventory.cursor() {
                    ui.text(format!("cursor: {}", item.name()));
                }
                ui.text_disabled(format!("{} free bag slots", inventory.free_bag_slots()));
            }
        }
        if ui.collapsing_header("Group") {
            match mq.group() {
                Some(group) => {
                    let leader = group.leader().map(|m| m.slot());
                    for member in group.members() {
                        let mark = if Some(member.slot()) == leader { " (leader)" } else { "" };
                        let place = member.spawn().map_or("not in zone".to_owned(), |s| format!("{:.0} away", s.distance_to(&me)));
                        ui.text(format!("{}{mark}, level {}, {place}", member.name(), member.level()));
                    }
                }
                None => ui.text_disabled("Not in a group."),
            }
        }
        if ui.collapsing_header("Ground items") {
            for item in mq.ground_items().take(MAX_LISTED_SPAWNS) {
                let name = item.item().map_or_else(|| item.name(), |i| i.name());
                ui.text(format!("{name} ({:.0} away)", item.position().distance(&here)));
            }
        }

        ui.separator_text("Nearby spawns");
        ui.set_next_item_width(200.0);
        ui.slider_format("Range", &mut self.window.spawn_range, 10.0..=1000.0, "%.0f");
        let mut nearby: Vec<_> = mq
            .spawns()
            .filter(|s| s.id() != me.id())
            .map(|s| (s.position().distance(&here), s))
            .filter(|(distance, _)| *distance <= self.window.spawn_range)
            .collect();
        nearby.sort_by(|a, b| a.0.total_cmp(&b.0));
        ui.same_line();
        ui.text(format!("{} within range", nearby.len()));
        let targeted = mq.target().map(|t| t.id());
        ui.table("spawns", 4)
            .flags(TableFlags::ROW_BG | TableFlags::BORDERS | TableFlags::SCROLL_Y | TableFlags::RESIZABLE)
            .outer_size([0.0, 0.0])
            .show(|table| {
                table.setup_column_with("Name", TableColumnFlags::WIDTH_STRETCH, 0.0);
                table.setup_column_with("Level", TableColumnFlags::WIDTH_FIXED, 40.0);
                table.setup_column_with("Type", TableColumnFlags::WIDTH_FIXED, 60.0);
                table.setup_column_with("Distance", TableColumnFlags::WIDTH_FIXED, 70.0);
                table.setup_scroll_freeze(0, 1);
                table.headers_row();
                for (distance, spawn) in nearby.iter().take(MAX_LISTED_SPAWNS) {
                    table.next_row();
                    table.next_column();
                    let id = spawn.id();
                    let clicked = table.with_id(id.0, |ui| {
                        ui.selectable_with(&spawn.name(), targeted == Some(id), mq_emu::imgui::SelectableFlags::SPAN_ALL_COLUMNS, [0.0, 0.0])
                    });
                    if clicked {
                        mq.command(format!("/target id {}", id.0));
                    }
                    table.item_tooltip_text("Click to target");
                    table.next_column();
                    table.text(spawn.level().to_string());
                    table.next_column();
                    table.text(match spawn.spawn_type() {
                        SpawnType::Player => "player".to_owned(),
                        SpawnType::Npc => "NPC".to_owned(),
                        SpawnType::Corpse => "corpse".to_owned(),
                        SpawnType::Unknown(n) => format!("type {n}"),
                    });
                    table.next_column();
                    table.text(format!("{distance:.1}"));
                }
            });
    }

    /// Every bound widget, to try by hand; values show what Rust received.
    fn widgets_tab(&mut self, ui: &mut Ui) {
        let s = &mut self.window.sampler;
        ui.text_wrapped("Every widget mq-emu binds. Values on the right are what the plugin received.");

        ui.separator_text("Buttons");
        if ui.button("Click me") {
            s.clicks += 1;
        }
        ui.item_tooltip_text("A plain button");
        ui.same_line();
        if ui.small_button("small") {
            s.clicks += 1;
        }
        ui.same_line();
        if ui.button_sized("sized", [80.0, 0.0]) {
            s.clicks += 1;
        }
        ui.same_line();
        ui.text(format!("{} clicks", s.clicks));
        if ui.button("Open popup") {
            ui.open_popup("sampler popup");
        }
        ui.popup("sampler popup", |ui| {
            ui.text("A popup");
            if ui.button("Close") {
                ui.close_current_popup();
            }
        });
        ui.same_line();
        if ui.text_link("A text link") {
            s.clicks += 1;
        }

        ui.separator_text("Values");
        ui.checkbox("Checkbox", &mut s.checked);
        ui.same_line();
        ui.text(format!("= {}", s.checked));
        ui.radio_value("One", &mut s.radio, 1);
        ui.same_line();
        ui.radio_value("Two", &mut s.radio, 2);
        ui.same_line();
        ui.radio_value("Three", &mut s.radio, 3);
        ui.same_line();
        ui.text(format!("= {}", s.radio));
        ui.slider("Fraction", &mut s.fraction, 0.0..=1.0);
        ui.slider("Count", &mut s.count, 0..=10);
        ui.drag_range("Speed", &mut s.speed, 0.05, 0.0..=10.0, Some("%.2f m/s"));
        ui.input_number("Count (typed)", &mut s.count);
        ui.combo_list("Combo", &mut s.choice, &["North", "East", "South", "West"]);
        ui.input_text("Name", &mut s.name);
        ui.same_line();
        ui.text(format!("{} bytes", s.name.len()));
        ui.input_text_multiline("Notes", &mut s.notes, [0.0, 50.0]);
        ui.color_edit("Color", &mut s.color);
        ui.text_colored(s.color, "Text in that color");

        ui.separator_text("Display");
        ui.progress_bar(s.fraction);
        // Animated, to show frames keep coming.
        let wave: Vec<f32> = (0..40).map(|i| ((ui.time() as f32) * 2.0 + i as f32 * 0.3).sin()).collect();
        ui.plot_lines("Wave", &wave);
        ui.plot_histogram("Bars", &[1.0, 3.0, 2.0, 5.0, 4.0]);
        ui.bullet_text("A bullet");
        ui.with_style(StyleVar::Alpha(0.5), |ui| ui.text("Half transparent"));

        ui.separator_text("Trees and lists");
        ui.tree_node("A tree node", |ui| {
            ui.text("Inside");
            ui.tree_node_with("A leaf", TreeNodeFlags::LEAF, |_| {});
        });
        if ui.collapsing_header("A collapsing header") {
            ui.indented(|ui| ui.text("Its contents"));
        }
        for index in 0..3 {
            let label = format!("Selectable {index}");
            if ui.selectable(&label, s.selected == Some(index)) {
                s.selected = Some(index);
            }
        }
        ui.group(|ui| {
            ui.text("A group,");
            ui.text("hover it");
        });
        ui.item_tooltip(|ui| {
            ui.text("A tooltip drawn with widgets:");
            ui.progress_bar_with(s.fraction, [120.0, 0.0], None);
        });
    }
}
