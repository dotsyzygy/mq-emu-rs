//! MQ2RustPluginTest: exercises mq-emu inside the real game.
//!
//! `/rusttest selftest` checks mq-emu's view of the game against MacroQuest's
//! own data (`${Me...}`, `${Target...}`) and exercises commands, re-entrancy,
//! parsing, keys and turning. Other subcommands deliberately trigger the
//! runtime's safety nets (panics, slow pulses, feedback loops) or move the
//! character so movement can be watched.

#![allow(non_snake_case)] // the crate is named after the DLL

mod checks;

use std::time::{Duration, Instant};

use mq_emu::color::{GREEN, RED, RESET, TEAL, YELLOW};
use mq_emu::{Heading, MoveKey, Mq, Plugin, Position, Spawn};

use crate::checks::{Check, Report};

const COMMAND: &str = "/rusttest";
/// Text the deferred re-entrancy checks look for in chat.
const MARKER_CHAT: &str = "rusttest-marker-chat";
const MARKER_ECHO: &str = "rusttest-marker-echo";
/// The selftest's turn: lopsided on purpose, so clockwise and counter-clockwise
/// degrees can't agree by coincidence (they do at north and south).
const SELFTEST_TURN_DEGREES: f32 = 100.0;
/// Longest a smooth turn may take before the check fails.
const TURN_TIMEOUT: Duration = Duration::from_secs(10);

/// The parts of the selftest that span several pulses.
enum Phase {
    /// Judge re-entrancy, which needs the selftest command to have returned.
    Reentrancy { stats_before: mq_emu::Stats },
    /// Wait for the smooth turn to `goal`, then turn back to `original`.
    Turning { goal: Heading, original: Heading, returning: bool, deadline: Instant },
}

struct Selftest {
    report: Report,
    phase: Phase,
}

/// `/rusttest walk`: walk forward until `until`, then stop and report.
struct Walk {
    until: Instant,
    start: Position,
}

#[derive(Default)]
struct RustTest {
    spawns_added: u64,
    spawns_removed: u64,
    chat_lines: u64,
    /// Sleep for this long in the next pulse (`/rusttest slow`).
    slow_next_pulse: Option<Duration>,
    /// Echo our own chat until the runtime's cap stops it (`/rusttest flood`).
    flooding: Option<mq_emu::Stats>,
    selftest: Option<Selftest>,
    walk: Option<Walk>,
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
        // Never leave the character running when the plugin goes away.
        if self.walk.take().is_some() || self.selftest.take().is_some() {
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
        self.advance_walk(mq);
        self.advance_selftest(mq);
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
                    &format!("seen: {} spawns added, {} removed, {} chat lines", self.spawns_added, self.spawns_removed, self.chat_lines),
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
                let Some(start) = mq.controlled().map(|me| me.position()) else {
                    say(mq, "no controlled spawn");
                    return;
                };
                if mq.set_key(MoveKey::Forward, true).is_ok() {
                    self.walk = Some(Walk { until: Instant::now() + Duration::from_millis(ms), start });
                    say(mq, &format!("walking forward for {ms} ms"));
                } else {
                    say(mq, "can't move outside the game");
                }
            }
            "stop" => {
                self.walk = None;
                report_move(mq, "stopped", mq.stop_moving());
            }
            _ => {
                say(mq, &format!("{COMMAND} selftest | stats | reset | me | target"));
                say(mq, "  movement: face | turn [degrees, + = left] | walk [ms] | stop");
                say(mq, "  safety nets: panic | slow [ms] | flood");
            }
        }
    }

    fn advance_walk(&mut self, mq: &Mq<Self>) {
        let Some(walk) = self.walk.take_if(|walk| Instant::now() >= walk.until) else { return };
        let stopped = mq.stop_moving();
        let moved = mq.controlled().map(|me| me.position().distance(&walk.start)).unwrap_or(0.0);
        let still_held = mq.is_key_down(MoveKey::Forward);
        say(mq, &format!("walked {moved:.1} units; forward key released: {}; {stopped:?}", !still_held));
    }

    fn start_selftest(&mut self, mq: &Mq<Self>) {
        if self.selftest.is_some() {
            say(mq, "a selftest is already running");
            return;
        }
        let mut report = Report::new(mq);
        checks::game_data(mq, &mut report);
        checks::commands(mq, &mut report);
        checks::parsing(mq, &mut report);
        checks::keys(mq, &mut report);

        // Re-entrancy can only be judged after this command returns.
        self.seen_marker_chat = false;
        self.seen_marker_echo = false;
        let stats_before = mq.stats();
        mq.chat(format!("[{}] {MARKER_CHAT}", mq.plugin_name()));
        mq.command_now(format!("/echo [{}] {MARKER_ECHO}", mq.plugin_name()));
        self.selftest = Some(Selftest { report, phase: Phase::Reentrancy { stats_before } });
    }

    fn advance_selftest(&mut self, mq: &Mq<Self>) {
        let Some(mut test) = self.selftest.take() else { return };
        match test.phase {
            Phase::Reentrancy { stats_before } => {
                let after = mq.stats();
                test.report.print(
                    mq,
                    "chat written inside a command is delivered after it (queued)",
                    Check::that(self.seen_marker_chat && after.queued > stats_before.queued, || {
                        format!("seen: {}, queued before {} after {}", self.seen_marker_chat, stats_before.queued, after.queued)
                    }),
                );
                test.report.print(
                    mq,
                    "command_now output comes back through on_write_chat_color",
                    Check::that(self.seen_marker_echo, || "the /echo line never arrived".into()),
                );

                match mq.controlled() {
                    Some(me) if mq.game_state() == mq_emu::GameState::InGame => {
                        let original = me.heading();
                        let goal = original.turned_left(SELFTEST_TURN_DEGREES);
                        let started = mq.face(goal);
                        test.report.print(mq, "a smooth turn starts", Check::that(started.is_ok() && mq.is_turning(), || format!("{started:?}")));
                        test.phase = Phase::Turning { goal, original, returning: false, deadline: Instant::now() + TURN_TIMEOUT };
                        self.selftest = Some(test);
                    }
                    _ => {
                        test.report.print(mq, "turning", Check::Skip("needs to be in game".into()));
                        test.report.finish(mq);
                    }
                }
            }
            Phase::Turning { goal, original, returning, deadline } => {
                let timed_out = Instant::now() >= deadline;
                if mq.is_turning() && !timed_out {
                    self.selftest = Some(test); // keep waiting
                    return;
                }
                let target = if returning { original } else { goal };
                let heading = mq.controlled().map(|me| me.heading());
                let label = if returning { "turning back ends on the original heading" } else { "the turn ends on its goal" };
                test.report.print(
                    mq,
                    label,
                    match heading {
                        // MacroQuest snaps the last step, so it should land within a unit.
                        Some(h) if !timed_out => Check::that(h.difference(target) <= 0.75, || {
                            format!("at {:.2}°, wanted {:.2}°", h.degrees(), target.degrees())
                        }),
                        Some(_) => Check::Fail(format!("still turning after {TURN_TIMEOUT:?}")),
                        None => Check::Fail("no controlled spawn".into()),
                    },
                );
                if returning {
                    test.report.finish(mq);
                } else {
                    // Now at a lopsided heading: compare with MacroQuest's views of it.
                    checks::heading_matches(mq, &mut test.report, "after turning");
                    mq.stop_turning();
                    let _ = mq.face(original);
                    test.phase = Phase::Turning { goal, original, returning: true, deadline: Instant::now() + TURN_TIMEOUT };
                    self.selftest = Some(test);
                }
            }
        }
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
