//! The `/rusttest selftest` checks. Each compares something mq-emu reads or
//! does with what MacroQuest itself reports.

use mq_emu::color::{GREEN, RED, RESET, YELLOW};
use mq_emu::{CommandError, GameState, Mq, Plugin, angle_difference};

/// The outcome of one check.
pub enum Check {
    Pass,
    Fail(String),
    Skip(String),
}

impl Check {
    pub fn that(ok: bool, detail: impl FnOnce() -> String) -> Check {
        if ok { Check::Pass } else { Check::Fail(detail()) }
    }
}

/// Collects check results and prints them as they come.
pub struct Report {
    name: &'static str,
    passed: u32,
    failed: u32,
    skipped: u32,
    /// Deferred checks, printed by `finish` once an `Mq` is available again.
    pending: Vec<(String, Check)>,
}

impl Report {
    pub fn new<P: Plugin>(mq: &Mq<P>) -> Report {
        Report { name: mq.plugin_name(), passed: 0, failed: 0, skipped: 0, pending: Vec::new() }
    }

    pub fn print<P: Plugin>(&mut self, mq: &Mq<P>, label: &str, check: Check) {
        let line = match check {
            Check::Pass => {
                self.passed += 1;
                format!("{GREEN}[PASS]{RESET} {label}")
            }
            Check::Fail(detail) => {
                self.failed += 1;
                format!("{RED}[FAIL]{RESET} {label}: {detail}")
            }
            Check::Skip(reason) => {
                self.skipped += 1;
                format!("{YELLOW}[SKIP]{RESET} {label}: {reason}")
            }
        };
        mq.chat(format!("[{}] {line}", self.name));
    }

    /// Records a deferred check's result; [`Report::finish`] prints it.
    pub fn check(&mut self, label: &str, check: Check) {
        self.pending.push((label.to_owned(), check));
    }

    pub fn finish<P: Plugin>(mut self, mq: &Mq<P>) {
        for (label, check) in std::mem::take(&mut self.pending) {
            self.print(mq, &label, check);
        }
        let color = if self.failed == 0 { GREEN } else { RED };
        mq.chat(format!(
            "[{}] {color}selftest: {} passed, {} failed, {} skipped{RESET}",
            self.name, self.passed, self.failed, self.skipped
        ));
    }
}

/// Compares a float we read with MacroQuest's text for it (printed to 2 decimals).
fn same_float(ours: f32, theirs: &str) -> Check {
    match theirs.trim().parse::<f32>() {
        Ok(value) => Check::that((ours - value).abs() <= 0.011, || format!("mq-emu {ours:.3}, MacroQuest {theirs}")),
        Err(_) => Check::Fail(format!("MacroQuest returned {theirs:?}")),
    }
}

/// Compares two directions in degrees, treating whole turns as equal.
fn same_angle(ours: f32, theirs: &str) -> Check {
    match theirs.trim().parse::<f32>() {
        Ok(value) => Check::that(angle_difference(ours, value) <= 0.011, || format!("mq-emu {ours:.3}°, MacroQuest {theirs}°")),
        Err(_) => Check::Fail(format!("MacroQuest returned {theirs:?}")),
    }
}

fn same_text(ours: impl ToString, theirs: String) -> Check {
    let ours = ours.to_string();
    Check::that(ours == theirs, || format!("mq-emu {ours:?}, MacroQuest {theirs:?}"))
}

/// mq-emu's spawn data (from the generated layouts) against `${Me}`/`${Target}`.
pub fn game_data<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let state = mq.game_state();
    report.print(mq, "game state matches ${EverQuest.GameState}", match state {
        GameState::InGame => same_text("INGAME", mq.parse("${EverQuest.GameState}")),
        other => Check::Skip(format!("not in game ({other:?})")),
    });
    if state != GameState::InGame {
        report.print(mq, "spawn data", Check::Skip("needs to be in game".into()));
        return;
    }

    let Some(me) = mq.me() else {
        report.print(mq, "me() while in game", Check::Fail("no local player".into()));
        return;
    };
    report.print(mq, "me().id() = ${Me.ID}", same_text(me.id(), mq.parse("${Me.ID}")));
    report.print(mq, "me().level() = ${Me.Level}", same_text(me.level(), mq.parse("${Me.Level}")));
    report.print(mq, "me().name() = ${Me.DisplayName}", same_text(me.name(), mq.parse("${Me.DisplayName}")));
    let pos = me.position();
    report.print(mq, "me().position().x = ${Me.X}", same_float(pos.x, &mq.parse("${Me.X}")));
    report.print(mq, "me().position().y = ${Me.Y}", same_float(pos.y, &mq.parse("${Me.Y}")));
    report.print(mq, "me().position().z = ${Me.Z}", same_float(pos.z, &mq.parse("${Me.Z}")));
    // Compared as directions: a raw heading of 512 is 360° to MacroQuest and 0° to mq-emu.
    report.print(mq, "me().heading_degrees() = ${Me.Heading.Degrees}", same_angle(me.heading_degrees(), &mq.parse("${Me.Heading.Degrees}")));
    report.print(mq, "me().hp_current() = ${Me.CurrentHPs}", same_text(me.hp_current(), mq.parse("${Me.CurrentHPs}")));
    report.print(mq, "me().hp_max() = ${Me.MaxHPs}", same_text(me.hp_max(), mq.parse("${Me.MaxHPs}")));

    match mq.target() {
        Some(target) => {
            report.print(mq, "target().id() = ${Target.ID}", same_text(target.id(), mq.parse("${Target.ID}")));
            report.print(mq, "me().distance_to(target) = ${Target.Distance3D}", same_float(me.distance_to(&target), &mq.parse("${Target.Distance3D}")));
        }
        None => report.print(mq, "target checks", Check::Skip("no target (target something to include them)".into())),
    }
}

/// Command registration through the C++ shim, against MacroQuest's command list.
pub fn commands<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    report.print(mq, "/rusttest is registered", Check::that(mq.is_command("/rusttest"), || "is_command says no".into()));
    report.print(mq, "registering /rusttest twice is refused", match mq.add_command("/rusttest", |_: &mut P, _, _| {}) {
        Err(CommandError::AlreadyRegistered) => Check::Pass,
        other => Check::Fail(format!("got {other:?}")),
    });

    let temp = "/rusttesttemp";
    let added = mq.add_command(temp, |_: &mut P, _, _| {});
    let visible = mq.is_command(temp);
    let removed = mq.remove_command(temp);
    let gone = !mq.is_command(temp);
    report.print(mq, "a temporary command can be added and removed", Check::that(added.is_ok() && visible && removed && gone, || {
        format!("added {added:?}, visible {visible}, removed {removed}, gone afterwards {gone}")
    }));
    report.print(mq, "invalid command names are refused", match mq.add_command("no-slash", |_: &mut P, _, _| {}) {
        Err(CommandError::InvalidName) => Check::Pass,
        other => Check::Fail(format!("got {other:?}")),
    });
}

/// ParseMacroData, including a negative case.
pub fn parsing<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    report.print(mq, "parse expands math", same_text("4", mq.parse("${Math.Calc[2+2].Int}")));
    report.print(mq, "parse of unknown data gives NULL", same_text("NULL", mq.parse("${ThisDataDoesNotExist}")));
    report.print(mq, "text without data passes through", same_text("plain text", mq.parse("plain text")));
}
