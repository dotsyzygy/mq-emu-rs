//! MQ2RustPluginTest: exercises mq-emu inside the real game.
//!
//! `/rusttest selftest` checks mq-emu's view of the game against MacroQuest's
//! own data (`${Me...}`, `${Target...}`) and exercises commands, re-entrancy,
//! parsing, keys, turning and async tasks. Other subcommands deliberately
//! trigger the runtime's safety nets (panics, slow pulses, feedback loops) or
//! move the character so movement can be watched.

#![allow(non_snake_case)] // the crate is named after the DLL

mod checks;

use std::time::{Duration, Instant};

use mq_emu::color::{GREEN, RED, RESET, TEAL, YELLOW};
use mq_emu::{MoveKey, Mq, Plugin, Spawn, Task, TaskId};

use crate::checks::{Check, Report};

const COMMAND: &str = "/rusttest";
/// Text the re-entrancy checks look for in chat.
const MARKER_CHAT: &str = "rusttest-marker-chat";
const MARKER_ECHO: &str = "rusttest-marker-echo";
/// The selftest's turn: lopsided on purpose, so clockwise and counter-clockwise
/// degrees can't agree by coincidence (they do at north and south).
const SELFTEST_TURN_DEGREES: f32 = 100.0;
/// Longest a smooth turn may take before the check fails.
const TURN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Default)]
struct RustTest {
    spawns_added: u64,
    spawns_removed: u64,
    chat_lines: u64,
    /// Sleep for this long in the next pulse (`/rusttest slow`).
    slow_next_pulse: Option<Duration>,
    /// Echo our own chat until the runtime's cap stops it (`/rusttest flood`).
    flooding: Option<mq_emu::Stats>,
    selftest: Option<TaskId>,
    walk: Option<TaskId>,
    seen_marker_chat: bool,
    seen_marker_echo: bool,
}

impl Plugin for RustTest {
    fn new(mq: &Mq<Self>) -> Self {
        if let Err(e) = mq.add_command(COMMAND, RustTest::command) {
            mq.chat(format!("{RED}[{}]{RESET} couldn't register {COMMAND}: {e}", mq.plugin_name()));
        }
        mq.chat(format!("{GREEN}[{}]{RESET} loaded. Try {YELLOW}{COMMAND} help{RESET}.", mq.plugin_name()));
        RustTest::default()
    }

    fn shutdown(&mut self, mq: &Mq<Self>) {
        // Tasks are dropped on unload; never leave the character moving.
        if self.walk.is_some() || self.selftest.is_some() {
            let _ = mq.stop_moving();
        }
        mq.chat(format!("[{}] unloading", mq.plugin_name()));
    }

    fn on_pulse(&mut self, mq: &Mq<Self>) {
        if let Some(delay) = self.slow_next_pulse.take() {
            std::thread::sleep(delay);
        }
        if let Some(before) = self.flooding.take() {
            let after = mq.stats();
            say(
                mq,
                &format!(
                    "flood stopped by the runtime: {} dropped, {} cap hit(s)",
                    after.dropped - before.dropped,
                    after.cap_hits - before.cap_hits
                ),
            );
        }
    }

    fn on_write_chat_color(&mut self, mq: &Mq<Self>, line: &str, _color: u32) {
        self.chat_lines += 1;
        self.seen_marker_chat |= line.contains(MARKER_CHAT);
        self.seen_marker_echo |= line.contains(MARKER_ECHO);
        if self.flooding.is_some() {
            // Deliberate feedback loop: every line we write comes back here.
            mq.chat("flood");
        }
    }

    fn on_add_spawn(&mut self, _mq: &Mq<Self>, _spawn: &Spawn<'_>) {
        self.spawns_added += 1;
    }

    fn on_remove_spawn(&mut self, _mq: &Mq<Self>, _spawn: &Spawn<'_>) {
        self.spawns_removed += 1;
    }
}

impl RustTest {
    fn command(&mut self, mq: &Mq<Self>, args: &str) {
        let mut words = args.split_whitespace();
        match words.next().unwrap_or("help") {
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
            "reset" => {
                mq.reset_stats();
                say(mq, "stats reset");
            }
            "me" => describe(mq, "me", mq.me()),
            "target" => describe(mq, "target", mq.target()),
            "selftest" => self.start_selftest(mq),
            "panic" => panic!("requested by {COMMAND} panic"),
            "slow" => {
                let ms = words.next().and_then(|w| w.parse().ok()).unwrap_or(50);
                self.slow_next_pulse = Some(Duration::from_millis(ms));
                say(mq, &format!("next pulse will take {ms} ms; expect a slow-pulse warning"));
            }
            "flood" => {
                self.flooding = Some(mq.stats());
                say(mq, "starting a chat feedback loop; the runtime should cap it");
            }
            "spawns" => {
                let n = words.next().and_then(|w| w.parse().ok()).unwrap_or(5);
                let Some(here) = mq.controlled().map(|me| me.position()) else {
                    say(mq, "no controlled spawn");
                    return;
                };
                let mut spawns: Vec<_> = mq.spawns().map(|s| (s.position().distance(&here), s)).collect();
                spawns.sort_by(|(a, _), (b, _)| a.total_cmp(b));
                say(mq, &format!("{} spawns in zone; nearest {n}:", spawns.len()));
                for (distance, spawn) in spawns.iter().take(n) {
                    say(mq, &format!("  {:>7.1}  {} (id {}, {:?}, level {})", distance, spawn.name(), spawn.id(), spawn.spawn_type(), spawn.level()));
                }
            }
            "find" => {
                let name = words.collect::<Vec<_>>().join(" ");
                match mq.spawn_by_name(&name) {
                    Some(spawn) => describe(mq, "found", Some(spawn)),
                    None => say(mq, &format!("no spawn named {name:?}")),
                }
            }
            "face" => match mq.target() {
                Some(target) => report_move(mq, "facing target", mq.face_point(&target.position())),
                None => say(mq, "target something first"),
            },
            "turn" => {
                let degrees: f32 = words.next().and_then(|w| w.parse().ok()).unwrap_or(90.0);
                match mq.controlled() {
                    Some(me) => report_move(mq, &format!("turning {degrees}° left"), mq.face(me.heading().turned_left(degrees))),
                    None => say(mq, "no controlled spawn"),
                }
            }
            "walk" => {
                let ms = words.next().and_then(|w| w.parse().ok()).unwrap_or(1000);
                if let Some(previous) = self.walk.take() {
                    mq.cancel_task(previous);
                }
                self.walk = Some(mq.spawn_fallible_task(move |task| walk(task, Duration::from_millis(ms))));
            }
            "stop" => {
                for id in [self.walk.take(), self.selftest.take()].into_iter().flatten() {
                    mq.cancel_task(id);
                }
                report_move(mq, "stopped", mq.stop_moving());
            }
            _ => {
                say(mq, &format!("{COMMAND} selftest | stats | reset | me | target"));
                say(mq, "  movement: face | turn [degrees, + = left] | walk [ms] | stop");
                say(mq, "  spawns: spawns [n] | find <name>");
                say(mq, "  safety nets: panic | slow [ms] | flood");
            }
        }
    }

    fn start_selftest(&mut self, mq: &Mq<Self>) {
        if self.selftest.is_some() {
            say(mq, "a selftest is already running");
            return;
        }
        let mut report = Report::new(mq);
        checks::game_data(mq, &mut report);
        checks::spawn_list(mq, &mut report);
        checks::commands(mq, &mut report);
        checks::parsing(mq, &mut report);
        checks::keys(mq, &mut report);

        // Re-entrancy can only be judged after this command returns.
        self.seen_marker_chat = false;
        self.seen_marker_echo = false;
        let stats_before = mq.stats();
        mq.chat(format!("[{}] {MARKER_CHAT}", mq.plugin_name()));
        mq.command_now(format!("/echo [{}] {MARKER_ECHO}", mq.plugin_name()));
        self.selftest = Some(mq.spawn_task(move |task| selftest_rest(task, report, stats_before)));
    }
}

/// `/rusttest walk`: walk forward for `duration`, then stop and report.
async fn walk(task: Task<RustTest>, duration: Duration) -> Result<(), mq_emu::MoveError> {
    let start = task.with(|mq| mq.controlled().map(|me| me.position())).ok_or(mq_emu::MoveError::NoPlayer)?;
    task.with(|mq| mq.set_key(MoveKey::Forward, true))?;
    task.with(|mq| say(mq, &format!("walking forward for {} ms", duration.as_millis())));
    task.delay(duration).await;
    task.with_plugin(|plugin, mq| {
        plugin.walk = None;
        let stopped = mq.stop_moving();
        let moved = mq.controlled().map(|me| me.position().distance(&start)).unwrap_or(0.0);
        let released = !mq.is_key_down(MoveKey::Forward);
        say(mq, &format!("walked {moved:.1} units; forward key released: {released}; {stopped:?}"));
    });
    Ok(())
}

/// The selftest's checks that span pulses, written as one task.
async fn selftest_rest(task: Task<RustTest>, mut report: Report, stats_before: mq_emu::Stats) {
    // The command that started us has returned; its queued chat has arrived.
    task.next_pulse().await;
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

    turning(&task, &mut report).await;

    task.with_plugin(|plugin, mq| {
        plugin.selftest = None;
        report.finish(mq);
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

fn say<P: Plugin>(mq: &Mq<P>, text: &str) {
    mq.chat(format!("{TEAL}[{}]{RESET} {text}", mq.plugin_name()));
}

fn report_move<P: Plugin>(mq: &Mq<P>, what: &str, result: Result<(), mq_emu::MoveError>) {
    match result {
        Ok(()) => say(mq, what),
        Err(e) => say(mq, &format!("{RED}couldn't: {e}{RESET}")),
    }
}

fn describe<P: Plugin>(mq: &Mq<P>, label: &str, spawn: Option<Spawn<'_>>) {
    let Some(spawn) = spawn else {
        say(mq, &format!("{label}: none"));
        return;
    };
    let pos = spawn.position();
    say(
        mq,
        &format!(
            "{label}: {} (id {}, level {}, {:?}) loc {:.2}, {:.2}, {:.2} heading {:.1}° hp {}/{} ({}%)",
            spawn.name(),
            spawn.id(),
            spawn.level(),
            spawn.spawn_type(),
            pos.y,
            pos.x,
            pos.z,
            spawn.heading().degrees(),
            spawn.hp_current(),
            spawn.hp_max(),
            spawn.hp_percent()
        ),
    );
}

mq_emu::plugin!(RustTest);
