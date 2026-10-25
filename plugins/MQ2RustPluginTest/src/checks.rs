//! The `/rusttest selftest` checks. Each compares something mq-emu reads or
//! does with what MacroQuest itself reports.

use mq_emu::color::{GREEN, RED, RESET, YELLOW};
use std::ffi::{CStr, CString};

use mq_emu::{CommandError, GameState, MoveKey, Mq, Plugin, angle_difference};

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
}

impl Report {
    pub fn new<P: Plugin>(mq: &Mq<P>) -> Report {
        Report { name: mq.plugin_name(), passed: 0, failed: 0, skipped: 0 }
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

    pub fn finish<P: Plugin>(self, mq: &Mq<P>) {
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
    heading_matches(mq, report, "as found");
    report.print(mq, "me().hp_current() = ${Me.CurrentHPs}", same_text(me.hp_current(), mq.parse("${Me.CurrentHPs}")));
    report.print(mq, "me().hp_max() = ${Me.MaxHPs}", same_text(me.hp_max(), mq.parse("${Me.MaxHPs}")));

    match mq.target() {
        Some(target) => {
            report.print(mq, "target().id() = ${Target.ID}", same_text(target.id(), mq.parse("${Target.ID}")));
            report.print(mq, "me().distance_to(target) = ${Target.Distance3D}", same_float(me.distance_to(&target), &mq.parse("${Target.Distance3D}")));
            let to = me.heading_to(&target);
            report.print(mq, "me().heading_to(target).degrees() = ${Target.HeadingTo.Degrees}", same_angle(to.degrees(), &mq.parse("${Target.HeadingTo.Degrees}")));
            report.print(mq, "me().heading_to(target).degrees_ccw() = ${Target.HeadingTo.DegreesCCW}", same_angle(to.degrees_ccw(), &mq.parse("${Target.HeadingTo.DegreesCCW}")));
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

/// The current heading both ways MacroQuest shows it. Only meaningful away from
/// north and south, where the two coincide; the selftest repeats it after a
/// lopsided turn.
pub fn heading_matches<P: Plugin>(mq: &Mq<P>, report: &mut Report, when: &str) {
    let Some(me) = mq.me() else {
        report.print(mq, &format!("heading ({when})"), Check::Skip("no local player".into()));
        return;
    };
    let heading = me.heading();
    report.print(mq, &format!("heading.degrees() = ${{Me.Heading.Degrees}} ({when})"), same_angle(heading.degrees(), &mq.parse("${Me.Heading.Degrees}")));
    report.print(mq, &format!("heading.degrees_ccw() = ${{Me.Heading.DegreesCCW}} ({when})"), same_angle(heading.degrees_ccw(), &mq.parse("${Me.Heading.DegreesCCW}")));
}

/// Movement key IDs against the game's own command table, and holding a key
/// against the game's key state.
pub fn keys<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    for key in MoveKey::ALL {
        let id = key.command();
        // SAFETY: the table is a static array of N_EQ_MAPPABLE_COMMANDS entries.
        let name = unsafe { mq_emu::sys::eqlib::szEQMappableCommands[id as usize] };
        let name = (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned());
        let wanted = CString::new(key.game_name()).unwrap();
        let found = unsafe { mq_emu::sys::mq2main::FindMappableCommand(wanted.as_ptr()) };
        report.print(
            mq,
            &format!("{key:?} is game command {id} \"{}\"", key.game_name()),
            Check::that(name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(key.game_name())) && found == id as i32, || {
                format!("the game calls command {id} {name:?}; FindMappableCommand(\"{}\") = {found}", key.game_name())
            }),
        );
    }

    if mq.game_state() != GameState::InGame {
        report.print(mq, "holding a key", Check::Skip("needs to be in game".into()));
        return;
    }
    let disabled: Vec<_> = MoveKey::HELD.into_iter().filter(|key| !mq.is_key_enabled(*key)).collect();
    report.print(mq, "the game allows the movement keys", Check::that(disabled.is_empty(), || format!("disabled: {disabled:?}")));
    if mq.is_key_down(MoveKey::Back) {
        report.print(mq, "holding a key", Check::Skip("the back key is already held".into()));
        return;
    }
    // Back, briefly: held and released within this call, before the game moves.
    let pressed = mq.set_key(MoveKey::Back, true);
    let held = mq.is_key_down(MoveKey::Back);
    let released = mq.set_key(MoveKey::Back, false);
    let held_after = mq.is_key_down(MoveKey::Back);
    report.print(mq, "a held key shows in the game's key state, and releases", Check::that(pressed.is_ok() && held && released.is_ok() && !held_after, || {
        format!("press {pressed:?}, held {held}, release {released:?}, held afterwards {held_after}")
    }));
}

/// The spawn list against MacroQuest's own spawn data.
pub fn spawn_list<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    if mq.game_state() != GameState::InGame {
        report.print(mq, "spawn list", Check::Skip("needs to be in game".into()));
        return;
    }
    let count = mq.spawns().count();
    report.print(mq, "spawns().count() = ${SpawnCount}", same_text(count, mq.parse("${SpawnCount}")));

    if let Some(me) = mq.me() {
        let id = me.id();
        report.print(mq, "you are in the spawn list", Check::that(mq.spawns().any(|s| s.id() == id), || format!("id {id} missing")));
        let name = me.name();
        report.print(mq, "spawn_by_name(your name) finds you", match mq.spawn_by_name(&name) {
            Some(found) => same_text(found.name(), name),
            None => Check::Fail("not found".into()),
        });
    }
    match mq.target() {
        Some(target) => report.print(mq, "spawn(target id) is the target", match mq.spawn(target.id()) {
            Some(found) => same_text(found.id(), target.id().to_string()),
            None => Check::Fail(format!("id {} not found", target.id())),
        }),
        None => report.print(mq, "spawn(target id)", Check::Skip("no target".into())),
    }
    report.print(mq, "spawn(nonexistent id) is None", Check::that(mq.spawn(mq_emu::SpawnId(u32::MAX)).is_none(), || "found one".into()));

    // A sample of spawns, each looked up through MacroQuest by ID.
    for spawn in mq.spawns().take(8) {
        let id = spawn.id();
        let name = mq.parse(&format!("${{Spawn[id {id}].DisplayName}}"));
        let x = mq.parse(&format!("${{Spawn[id {id}].X}}"));
        let y = mq.parse(&format!("${{Spawn[id {id}].Y}}"));
        let pos = spawn.position();
        let ok = name == spawn.name() && matches!(same_float(pos.x, &x), Check::Pass) && matches!(same_float(pos.y, &y), Check::Pass);
        report.print(mq, &format!("spawn {id} ({}) = ${{Spawn[id {id}]}}", spawn.name()), Check::that(ok, || {
            format!("mq-emu {:?} at {:.2}, {:.2}; MacroQuest {name:?} at {x}, {y}", spawn.name(), pos.x, pos.y)
        }));
    }
}
