//! Selftest checks for MacroQuest's data system: typed reads against `${...}`
//! text, and the plugin's own `RustTest` TLO and `RustTestCounter` datatype
//! as macros see them.

use std::time::Duration;

use mq_emu::color::{RED, RESET};
use mq_emu::{DataType, DataTypeDef, GameState, Mq, Plugin, Task, Value};

use crate::RustTest;
use crate::checks::{Check, Report, same_text};

/// The TLO this plugin provides (see `RustTest::tlo`).
pub const TLO: &str = "RustTest";
/// The datatype this plugin provides; `${RustTest[counter N]}` is one.
pub const COUNTER_TYPE: &str = "RustTestCounter";
/// A datatype the selftest adds and removes.
const TEMP_TYPE: &str = "RustTestTemp";
/// Macro variables the selftest declares.
const VARIABLES: &[&str] = &["rusttest_v1", "rusttest_v2", "rusttest_v3", "rusttest_va", "rusttest_t", "rusttest_ta"];
/// Marks the `/echo` line that reads the TLO.
const MARKER: &str = "rusttest-tlo";
const TIMEOUT: Duration = Duration::from_secs(3);

/// Paths read both ways, with the kind of value each should be.
const PATHS: &[(&str, &str)] = &[
    ("Me.Name", "string"),
    ("Me.Level", "int"),
    ("Me.ID", "int"),
    ("Me.PctHPs", "int"),
    ("Me.CurrentMana", "int"),
    ("Me.X", "float"),
    ("Me.Heading.Degrees", "float"),
    ("Me.Grouped", "bool"),
    ("Me.Class.Name", "string"),
    ("Zone.ShortName", "string"),
    ("Me.Buff[1].Name", "string"),
];

fn kind_of(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::Object { .. } => "object",
        Value::Custom(_) => "custom",
    }
}

/// Typed reads, run inside the selftest command.
pub fn reads<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    if mq.game_state() != GameState::InGame {
        report.print(mq, "data", Check::Skip("needs to be in game".into()));
        return;
    }
    for (path, kind) in PATHS {
        let text = mq.parse(&format!("${{{path}}}"));
        let read = mq.data(path);
        report.print(mq, &format!("data({path:?}) matches ${{{path}}}"), match &read {
            Err(e) => Check::Fail(e.to_string()),
            Ok(None) => Check::that(text == "NULL", || format!("NULL here, MacroQuest says {text:?}")),
            Ok(Some(data)) => {
                let value = data.value();
                let same_value = match value {
                    // In game, MacroQuest's "%.2f" can round the last digit
                    // differently (Direct3D runs the FPU at single precision),
                    // so compare floats as numbers.
                    Value::Float(x) => text.parse::<f64>().is_ok_and(|shown| (x - shown).abs() <= 0.011),
                    _ => value.to_string() == text,
                };
                Check::that(kind_of(&value) == *kind && same_value && data.text() == text, || {
                    format!("{value:?} (shown {:?}), MacroQuest says {text:?}", data.text())
                })
            }
        });
    }

    // Typed values agree with mq-emu's own reading of the game.
    if let Some(me) = mq.me() {
        let level = mq.data("Me.Level").ok().flatten().and_then(|d| d.as_int());
        report.print(mq, "data(\"Me.Level\") as an int = me().level()", Check::that(level == Some(i64::from(me.level())), || {
            format!("{level:?} vs {}", me.level())
        }));
        let by_id = mq.tlo_at("Spawn", format!("id {}", me.id())).and_then(|s| s.member("CleanName")).and_then(|d| d.as_string());
        report.print(mq, "tlo_at(\"Spawn\", \"id <my id>\") finds me", same_text(me.name(), by_id.unwrap_or_default()));
    }

    // What isn't there is None, not a crash.
    report.print(mq, "a TLO that doesn't exist is None", Check::that(mq.tlo("NoSuchTloXyzzy").is_none(), || "got a value".into()));
    let missing = mq.data("Me.NoSuchMemberXyzzy");
    report.print(mq, "a member that doesn't exist is None", Check::that(matches!(missing, Ok(None)), || format!("{missing:?}")));
    let bad = mq.data("Me..Name");
    report.print(mq, "a malformed path is an error", Check::that(bad.is_err(), || format!("{bad:?}")));
}

/// The plugin's `RustTest` TLO: NULL from inside the plugin, its values from
/// a macro (a queued `/echo` runs outside the plugin).
pub async fn provided_tlo(task: &Task<RustTest>, report: &mut Report) {
    task.with(|mq| {
        // A task runs while the plugin is borrowed, so its own TLO can't answer.
        report.print(mq, "the plugin's own TLO reads as NULL from inside it", Check::that(mq.tlo(TLO).is_none(), || {
            "it answered while the plugin was busy".into()
        }));
        // That counts as a re-entrant event that couldn't be served; expected here.
        mq.dismiss_pending_warnings();
    });
    task.with_plugin(|plugin, mq| {
        plugin.tlo_echo = None;
        mq.command(format!("/echo {MARKER} ${{{TLO}}} ${{{TLO}[answer]}} ${{{TLO}[half]}} ${{{TLO}[yes]}} ${{{TLO}[big]}} ${{{TLO}[nope]}}"));
    });
    let arrived = wait_for_echo(task).await;
    task.with(|mq| {
        let expected = format!("{MARKER} {} 42 2.50 TRUE 1099511627776 NULL", mq.plugin_name());
        report.print(mq, &format!("macros read ${{{TLO}[...]}} (unknown index: NULL)"), match arrived {
            Some(line) => same_text(expected, line),
            None => Check::Fail("the /echo never arrived".into()),
        })
    });
}

/// Adds `RustTestCounter`: values keyed by an i32, whose members do
/// arithmetic on it, with a method that counts its calls.
pub fn add_counter_type(mq: &Mq<RustTest>) -> Option<DataType<i32>> {
    let def = DataTypeDef::new()
        .member("Value", |_, _, n: &i32, _| Some((*n).into()))
        .member("Label", |_, _, n, _| Some(format!("counter {n}").into()))
        .member("Plus", |_, _, n, index| Some(n.checked_add(index.parse().ok()?)?.into()))
        .member("Next", |plugin: &mut RustTest, _, n, _| Some(plugin.counter_type.as_ref()?.value(n.checked_add(1)?)))
        .method("Bump", |plugin, _, _, _| plugin.counter_bumps += 1)
        .text(|_, _, n| format!("#{n}"));
    match mq.add_type(COUNTER_TYPE, def) {
        Ok(ty) => Some(ty),
        Err(e) => {
            mq.chat(format!("{RED}[{}]{RESET} couldn't add the {COUNTER_TYPE} datatype: {e}", mq.plugin_name()));
            None
        }
    }
}

/// The plugin's `RustTestCounter` datatype as macros see it (from queued
/// commands, which run outside the plugin): members, methods and text, macro
/// variables of it set from text and from data, and a removed type's
/// variables turned into strings.
pub async fn provided_type(task: &Task<RustTest>, report: &mut Report) {
    let bumps_before = task.with_plugin(|plugin, mq| {
        plugin.tlo_echo = None;
        for command in [
            "/declare rusttest_v1 RustTestCounter global 7",
            "/declare rusttest_v2 RustTestCounter global",
            "/varset rusttest_v2 4",
            "/declare rusttest_v3 RustTestCounter global",
            "/vardata rusttest_v3 RustTest[counter 9]",
            "/declare rusttest_va[2] RustTestCounter global",
            "/varset rusttest_va[2] 5",
        ] {
            mq.command(command);
        }
        let five = format!("{TLO}[counter 5]");
        mq.command(format!(
            "/echo {MARKER} ${{{five}}} ${{{five}.Label}} ${{{five}.Plus[10]}} ${{{five}.Plus[x]}} ${{{five}.Next.Next.Value}} \
             ${{{five}.Bump}} ${{rusttest_v1.Value}} ${{rusttest_v2.Value}} ${{rusttest_v3.Value}} ${{rusttest_va[2].Value}} ${{rusttest_v2}}"
        ));
        plugin.counter_bumps
    });
    let read = wait_for_echo(task).await;
    task.with_plugin(|plugin, mq| {
        report.print(mq, &format!("macros read {COUNTER_TYPE}: members, a method, text, variables of it"), match read {
            Some(line) => same_text(format!("{MARKER} #5 counter 5 15 NULL 7 #5 7 4 9 5 #4"), line),
            None => Check::Fail("the /echo never arrived".into()),
        });
        report.print(mq, "a method runs once per read", Check::that(plugin.counter_bumps == bumps_before + 1, || {
            format!("ran {} times", plugin.counter_bumps - bumps_before)
        }));
    });

    // A removed type's variables become strings holding their keys.
    let added = task.with_plugin(|plugin, mq| {
        plugin.tlo_echo = None;
        let def = DataTypeDef::new().member("Key", |_: &mut RustTest, _, key: &String, _| Some(key.as_str().into()));
        let added = mq.add_type(TEMP_TYPE, def).map(|_| ());
        mq.command(format!("/declare rusttest_t {TEMP_TYPE} global Tank"));
        mq.command(format!("/declare rusttest_ta[2] {TEMP_TYPE} global"));
        mq.command(format!("/echo {MARKER} ${{rusttest_t.Key}}"));
        added
    });
    let declared = wait_for_echo(task).await;
    let removed = task.with_plugin(|plugin, mq| {
        plugin.tlo_echo = None;
        let removed = mq.remove_type(TEMP_TYPE);
        mq.command(format!("/echo {MARKER} ${{rusttest_t}} ${{rusttest_t.Length}} [${{rusttest_ta}}] ${{rusttest_va[2].Value}}"));
        removed
    });
    let after = wait_for_echo(task).await;
    task.with(|mq| {
        report.print(mq, "a removed datatype's variables become strings holding their keys", match (added, removed, declared, after) {
            (Err(e), ..) => Check::Fail(format!("couldn't add {TEMP_TYPE}: {e}")),
            (_, false, ..) => Check::Fail(format!("{TEMP_TYPE} wasn't removed")),
            (_, _, Some(declared), Some(after)) => {
                same_text(format!("{MARKER} Tank / {MARKER} Tank 4 [] 5"), format!("{declared} / {after}"))
            }
            _ => Check::Fail("an /echo never arrived".into()),
        });
        for variable in VARIABLES {
            mq.command(format!("/deletevar {variable}"));
        }
    });
}

async fn wait_for_echo(task: &Task<RustTest>) -> Option<String> {
    let deadline = std::time::Instant::now() + TIMEOUT;
    loop {
        if let Some(line) = task.with_plugin(|plugin, _| plugin.tlo_echo.take()) {
            return Some(line);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        task.next_pulse().await;
    }
}

/// Remembers the `/echo` line, if this is it.
pub fn note_echo(line: &str) -> Option<String> {
    line.find(MARKER).map(|at| line[at..].trim_end().to_owned())
}
