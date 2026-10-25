//! The `/rusttest selftest` checks. Each compares something mq-emu reads or
//! does with what MacroQuest itself reports.

use mq_emu::color::{GREEN, RED, RESET, TEAL, YELLOW};
use std::ffi::{CStr, CString};
use std::time::Instant;

use mq_emu::{AliasError, CommandError, GameState, MoveKey, Mq, Plugin, SpawnId, angle_difference};

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

/// Pass/fail/skip counts.
#[derive(Default, Clone, Copy)]
struct Counts {
    passed: u32,
    failed: u32,
    skipped: u32,
}

impl Counts {
    fn add(&mut self, other: Counts) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.skipped += other.skipped;
    }

    /// "12 passed, 1 failed, 2 skipped", leaving out zero failures and skips.
    fn summary(self) -> String {
        let mut parts = vec![format!("{} passed", self.passed)];
        if self.failed > 0 {
            parts.push(format!("{RED}{} failed{RESET}", self.failed));
        }
        if self.skipped > 0 {
            parts.push(format!("{YELLOW}{} skipped{RESET}", self.skipped));
        }
        parts.join(", ")
    }
}

/// Collects check results, in titled sections, and prints them as they come.
pub struct Report {
    name: &'static str,
    /// Each section's title and counts, in order. Checks before the first
    /// section go in an untitled one.
    sections: Vec<(String, Counts)>,
    started: Instant,
    /// When the previous line was printed.
    last: Instant,
}

impl Report {
    pub fn new<P: Plugin>(mq: &Mq<P>) -> Report {
        let now = Instant::now();
        Report { name: mq.plugin_name(), sections: Vec::new(), started: now, last: now }
    }

    /// Starts a titled group of checks, with a header line.
    pub fn section<P: Plugin>(&mut self, mq: &Mq<P>, title: &str) {
        self.sections.push((title.to_owned(), Counts::default()));
        mq.chat(format!("[{}] {TEAL}===== {}. {title} ====={RESET}", self.name, self.sections.len()));
    }

    /// A plain line in the report, not counted as a check.
    pub fn note<P: Plugin>(&mut self, mq: &Mq<P>, text: &str) {
        mq.chat(format!("[{}] {text}", self.name));
    }

    fn counts(&mut self) -> &mut Counts {
        if self.sections.is_empty() {
            self.sections.push((String::new(), Counts::default()));
        }
        &mut self.sections.last_mut().expect("just ensured").1
    }

    pub fn print<P: Plugin>(&mut self, mq: &Mq<P>, label: &str, check: Check) {
        let line = match check {
            Check::Pass => {
                self.counts().passed += 1;
                format!("{GREEN}[PASS]{RESET} {label}")
            }
            Check::Fail(detail) => {
                self.counts().failed += 1;
                format!("{RED}[FAIL]{RESET} {label}: {detail}")
            }
            Check::Skip(reason) => {
                self.counts().skipped += 1;
                format!("{YELLOW}[SKIP]{RESET} {label}: {reason}")
            }
        };
        // Time since the selftest started, and since the previous line.
        let now = Instant::now();
        let stamp = format!("{:.2}s +{:.2}s", (now - self.started).as_secs_f64(), (now - self.last).as_secs_f64());
        self.last = now;
        mq.chat(format!("[{}] [{stamp}] {line}", self.name));
    }

    /// Prints each section's counts, then the totals.
    pub fn finish<P: Plugin>(self, mq: &Mq<P>) {
        mq.chat(format!("[{}] {TEAL}===== summary ====={RESET}", self.name));
        let mut total = Counts::default();
        for (number, (title, counts)) in self.sections.iter().enumerate() {
            if !title.is_empty() {
                mq.chat(format!("[{}]   {}. {title}: {}", self.name, number + 1, counts.summary()));
            }
            total.add(*counts);
        }
        let color = if total.failed == 0 { GREEN } else { RED };
        mq.chat(format!(
            "[{}] {color}selftest: {} passed, {} failed, {} skipped in {:.2}s{RESET}",
            self.name,
            total.passed,
            total.failed,
            total.skipped,
            self.started.elapsed().as_secs_f64()
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

pub fn same_text(ours: impl ToString, theirs: String) -> Check {
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

/// Targets the nearest spawn if nothing is targeted, so the target checks
/// always run. Returns its id if this did the targeting (to clear it after).
pub fn targeting<P: Plugin>(mq: &Mq<P>, report: &mut Report) -> Option<SpawnId> {
    let (Some(me), GameState::InGame) = (mq.me(), mq.game_state()) else {
        report.print(mq, "targeting", Check::Skip("needs to be in game".into()));
        return None;
    };
    if let Some(target) = mq.target() {
        report.note(mq, &format!("using your target, {} (id {})", target.name(), target.id()));
        return None;
    }
    let Some(nearest) = mq.nearest(|spawn| spawn.id() != me.id()) else {
        report.print(mq, "targeting the nearest spawn", Check::Skip("no spawns to target, so the target checks are skipped".into()));
        return None;
    };
    let (id, name) = (nearest.id(), nearest.name());
    mq.command_now(format!("/target id {id}"));
    let targeted = mq.target().map(|target| target.id());
    report.print(mq, &format!("/target id {id} targets the nearest spawn, {name}"), Check::that(targeted == Some(id), || {
        format!("target() is {targeted:?}")
    }));
    report.print(mq, "${Target.ID} agrees", same_text(id, mq.parse("${Target.ID}")));
    (targeted == Some(id)).then_some(id)
}

/// Clears a target that [`targeting`] set, leaving things as they were.
pub fn clear_target<P: Plugin>(mq: &Mq<P>, report: &mut Report, id: SpawnId) {
    mq.command_now("/target clear");
    let left = mq.target().map(|target| target.id());
    report.print(mq, &format!("/target clear clears the selftest's target ({id})"), Check::that(left.is_none(), || {
        format!("target() is still {left:?}")
    }));
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

/// The temporary alias the selftest runs (its expansion is checked a pulse
/// later, once the `/echo` it runs has come back as chat).
pub const ALIAS: &str = "/rusttestalias";

/// Alias management against MacroQuest's alias list, `${Alias}` and
/// MacroQuest.ini. Leaves [`ALIAS`] in place, run with `arguments`, for
/// [`alias_expanded`].
pub fn aliases<P: Plugin>(mq: &Mq<P>, report: &mut Report, echo: &str, arguments: &str) {
    let added = mq.add_alias(ALIAS, &format!("/echo {echo}"));
    let visible = mq.is_alias(&ALIAS.to_uppercase());
    report.print(mq, "a temporary alias can be added (names are case-insensitive)", Check::that(added.is_ok() && visible, || {
        format!("added {added:?}, is_alias in capitals {visible}")
    }));
    report.print(mq, "${Alias} agrees", same_text("TRUE", mq.parse(&format!("${{Alias[{ALIAS}]}}"))));
    report.print(mq, "adding it twice is refused", match mq.add_alias(ALIAS, "/echo again") {
        Err(AliasError::Rejected) => Check::Pass,
        other => Check::Fail(format!("got {other:?}")),
    });
    report.print(mq, "an alias can't shadow a plugin's command", match mq.add_alias("/rusttest", "/echo shadowed") {
        Err(AliasError::Rejected) => Check::Pass,
        other => Check::Fail(format!("got {other:?}")),
    });
    report.print(mq, "invalid alias names are refused", match mq.add_alias("no-slash", "/echo nope") {
        Err(AliasError::InvalidName) => Check::Pass,
        other => Check::Fail(format!("got {other:?}")),
    });

    // Persistent aliases go to MacroQuest.ini; removing one takes it out again.
    let saved = "/rusttestsaved";
    let ini = |mq: &Mq<P>| mq.parse(&format!("${{Ini[${{MacroQuest.Path[mqini]}},Aliases,{saved}]}}"));
    let added = mq.add_persistent_alias(saved, "/rusttest stats");
    let written = ini(mq);
    report.print(mq, "a persistent alias is written to MacroQuest.ini", Check::that(added.is_ok() && written == "/rusttest stats", || {
        format!("added {added:?}, ini has {written:?}")
    }));
    let removed = mq.remove_alias(saved);
    let left = ini(mq);
    report.print(mq, "removing it deletes it from MacroQuest.ini", Check::that(removed && !mq.is_alias(saved) && left == "NULL", || {
        format!("removed {removed}, still an alias {}, ini has {left:?}", mq.is_alias(saved))
    }));

    mq.command_now(format!("{ALIAS} {arguments}"));
}

/// [`aliases`]'s run of [`ALIAS`], judged from the chat it caused, then the
/// alias is removed.
pub fn alias_expanded<P: Plugin>(mq: &Mq<P>, report: &mut Report, echoed: Option<&str>, expected: &str) {
    report.print(mq, "running an alias runs its replacement, with the arguments", match echoed {
        Some(line) => Check::that(line.ends_with(expected), || format!("echoed {line:?}, wanted it to end {expected:?}")),
        None => Check::Fail("the alias's /echo never arrived".into()),
    });
    let removed = mq.remove_alias(ALIAS);
    report.print(mq, "the alias can be removed", Check::that(removed && !mq.is_alias(ALIAS), || {
        format!("removed {removed}, still an alias {}", mq.is_alias(ALIAS))
    }));
    report.print(mq, "${Alias} agrees it's gone", same_text("NULL", mq.parse(&format!("${{Alias[{ALIAS}]}}"))));
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
