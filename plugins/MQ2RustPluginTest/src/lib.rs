//! MQ2RustPluginTest: exercises mq-emu inside the real game.
//!
//! `/rusttest selftest` checks mq-emu's view of the game against MacroQuest's
//! own data (`${Me...}`, `${Target...}`) and exercises commands, re-entrancy
//! and parsing. Other subcommands deliberately trigger the runtime's safety
//! nets (panics, slow pulses, feedback loops) so they can be seen working.

#![allow(non_snake_case)] // the crate is named after the DLL

mod checks;

use std::time::Duration;

use mq_emu::color::{GREEN, RED, RESET, TEAL, YELLOW};
use mq_emu::{Mq, Plugin, Spawn};

use crate::checks::{Check, Report};

const COMMAND: &str = "/rusttest";
/// Text the deferred re-entrancy checks look for in chat.
const MARKER_CHAT: &str = "rusttest-marker-chat";
const MARKER_ECHO: &str = "rusttest-marker-echo";

#[derive(Default)]
struct RustTest {
    spawns_added: u64,
    spawns_removed: u64,
    chat_lines: u64,
    /// Sleep for this long in the next pulse (`/rusttest slow`).
    slow_next_pulse: Option<Duration>,
    /// Echo our own chat until the runtime's cap stops it (`/rusttest flood`).
    flooding: Option<mq_emu::Stats>,
    /// Checks finished on the next pulse, with the report they belong to.
    deferred: Option<(Report, mq_emu::Stats)>,
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
        if let Some((mut report, before)) = self.deferred.take() {
            let after = mq.stats();
            report.check(
                "chat written inside a command is delivered after it (queued)",
                Check::that(self.seen_marker_chat && after.queued > before.queued, || {
                    format!("seen: {}, queued before {} after {}", self.seen_marker_chat, before.queued, after.queued)
                }),
            );
            report.check(
                "command_now output comes back through on_write_chat_color",
                Check::that(self.seen_marker_echo, || "the /echo line never arrived".into()),
            );
            report.finish(mq);
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
                    &format!("seen: {} spawns added, {} removed, {} chat lines", self.spawns_added, self.spawns_removed, self.chat_lines),
                );
            }
            "reset" => {
                mq.reset_stats();
                say(mq, "stats reset");
            }
            "me" => describe(mq, "me", mq.me()),
            "target" => describe(mq, "target", mq.target()),
            "selftest" => self.selftest(mq),
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
            _ => {
                say(mq, &format!("{COMMAND} selftest | stats | reset | me | target"));
                say(mq, "  safety nets: panic | slow [ms] | flood");
            }
        }
    }

    fn selftest(&mut self, mq: &Mq<Self>) {
        let mut report = Report::new(mq);
        checks::game_data(mq, &mut report);
        checks::commands(mq, &mut report);
        checks::parsing(mq, &mut report);

        // Re-entrancy can only be judged after this command returns.
        self.seen_marker_chat = false;
        self.seen_marker_echo = false;
        let before = mq.stats();
        mq.chat(format!("[{}] {MARKER_CHAT}", mq.plugin_name()));
        mq.command_now(format!("/echo [{}] {MARKER_ECHO}", mq.plugin_name()));
        self.deferred = Some((report, before));
    }
}

fn say<P: Plugin>(mq: &Mq<P>, text: &str) {
    mq.chat(format!("{TEAL}[{}]{RESET} {text}", mq.plugin_name()));
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
            spawn.heading_degrees(),
            spawn.hp_current(),
            spawn.hp_max(),
            spawn.hp_percent()
        ),
    );
}

mq_emu::plugin!(RustTest);
