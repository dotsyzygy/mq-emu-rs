//! Chat events and key binds against MacroQuest: events matched on lines
//! `/echo` writes (captures, `${...}` data, item links), a key bind set by
//! `/bind`, saved to MacroQuest.ini as mq-emu writes combos and pressed, and
//! every binding read against MacroQuest's own `/dumpbinds`.

use std::collections::HashMap;

use mq_emu::{BindSlot, ChatSource, EventError, EventMatch, Ini, InvSlot, Key, KeyBindError, KeyBinding, KeyCombo, Mq, Task};

use crate::RustTest;
use crate::checks::{Check, Report};


/// The selftest's key bind, saved to MacroQuest.ini while it runs.
const BIND: &str = "RustTestBind";
const MARKER_EVENT: &str = "rusttest-marker-event";
const MARKER_ME: &str = "rusttest-marker-me";
const MARKER_LINK: &str = "rusttest-marker-link";
const EVENTS: [&str; 4] = ["rusttest-capture", "rusttest-me", "rusttest-link", "rusttest-raw-link"];
/// What `/dumpbinds` writes, in the config directory.
const DUMP: &str = "rusttest-binds";

/// Keeps what a selftest event matched, under the event's name.
fn record(name: &'static str) -> impl Fn(&mut RustTest, &Mq<RustTest>, &EventMatch) + 'static {
    move |plugin, _, matched| plugin.events_seen.push((name.to_owned(), matched.clone()))
}

fn matches<'a>(seen: &'a [(String, EventMatch)], name: &str) -> Vec<&'a EventMatch> {
    seen.iter().filter(|(n, _)| n == name).map(|(_, matched)| matched).collect()
}

/// Events on `/echo` lines; removes them after.
pub async fn events(task: &Task<RustTest>, report: &mut Report) {
    // Any item, to echo a link to.
    let item = task.with(|mq| {
        let inventory = mq.inventory()?;
        InvSlot::all().find_map(|slot| Some((slot.name()?.to_owned(), inventory.get(slot)?.name())))
    });
    task.with_plugin(|plugin, mq| {
        plugin.events_seen.clear();
        let added = [
            mq.add_event(EVENTS[0], &format!("#1# {MARKER_EVENT} #2#."), record("capture")),
            mq.add_event(EVENTS[1], &format!("|${{Me.CleanName}}| {MARKER_ME} #1#"), record("me")),
            mq.add_event(EVENTS[2], &format!("{MARKER_LINK} #1#"), record("link")),
            mq.add_event_with_links(EVENTS[3], &format!("{MARKER_LINK} #1#"), record("raw link")),
        ];
        report.print(mq, "events are added", Check::that(added.iter().all(Result::is_ok), || format!("{added:?}")));
        let refused = mq.add_event("rusttest-bad", "#|", record("bad"));
        report.print(mq, "a pattern MacroQuest's matcher would crash on is refused", Check::that(refused == Err(EventError::NoText), || {
            format!("{refused:?}")
        }));
        mq.command_now(format!("/echo alpha {MARKER_EVENT} beta gamma."));
        mq.command_now(format!("/echo ${{Me.CleanName}} {MARKER_ME} yes"));
        mq.command_now(format!("/echo Nobody {MARKER_ME} no"));
        if let Some((slot, _)) = &item {
            mq.command_now(format!("/echo {MARKER_LINK} ${{Me.Inventory[{slot}].ItemLink[CLICKABLE]}}"));
        }
    });
    task.next_pulse().await;

    task.with_plugin(|plugin, mq| {
        let seen = std::mem::take(&mut plugin.events_seen);
        let capture = matches(&seen, "capture");
        report.print(mq, "#1# and #2# capture from an /echo line", Check::that(
            capture.len() == 1
                && capture[0].arg(1) == Some("alpha")
                && capture[0].arg(2) == Some("beta gamma")
                && capture[0].source() == ChatSource::MacroQuest,
            || format!("{capture:?}"),
        ));
        let me = matches(&seen, "me");
        report.print(mq, "|${Me.CleanName}| matches your name and only it", Check::that(me.len() == 1 && me[0].arg(1) == Some("yes"), || {
            format!("{me:?}")
        }));
        match &item {
            None => {
                let reason = || Check::Skip("no item to link".into());
                report.print(mq, "item links match as the item's name", reason());
                report.print(mq, "events keeping links see the link", reason());
            }
            Some((_, name)) => {
                let link = matches(&seen, "link");
                report.print(mq, "item links match as the item's name", Check::that(
                    link.len() == 1 && link[0].arg(1) == Some(name.as_str()),
                    || format!("{name:?}: {link:?}"),
                ));
                let raw = matches(&seen, "raw link");
                report.print(mq, "events keeping links see the link", Check::that(
                    raw.len() == 1 && raw[0].arg(1).is_some_and(|text| text.contains('\x12') && text.contains(name.as_str())),
                    || format!("{name:?}: {raw:?}"),
                ));
            }
        }
        let removed = mq.remove_event(EVENTS[0]);
        report.print(mq, "remove_event says it removed the event", Check::that(removed, || "it says there was none".into()));
        mq.command_now(format!("/echo alpha {MARKER_EVENT} beta gamma."));
    });
    task.next_pulse().await;

    task.with_plugin(|plugin, mq| {
        let seen = std::mem::take(&mut plugin.events_seen);
        report.print(mq, "a removed event no longer matches", Check::that(matches(&seen, "capture").is_empty(), || format!("{seen:?}")));
        for name in EVENTS {
            mq.remove_event(name);
        }
    });
}

/// The selftest's key bind against `/bind` and MacroQuest.ini, and every
/// binding against `/dumpbinds`. Removes the bind and its saved keys after.
pub async fn key_binds(task: &Task<RustTest>, report: &mut Report) {
    let ctrl_shift_f11 = KeyCombo::new(Key::named("F11").expect("a named key")).with_ctrl().with_shift();
    let alt_num_5 = KeyCombo::new(Key::named("Num_5").expect("a named key")).with_alt();
    task.with_plugin(|plugin, mq| {
        plugin.bind_presses.clear();
        let added = mq.add_key_bind(BIND, |plugin: &mut RustTest, _, down| plugin.bind_presses.push(down));
        report.print(mq, "a key bind is added", Check::that(added.is_ok(), || format!("{added:?}")));
        let noop = |_: &mut RustTest, _: &Mq<RustTest>, _: bool| {};
        let taken = [mq.add_key_bind("ranged", noop), mq.add_key_bind("FORWARD", noop)];
        report.print(mq, "MacroQuest's RANGED bind and the game's forward command are taken", Check::that(
            taken == [Err(KeyBindError::Taken); 2],
            || format!("{taken:?}"),
        ));

        mq.command_now(format!("/bind {BIND} ctrl+shift+F11"));
        let bound = mq.key_bind(BIND);
        report.print(mq, "keys /bind sets read back", Check::that(
            bound == Some(KeyBinding { normal: ctrl_shift_f11, alternate: KeyCombo::CLEAR }),
            || format!("{bound:?}"),
        ));
        let set = mq.set_key_bind(BIND, BindSlot::Alternate, alt_num_5);
        let saved = Ini::new(mq.config_dir().join("MacroQuest.ini"))
            .map(|ini| (ini.get("Key Binds", &format!("{BIND}_Nrm")), ini.get("Key Binds", &format!("{BIND}_Alt"))));
        report.print(mq, "MacroQuest.ini has both as mq-emu writes them", Check::that(
            set.is_ok() && saved.as_ref().is_ok_and(|saved| *saved == (Some(ctrl_shift_f11.to_string()), Some(alt_num_5.to_string()))),

            || format!("set {set:?}, saved {saved:?}"),
        ));
        let pressed = mq.press_key_bind(BIND);
        report.print(mq, "press_key_bind finds the bind", Check::that(pressed, || "it says there's none".into()));
    });
    task.next_pulse().await;

    task.with_plugin(|plugin, mq| {
        let presses = std::mem::take(&mut plugin.bind_presses);
        report.print(mq, "a pressed bind's handler runs down, then up", Check::that(presses == [true, false], || format!("{presses:?}")));
        report.print(mq, "every binding reads as /dumpbinds writes it", against_dump(mq));

        let removed = mq.remove_key_bind(BIND);
        let gone = mq.key_bind(BIND);
        report.print(mq, "the bind is removed", Check::that(removed && gone.is_none(), || format!("removed {removed}, still {gone:?}")));
        if let Ok(ini) = Ini::new(mq.config_dir().join("MacroQuest.ini")) {
            let _ = ini.remove("Key Binds", &format!("{BIND}_Nrm"));
            let _ = ini.remove("Key Binds", &format!("{BIND}_Alt"));
        }
    });
}

/// Every `/bind` line `/dumpbinds` writes against the binding mq-emu reads.
fn against_dump(mq: &Mq<RustTest>) -> Check {
    let eq: HashMap<String, KeyBinding> = mq.eq_key_binds().into_iter().map(|(name, keys)| (name.to_ascii_lowercase(), keys)).collect();
    if eq.is_empty() {
        return Check::Fail("eq_key_binds() is empty".into());
    }
    // MacroQuest crashes describing a key it has no name for.
    let unnamed: Vec<_> = eq.iter().filter(|(_, keys)| [keys.normal, keys.alternate].iter().any(|c| c.key().is_some_and(|k| k.name().is_none()))).collect();
    if let Some((name, keys)) = unnamed.first() {
        return Check::Skip(format!("{name} is bound to a key MacroQuest has no name for ({keys:?}), which would crash /dumpbinds"));
    }
    mq.command_now(format!("/dumpbinds {DUMP}"));
    let path = mq.config_dir().join(format!("{DUMP}.cfg"));
    let text = std::fs::read_to_string(&path);
    let _ = std::fs::remove_file(&path);
    let text = match text {
        Ok(text) => text,
        Err(e) => return Check::Fail(format!("can't read {}: {e}", path.display())),
    };
    let (mut compared, mut wrong) = (0, Vec::new());
    for line in text.lines() {
        let Some((name, theirs)) = line.strip_prefix("/bind ").and_then(|rest| rest.split_once(' ')) else { continue };
        let (name, alternate) = match name.strip_prefix('~') {
            Some(name) => (name, true),
            None => (name, false),
        };
        let keys = eq.get(&name.to_ascii_lowercase()).copied().or_else(|| mq.key_bind(name));
        let ours = keys.map(|keys| if alternate { keys.alternate } else { keys.normal }.to_string());
        compared += 1;
        if ours.as_deref() != Some(theirs) {
            wrong.push(format!("{line} (mq-emu {ours:?})"));
        }
    }
    Check::that(compared > 0 && wrong.is_empty(), || {
        format!("{} of {compared} differ: {}", wrong.len(), wrong.iter().take(3).cloned().collect::<Vec<_>>().join("; "))
    })
}
