//! Selftest checks for MacroQuest's data system: typed reads against `${...}`
//! text, and the plugin's own `RustTest` TLO as macros see it.

use std::time::Duration;

use mq_emu::{GameState, Mq, Plugin, Task, Value};

use crate::RustTest;
use crate::checks::{Check, Report, same_text};

/// The TLO this plugin provides (see `RustTest::tlo`).
pub const TLO: &str = "RustTest";
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
