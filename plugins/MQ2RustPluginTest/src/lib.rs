//! MQ2RustPluginTest: exercises mq-emu inside the real game.
//!
//! `/rusttest selftest` runs everything: mq-emu's view of the game against
//! MacroQuest's own data (`${Me...}`, `${Target...}`), commands, re-entrancy,
//! parsing, keys, async tasks, turning and walking, the runtime's safety nets
//! (a caught panic, a slow pulse, a capped feedback loop), messaging, Lua
//! scripts, and your other characters running the plugin.
//!
//! `/rusttest window` opens a debug window (ImGui) with the results, the
//! runtime's stats, the game as mq-emu sees it, and every bound widget.

#![allow(non_snake_case)] // the crate is named after the DLL

mod checks;
mod data_checks;
mod game_checks;
mod graphics;
mod lua_checks;
mod peer_checks;
mod play_checks;
mod window;

use std::collections::{BTreeSet, VecDeque};
use std::time::{Duration, Instant};

use mq_emu::color::{GREEN, RED, RESET, TEAL, YELLOW};
use mq_emu::{Address, LuaTable, LuaValue, Message, MoveKey, Mq, Plugin, Spawn, Task, TaskId, Ui, Value};

use crate::checks::{Check, Report, SharedLog};

const COMMAND: &str = "/rusttest";
/// Text the re-entrancy checks look for in chat.
const MARKER_CHAT: &str = "rusttest-marker-chat";
const MARKER_ECHO: &str = "rusttest-marker-echo";
/// What the selftest's alias echoes.
const MARKER_ALIAS: &str = "rusttest-marker-alias";
/// The selftest's turn: lopsided on purpose, so clockwise and counter-clockwise
/// degrees can't agree by coincidence (they do at north and south).
const SELFTEST_TURN_DEGREES: f32 = 100.0;
/// Longest a smooth turn may take before the check fails.
const TURN_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the selftest walks each way.
const WALK_TIME: Duration = Duration::from_millis(500);
/// How long the selftest's deliberately slow pulse takes.
const SLOW_PULSE: Duration = Duration::from_millis(50);
/// How many recent pulse times are kept to measure the frame interval.
const RECENT_PULSES: usize = 120;
/// How long the safety-net checks wait for the runtime to react.
const SAFETY_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Default)]
struct RustTest {
    spawns_added: u64,
    spawns_removed: u64,
    chat_lines: u64,
    /// Sleep for this long in the next pulse.
    slow_next_pulse: Option<Duration>,
    /// Panic in the next pulse.
    panic_next_pulse: bool,
    /// Echo our own chat until the runtime's cap stops it; cleared next pulse.
    flooding: bool,
    selftest: Option<TaskId>,
    seen_marker_chat: bool,
    seen_marker_echo: bool,
    /// The chat line the selftest's alias echoed.
    alias_echo: Option<String>,
    /// What the `selftest` mailbox last received.
    selftest_mailbox: Option<String>,
    /// The last "hello" from a Lua script, and the mailbox it came from.
    lua_hello: Option<(LuaValue, Option<String>)>,
    /// Characters that answered "who", while the selftest is asking.
    peers: Option<BTreeSet<String>>,
    /// Who last invited us to a raid, from the invite's chat line. The game
    /// shows it as a dialog; nothing in the raid or player state says so.
    raid_invite_from: Option<String>,
    /// Who last invited us to a group, the same way.
    group_invite_from: Option<String>,
    /// When the latest pulses started, oldest first.
    recent_pulses: VecDeque<Instant>,
    /// The selftest's `/echo` of the RustTest TLO, once it arrives.
    tlo_echo: Option<String>,
    /// The latest selftest's results, for the debug window.
    selftest_log: SharedLog,
    window: window::DebugWindow,
    /// The debug window's graphics tabs and the selftest's graphics exercise.
    graphics: graphics::Graphics,
}

impl Plugin for RustTest {
    fn new(mq: &Mq<Self>) -> Self {
        if let Err(e) = mq.add_command(COMMAND, RustTest::command) {
            mq.chat(format!("{RED}[{}]{RESET} couldn't register {COMMAND}: {e}", mq.plugin_name()));
        }
        // The selftest sends itself a message here through the real post office.
        if let Err(e) = mq.add_mailbox("selftest", |plugin: &mut RustTest, _, message| {
            plugin.selftest_mailbox = Some(message.text().into_owned());
        }) {
            mq.chat(format!("{RED}[{}]{RESET} couldn't add the selftest mailbox: {e}", mq.plugin_name()));
        }
        // Lua scripts reach us at "RustPluginTest:lua" (see lua/rustpeer.lua).
        if let Err(e) = mq.add_mailbox("lua", RustTest::lua_message) {
            mq.chat(format!("{RED}[{}]{RESET} couldn't add the lua mailbox: {e}", mq.plugin_name()));
        }
        // ${RustTest[...]} for macros and mq.TLO.RustTest(...) for Lua.
        if let Err(e) = mq.add_tlo(data_checks::TLO, RustTest::tlo) {
            mq.chat(format!("{RED}[{}]{RESET} couldn't add the {} TLO: {e}", mq.plugin_name(), data_checks::TLO));
        }
        mq.chat(format!("{GREEN}[{}]{RESET} loaded. Try {YELLOW}{COMMAND} selftest{RESET}.", mq.plugin_name()));
        RustTest::default()
    }

    fn shutdown(&mut self, mq: &Mq<Self>) {
        // Tasks are dropped on unload; never leave the character moving.
        if self.selftest.is_some() {
            let _ = mq.stop_moving();
        }
        mq.chat(format!("[{}] unloading", mq.plugin_name()));
    }

    fn on_pulse(&mut self, _mq: &Mq<Self>) {
        if self.recent_pulses.len() == RECENT_PULSES {
            self.recent_pulses.pop_front();
        }
        self.recent_pulses.push_back(Instant::now());
        self.flooding = false;
        if let Some(delay) = self.slow_next_pulse.take() {
            std::thread::sleep(delay);
        }
        if std::mem::take(&mut self.panic_next_pulse) {
            panic!("deliberate, from {COMMAND} selftest");
        }
    }

    fn on_incoming_chat(&mut self, _mq: &Mq<Self>, line: &str, _color: u32) -> bool {
        // "Syzygy invites you to join a raid." (what MQ2AutoAccept watches for)
        if let Some((name, _)) = line.split_once(" invites you to join a raid") {
            self.raid_invite_from = Some(name.trim().to_owned());
        } else if let Some((name, _)) = line.split_once(" invites you to join a group") {
            self.group_invite_from = Some(name.trim().to_owned());
        }
        false
    }

    fn on_write_chat_color(&mut self, mq: &Mq<Self>, line: &str, _color: u32) {
        self.chat_lines += 1;
        self.seen_marker_chat |= line.contains(MARKER_CHAT);
        self.seen_marker_echo |= line.contains(MARKER_ECHO);
        if line.contains(MARKER_ALIAS) {
            self.alias_echo = Some(line.to_owned());
        }
        if let Some(echo) = data_checks::note_echo(line) {
            self.tlo_echo = Some(echo);
        }
        if self.flooding {
            // Deliberate feedback loop: every line we write comes back here.
            mq.chat("flood");
        }
    }

    fn on_update_imgui(&mut self, mq: &Mq<Self>, ui: &mut Ui) {
        self.draw_window(mq, ui);
        self.draw_graphics_overlays(mq, ui);
    }

    fn on_add_spawn(&mut self, _mq: &Mq<Self>, _spawn: &Spawn<'_>) {
        self.spawns_added += 1;
    }

    fn on_remove_spawn(&mut self, _mq: &Mq<Self>, _spawn: &Spawn<'_>) {
        self.spawns_removed += 1;
    }

    /// Messages from this plugin on other characters (and on this one): "who"
    /// is answered with "here", "ping" with "pong from <name>", and
    /// "show:<text>" shows the text and answers "shown".
    fn on_message(&mut self, mq: &Mq<Self>, message: Message) {
        let from = message.sender().and_then(|a| a.character).unwrap_or_else(|| "?".into());
        let me = mq.me().map(|me| me.name()).unwrap_or_default();
        let text = message.text().into_owned();
        let answer = if text == "who" {
            if from != me {
                if let Err(e) = mq.send(&Address::character(from.clone()), "here") {
                    say(mq, &format!("{RED}couldn't answer {from}: {e}{RESET}"));
                }
            }
            return;
        } else if text == "here" {
            if let Some(peers) = &mut self.peers
                && from != me
            {
                peers.insert(from);
            }
            return;
        } else if text == "ping" {
            format!("pong from {me}")
        } else if text == "frame" {
            self.frame_ms().map_or_else(|| "unknown".to_owned(), |ms| format!("{ms:.2}"))
        } else if let Some(line) = text.strip_prefix("show:") {
            say(mq, &format!("{YELLOW}{from}{RESET}: {line}"));
            "shown".to_owned()
        } else if text == "group:leave" {
            // Another character's selftest is forming a raid with us; the
            // server won't deliver its invite while we're in its group.
            if mq.group().is_some() {
                mq.command_now("/disband");
                "left".to_owned()
            } else {
                "not grouped".to_owned()
            }
        } else if text == "group:accept" {
            // ...and invites us back afterwards.
            if self.group_invite_from.take().is_some_and(|name| name.eq_ignore_ascii_case(&from)) {
                mq.command_now("/invite");
                "accepted".to_owned()
            } else {
                "no invite".to_owned()
            }
        } else if text == "raid:accept" {
            // Another character's selftest invited us to a raid.
            if mq.raid().is_some() {
                "accepted".to_owned()
            } else if self.raid_invite_from.take().is_some_and(|name| name.eq_ignore_ascii_case(&from)) {
                mq.command_now("/raidaccept");
                "accepted".to_owned()
            } else {
                "no invite".to_owned()
            }
        } else if text == "raid:leave" {
            // Whatever raid() says: leaving must work even if reading the raid doesn't.
            mq.command_now("/raiddisband");
            "left".to_owned()
        } else {
            say(mq, &format!("message from {from}: {text:?}"));
            return;
        };
        if let Err(e) = mq.reply(&message, answer, 0) {
            say(mq, &format!("{RED}couldn't answer {from}: {e}{RESET}"));
        }
    }
}

impl RustTest {
    /// The `RustTest` TLO: `${RustTest}` is the plugin's name; indexes give
    /// one value of each kind; anything else is NULL.
    fn tlo(&mut self, mq: &Mq<Self>, index: &str) -> Option<Value> {
        match index {
            "" => Some(mq.plugin_name().into()),
            "answer" => Some(42.into()),
            "half" => Some(2.5.into()),
            "yes" => Some(true.into()),
            "big" => Some(Value::Int(1 << 40)),
            "pulses" => Some(Value::Int(mq.stats().pulses as i64)),
            _ => None,
        }
    }

    /// The average time between recent pulses, in milliseconds.
    fn frame_ms(&self) -> Option<f64> {
        let (first, last) = (self.recent_pulses.front()?, self.recent_pulses.back()?);
        let gaps = self.recent_pulses.len().checked_sub(1).filter(|&n| n > 0)?;
        Some((*last - *first).as_secs_f64() * 1000.0 / gaps as f64)
    }

    /// Messages from Lua scripts, as Lua tables with an `id`.
    fn lua_message(&mut self, mq: &Mq<Self>, message: Message) {
        let sender = message.sender().and_then(|a| a.mailbox);
        let value = match message.lua() {
            Ok(value) => value,
            Err(e) => {
                say(mq, &format!("{RED}unreadable Lua message from {sender:?}: {e}{RESET}"));
                return;
            }
        };
        let reply = match value.get("id").and_then(LuaValue::as_str) {
            Some("hello") => {
                let token = value.get("token").cloned().unwrap_or_default();
                self.lua_hello = Some((value, sender));
                LuaTable::new().with("id", "hello_back").with("token", token)
            }
            _ => {
                say(mq, &format!("Lua message from {sender:?}: {value:?}"));
                LuaTable::new().with("error", "unknown request")
            }
        };
        if let Err(e) = mq.reply(&message, LuaValue::from(reply).encode(), 0) {
            say(mq, &format!("{RED}couldn't answer Lua: {e}{RESET}"));
        }
    }

    fn command(&mut self, mq: &Mq<Self>, args: &str) {
        match args.split_whitespace().next().unwrap_or("help") {
            "selftest" => self.start_selftest(mq),
            "window" => self.window.open = !self.window.open,
            "stats" => {
                for line in mq.stats().summary() {
                    say(mq, &line);
                }
                say(
                    mq,
                    &format!(
                        "seen: {} spawns added, {} removed, {} chat lines; {} task(s) running",
                        self.spawns_added,
                        self.spawns_removed,
                        self.chat_lines,
                        mq.task_count()
                    ),
                );
            }
            "stop" => {
                if let Some(id) = self.selftest.take() {
                    mq.cancel_task(id);
                }
                match mq.stop_moving() {
                    Ok(()) => say(mq, "stopped"),
                    Err(e) => say(mq, &format!("{RED}couldn't stop: {e}{RESET}")),
                }
            }
            _ => {
                say(mq, &format!("{COMMAND} selftest | stats | stop | window"));
                say(mq, &format!("  for the Lua checks: /lua run {} (here and on your other characters)", lua_checks::PEER_SCRIPT));
            }
        }
    }

    fn start_selftest(&mut self, mq: &Mq<Self>) {
        if self.selftest.is_some() {
            say(mq, "a selftest is already running");
            return;
        }
        let mut report = Report::new(mq, self.selftest_log.clone());
        report.section(mq, "Targeting");
        let targeted = checks::targeting(mq, &mut report);
        report.section(mq, "Game data");
        checks::game_data(mq, &mut report);
        report.section(mq, "Items, spells, group and zone");
        game_checks::run(mq, &mut report);
        report.section(mq, "Pet buffs, raid, mercenary, AAs, skills, merchant and loot");
        play_checks::run(mq, &mut report);
        report.section(mq, "Spawn list");
        checks::spawn_list(mq, &mut report);
        if let Some(id) = targeted {
            checks::clear_target(mq, &mut report, id);
        }
        report.section(mq, "Commands and parsing");
        checks::commands(mq, &mut report);
        checks::parsing(mq, &mut report);
        report.section(mq, "Movement keys");
        checks::keys(mq, &mut report);
        report.section(mq, "Reading TLOs");
        data_checks::reads(mq, &mut report);

        // Re-entrancy can only be judged after this command returns.
        self.seen_marker_chat = false;
        self.seen_marker_echo = false;
        let stats_before = mq.stats();
        mq.chat(format!("[{}] {MARKER_CHAT}", mq.plugin_name()));
        mq.command_now(format!("/echo [{}] {MARKER_ECHO}", mq.plugin_name()));
        self.selftest = Some(mq.spawn_task(move |task| selftest_rest(task, report, stats_before)));
    }
}

/// The selftest's checks that span pulses, written as one task.
async fn selftest_rest(task: Task<RustTest>, mut report: Report, stats_before: mq_emu::Stats) {
    // The command that started us has returned; its queued chat has arrived.
    task.next_pulse().await;
    task.with(|mq| report.section(mq, "Re-entrancy and tasks"));
    task.with_plugin(|plugin, mq| {
        let after = mq.stats();
        report.print(
            mq,
            "chat written inside a command is delivered after it (queued)",
            Check::that(plugin.seen_marker_chat && after.queued > stats_before.queued, || {
                format!("seen: {}, queued before {} after {}", plugin.seen_marker_chat, stats_before.queued, after.queued)
            }),
        );
        report.print(
            mq,
            "command_now output comes back through on_write_chat_color",
            Check::that(plugin.seen_marker_echo, || "the /echo line never arrived".into()),
        );
    });

    task.with_plugin(|plugin, mq| {
        report.section(mq, "Aliases");
        plugin.alias_echo = None;
        checks::aliases(mq, &mut report, &format!("[{}] {MARKER_ALIAS}", mq.plugin_name()), "with arguments");
    });
    task.next_pulse().await;
    task.with_plugin(|plugin, mq| checks::alias_expanded(mq, &mut report, plugin.alias_echo.as_deref(), "with arguments"));

    // Async tasks themselves.
    let pulses_before = task.with(|mq| mq.stats().pulses);
    task.next_pulse().await;
    let pulses_after = task.with(|mq| mq.stats().pulses);
    task.with(|mq| {
        report.print(mq, "next_pulse resumes on the very next pulse", Check::that(pulses_after == pulses_before + 1, || {
            format!("pulses went {pulses_before} -> {pulses_after}")
        }))
    });
    let started = Instant::now();
    task.delay(Duration::from_millis(500)).await;
    let waited = started.elapsed();
    task.with(|mq| {
        // At least the delay; at most a few frames more.
        report.print(mq, "delay(500 ms) waits about 500 ms", Check::that(
            waited >= Duration::from_millis(500) && waited < Duration::from_millis(750),
            || format!("waited {waited:?}"),
        ))
    });

    task.with(|mq| report.section(mq, "Target buffs"));
    play_checks::target_buffs(&task, &mut report).await;

    task.with(|mq| report.section(mq, "ImGui"));
    imgui(&task, &mut report).await;
    task.with(|mq| report.section(mq, "Providing a TLO"));
    data_checks::provided_tlo(&task, &mut report).await;
    task.with(|mq| report.section(mq, "Turning and walking"));
    turning(&task, &mut report).await;
    walking(&task, &mut report).await;
    task.with(|mq| report.section(mq, "Safety nets"));
    safety_nets(&task, &mut report).await;
    task.with(|mq| report.section(mq, "Messaging"));
    messaging(&task, &mut report).await;
    task.with(|mq| report.section(mq, "Lua"));
    let lua_here = lua_checks::run(&task, &mut report).await;
    task.with(|mq| report.section(mq, "Other characters"));
    let peers = peer_checks::run(&task, &mut report, lua_here).await;
    task.with(|mq| report.section(mq, "Raid"));
    play_checks::raid_with_others(&task, &mut report, &peers).await;

    task.with_plugin(|plugin, mq| {
        plugin.selftest = None;
        report.finish(mq);
    });
}

/// ImGui frames reach the plugin, and the debug window draws without errors:
/// it's opened for this (and left open, showing the results).
async fn imgui(task: &Task<RustTest>, report: &mut Report) {
    let frames_before = task.with_plugin(|plugin, _| plugin.window.frames);
    let pulses = 30;
    for _ in 0..pulses {
        task.next_pulse().await;
    }
    let frames = task.with_plugin(|plugin, _| plugin.window.frames) - frames_before;
    if frames == 0 {
        task.with(|mq| {
            for label in ["on_update_imgui is called every frame", "the debug window draws without ImGui errors"] {
                report.print(mq, label, Check::Skip("no ImGui frames arrived (is the overlay hidden?)".into()));
            }
        });
        return;
    }
    task.with(|mq| {
        // Pulses and ImGui frames both come once per game frame.
        report.print(mq, "on_update_imgui is called every frame", Check::that(frames + 2 >= pulses, || {
            format!("{frames} ImGui frames in {pulses} pulses")
        }))
    });

    let (drawn_before, errors_before) = task.with_plugin(|plugin, mq| {
        plugin.window.open = true;
        // A collapsed window doesn't draw its contents.
        plugin.window.expand = true;
        (plugin.window.drawn, mq.stats().imgui_errors)
    });
    for _ in 0..5 {
        task.next_pulse().await;
    }
    task.with_plugin(|plugin, mq| {
        let drawn = plugin.window.drawn - drawn_before;
        let errors = mq.stats().imgui_errors - errors_before;
        report.print(mq, "the debug window draws without ImGui errors", Check::that(drawn > 0 && errors == 0, || {
            format!("drawn {drawn} times in 5 pulses, {errors} ImGui errors")
        }));
    });
    graphics_exercise(task, report).await;
}

/// Frames the graphics exercise draws.
const EXERCISE_FRAMES: u32 = 10;

/// Draws every graphics feature (ImPlot and its closures, draw lists,
/// textures, the game's icons, fonts, drag and drop, docking and the dock
/// builder, multi-select, spell gems and EQ text) in a window for a few
/// frames, then checks nothing went wrong and that the game's resources were
/// there.
async fn graphics_exercise(task: &Task<RustTest>, report: &mut Report) {
    let errors_before = task.with_plugin(|plugin, mq| {
        plugin.graphics.exercise = Some(graphics::Exercise::new(EXERCISE_FRAMES));
        mq.stats().imgui_errors
    });
    let started = Instant::now();
    let mut done = false;
    while !done && started.elapsed() < Duration::from_secs(5) {
        task.next_pulse().await;
        done = task.with_plugin(|plugin, _| plugin.graphics.exercise.as_ref().is_some_and(|e| e.frames_left == 0));
    }
    let (exercise, errors, in_game) = task.with_plugin(|plugin, mq| {
        let exercise = plugin.graphics.exercise.take().unwrap_or_default();
        (exercise, mq.stats().imgui_errors - errors_before, mq.game_state() == mq_emu::GameState::InGame)
    });
    task.with(|mq| {
        let label = "ImPlot, draw lists, images, fonts, drag and drop, multi-select and EQ widgets draw without ImGui errors";
        if !done {
            report.print(mq, label, Check::Skip(format!("only {} ImGui frames arrived (is the overlay hidden?)", exercise.frames_drawn)));
            return;
        }
        report.print(mq, label, Check::that(errors == 0 && exercise.plotted, || {
            format!("{errors} ImGui errors in {} frames; plotted: {}", exercise.frames_drawn, exercise.plotted)
        }));
        report.print(mq, "an image file loads as a texture of its size", Check::that(exercise.texture_size == Some([16.0, 16.0]), || {
            format!("texture size {:?}", exercise.texture_size)
        }));
        report.print(mq, "MacroQuest's default, console and large fonts are available", Check::that(exercise.mq_fonts, || {
            "one of them is missing".into()
        }));
        let ui_checks = [
            ("the game's item icon sheet (A_DragItem) is found, as a grid", exercise.item_icon_cell),
            ("the game's spell icon sheet (A_SpellIcons) is found, as a grid", exercise.spell_icon_cell),
        ];
        for (label, cell) in ui_checks {
            let check = match cell {
                _ if !in_game => Check::Skip("needs you in game".into()),
                Some(_) => Check::Pass,
                None => Check::Fail("not found, or not a grid".into()),
            };
            report.print(mq, label, check);
        }
        let eq_fonts = match &exercise.eq_fonts {
            _ if !in_game => Check::Skip("needs you in game".into()),
            fonts if fonts.is_empty() => Check::Fail("none of styles 0 to 9".into()),
            _ => Check::Pass,
        };
        report.print(mq, "EverQuest's fonts are available in ImGui", eq_fonts);
        let docked = match exercise.docked {
            None => Check::Skip("docking is off in MacroQuest's overlay settings".into()),
            Some(docked) => Check::that(docked, || "the window wasn't in the dock space the builder made".into()),
        };
        report.print(mq, "the dock builder docks a window where it says", docked);
        let closures = exercise.getter_points > 0 && exercise.formatted && exercise.scaled;
        report.print(mq, "ImPlot calls item closures, axis formatters and custom scales", Check::that(closures, || {
            format!("{} points asked for; formatted: {}; scaled: {}", exercise.getter_points, exercise.formatted, exercise.scaled)
        }));
        let gems = match exercise.spell_gems {
            _ if !in_game => Check::Skip("needs you in game".into()),
            0 => Check::Fail("the game's spell gem window has no gems".into()),
            _ => Check::Pass,
        };
        report.print(mq, "spell gems draw as the game draws them", gems);
        let clipped = exercise.clipped_lines;
        report.print(mq, "the list clipper draws only the lines in view", Check::that((1..100).contains(&clipped), || {
            format!("{clipped} of {} lines drawn in a frame", graphics::CLIPPED_LINES)
        }));
        report.print(mq, "a custom widget lays out, takes the mouse and draws with ImGui's internals", Check::that(exercise.custom_item, || {
            "its item was never in view".into()
        }));
    });
}

/// The runtime's safety nets, each triggered on purpose: expect a caught-panic
/// message and a slow-pulse warning in chat.
async fn safety_nets(task: &Task<RustTest>, report: &mut Report) {
    // A panic in a callback is caught, counted, and the plugin keeps running.
    let before = task.with(|mq| mq.stats());
    task.with_plugin(|plugin, mq| {
        say(mq, "next: a deliberate panic, a slow pulse and a chat feedback loop");
        plugin.panic_next_pulse = true;
    });
    let caught = task.wait_until_timeout(|mq| mq.stats().panics > before.panics, SAFETY_TIMEOUT).await;
    task.next_pulse().await;
    task.with(|mq| {
        report.print(mq, "a panic in a callback is caught and the plugin keeps running", Check::that(caught, || {
            "the panic count never went up".into()
        }))
    });

    // A slow pulse is measured.
    let before = task.with(|mq| mq.stats());
    task.with_plugin(|plugin, _| plugin.slow_next_pulse = Some(SLOW_PULSE));
    let measured = task
        .wait_until_timeout(|mq| mq.stats().pulse_time.saturating_sub(before.pulse_time) >= SLOW_PULSE, SAFETY_TIMEOUT)
        .await;
    task.with(|mq| {
        let stats = mq.stats();
        report.print(mq, "a slow pulse is measured", Check::that(measured && stats.slowest_pulse >= SLOW_PULSE, || {
            format!("pulse time grew by {:?}; slowest {:?}", stats.pulse_time.saturating_sub(before.pulse_time), stats.slowest_pulse)
        }))
    });

    // Chat that answers its own chat is capped, not endless.
    let before = task.with(|mq| mq.stats());
    task.with_plugin(|plugin, mq| {
        plugin.flooding = true;
        mq.chat("flood");
    });
    task.next_pulse().await;
    task.next_pulse().await;
    task.with(|mq| {
        let after = mq.stats();
        report.print(mq, "a chat feedback loop is capped", Check::that(after.cap_hits > before.cap_hits && after.dropped > before.dropped, || {
            format!("cap hits {} -> {}, dropped {} -> {}", before.cap_hits, after.cap_hits, before.dropped, after.dropped)
        }));
        // These problems were the point; don't warn about them later.
        mq.dismiss_pending_warnings();
    });
}

/// Messaging through the real post office, to this same character.
async fn messaging(task: &Task<RustTest>, report: &mut Report) {
    let Some(me) = task.with(|mq| mq.me().map(|me| me.name())) else {
        task.with(|mq| report.print(mq, "messaging", Check::Skip("needs to be in game".into())));
        return;
    };
    let myself = Address::character(me.clone());

    // A request to our own default mailbox, answered by on_message.
    let reply = task.request_timeout(&myself, "ping", Duration::from_secs(5)).await;
    task.with(|mq| {
        report.print(mq, "a request to yourself is answered (round trip through the launcher)", match &reply {
            Ok(reply) => checks::same_text(reply.text(), format!("pong from {me}")),
            Err(e) => Check::Fail(e.to_string()),
        })
    });

    // A plain message to our own named mailbox.
    task.with_plugin(|plugin, _| plugin.selftest_mailbox = None);
    let sent = task.with(|mq| mq.send(&myself.clone().mailbox("selftest"), "hello mailbox"));
    let deadline = Instant::now() + Duration::from_secs(5);
    let received = loop {
        let received = task.with_plugin(|plugin, _| plugin.selftest_mailbox.take());
        if received.is_some() || sent.is_err() || Instant::now() >= deadline {
            break received;
        }
        task.next_pulse().await;
    };
    task.with(|mq| {
        report.print(mq, "a message reaches a named mailbox", match (&sent, &received) {
            (Err(e), _) => Check::Fail(format!("send failed: {e}")),
            (Ok(()), Some(text)) => checks::same_text(text.as_str(), "hello mailbox".to_owned()),
            (Ok(()), None) => Check::Fail("nothing arrived".into()),
        })
    });

    // Nobody there: the request must fail, not hang.
    let nobody = task.request_timeout(&Address::character("Nobodyxyzzy"), "ping", Duration::from_secs(3)).await;
    task.with(|mq| {
        report.print(mq, "a request to a missing character fails cleanly", match nobody {
            Err(_) => Check::Pass,
            Ok(reply) => Check::Fail(format!("got a reply: {:?}", reply.text())),
        })
    });
}

/// Turns to a lopsided heading, checks it against MacroQuest, and turns back.
async fn turning(task: &Task<RustTest>, report: &mut Report) {
    let start = task.with(|mq| match mq.controlled() {
        Some(me) if mq.game_state() == mq_emu::GameState::InGame => Some(me.heading()),
        _ => None,
    });
    let Some(original) = start else {
        task.with(|mq| report.print(mq, "turning", Check::Skip("needs to be in game".into())));
        return;
    };
    let goal = original.turned_left(SELFTEST_TURN_DEGREES);

    for (label, target) in [("the turn ends on its goal", goal), ("turning back ends on the original heading", original)] {
        let started = task.with(|mq| mq.face(target));
        task.with(|mq| {
            report.print(mq, &format!("a smooth turn starts ({label})"), Check::that(started.is_ok() && mq.is_turning(), || format!("{started:?}")))
        });
        let finished = task.wait_until_timeout(|mq| !mq.is_turning(), TURN_TIMEOUT).await;
        task.with(|mq| {
            let heading = mq.controlled().map(|me| me.heading());
            report.print(mq, label, match heading {
                // MacroQuest snaps the last step, so it should land within a unit.
                Some(h) if finished => Check::that(h.difference(target) <= 0.75, || {
                    format!("at {:.2}°, wanted {:.2}°", h.degrees(), target.degrees())
                }),
                Some(_) => Check::Fail(format!("still turning after {TURN_TIMEOUT:?}")),
                None => Check::Fail("no controlled spawn".into()),
            });
            if target == goal {
                // Now at a lopsided heading: compare with MacroQuest's views of it.
                checks::heading_matches(mq, report, "after turning");
            }
        });
    }
}

/// Walks forward briefly, then back, releasing each key.
async fn walking(task: &Task<RustTest>, report: &mut Report) {
    if task.with(|mq| mq.game_state() != mq_emu::GameState::InGame || mq.controlled().is_none()) {
        task.with(|mq| report.print(mq, "walking", Check::Skip("needs to be in game".into())));
        return;
    }
    for (label, key) in [("walking forward moves you", MoveKey::Forward), ("walking back moves you", MoveKey::Back)] {
        let start = task.with(|mq| mq.controlled().map(|me| me.position()));
        let pressed = task.with(|mq| mq.set_key(key, true));
        task.delay(WALK_TIME).await;
        let stopped = task.with(|mq| mq.stop_moving());
        task.with(|mq| {
            let moved = match (start, mq.controlled()) {
                (Some(start), Some(me)) => me.position().distance(&start),
                _ => 0.0,
            };
            let released = !mq.is_key_down(key);
            report.print(mq, label, Check::that(pressed.is_ok() && stopped.is_ok() && moved > 1.0 && released, || {
                format!("moved {moved:.1}; key released: {released}; press {pressed:?}, stop {stopped:?}")
            }))
        });
    }
}

fn say<P: Plugin>(mq: &Mq<P>, text: &str) {
    mq.chat(format!("{TEAL}[{}]{RESET} {text}", mq.plugin_name()));
}

mq_emu::plugin!(RustTest);
