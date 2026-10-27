//! Chat events against the real matcher (the shim's Blech), with the fake
//! host's stand-ins for MacroQuest's chat cleaning and `${...}` data.

use super::*;
use crate::events::{EventError, EventMatch};
use crate::game_fns::fake::LINK_BODY;

fn incoming(rt: &'static Runtime<Recorder>, line: &str) {
    let line = CString::new(line).unwrap();
    unsafe { rt.incoming_chat(line.as_ptr(), 0) };
}

fn written(rt: &'static Runtime<Recorder>, line: &str) {
    let line = CString::new(line).unwrap();
    unsafe { rt.write_chat_color(line.as_ptr(), 0) };
}

/// Logs an event's name, source and captures.
fn log_match(name: &'static str) -> impl Fn(&mut Recorder, &Mq<Recorder>, &EventMatch) + 'static {
    move |plugin, _mq, matched| {
        let values: Vec<String> = matched.values().map(|(n, v)| format!("{n}={v}")).collect();
        plugin.log.push(format!("{name}:{:?}:{}", matched.source(), values.join(",")));
    }
}

fn with_events(add: impl Fn(&Mq<Recorder>) + 'static) -> &'static Runtime<Recorder> {
    let rt = runtime();
    set_hook(&ON_NEW, move |_, mq| add(mq));
    rt.initialize();
    rt
}

/// An item link as RoF2 writes it.
fn link(name: &str) -> String {
    format!("\x12{}{name}\x12", "0".repeat(LINK_BODY))
}

#[test]
fn events_run_after_the_chat_callback_that_received_the_line() {
    let rt = with_events(|mq| mq.add_event("tell", "#1# tells you, '#2#'", log_match("tell")).unwrap());
    incoming(rt, "Bob tells you, 'hi there'");
    assert_eq!(log_of(rt), ["new", "incoming:Bob tells you, 'hi there'", "tell:Game:1=Bob,2=hi there"]);
}

#[test]
fn written_chat_is_matched_too_and_says_so() {
    let rt = with_events(|mq| mq.add_event("echo", "[MQ] #*#", log_match("echo")).unwrap());
    written(rt, "[MQ] something");
    assert_eq!(log_of(rt), ["new", "chat:[MQ] something", "echo:MacroQuest:*=something"]);
}

#[test]
fn captures_are_read_by_number_or_name() {
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.add_event("slain", "You have slain #mob#! #1# #2#", |plugin: &mut Recorder, _mq, matched| {
            plugin.log.push(format!(
                "{:?} {:?} {:?} {:?} {}",
                matched.get("mob"),
                matched.arg(2),
                matched.arg(3),
                matched.get("nope"),
                matched.line()
            ));
        })
        .unwrap();
    });
    rt.initialize();
    incoming(rt, "You have slain a rat! one two");
    assert_eq!(log_of(rt).last().unwrap(), r#"Some("a rat") Some("two") None None You have slain a rat! one two"#);
}

#[test]
fn lines_are_matched_without_color_codes_and_with_links_as_their_names() {
    let rt = with_events(|mq| {
        mq.add_event("loot", "You looted #1#.", log_match("plain")).unwrap();
        mq.add_event_with_links("loot-links", "You looted #1#.", log_match("links")).unwrap();
    });
    let line = format!("\x07gYou looted \x07#ff0000{}.\x07x", link("Rusty Sword"));
    written(rt, &line);
    let log = log_of(rt);
    assert_eq!(log[2], "plain:MacroQuest:1=Rusty Sword");
    assert_eq!(log[3], format!("links:MacroQuest:1={}", link("Rusty Sword")));
}

#[test]
fn print_variables_are_macroquest_data_read_when_matching() {
    let rt = with_events(|mq| mq.add_event("hit", "|${Me.Name}| hits #1#.", log_match("hit")).unwrap());
    incoming(rt, "Tester hits a rat.");
    incoming(rt, "Somebody hits a rat.");
    assert_eq!(log_of(rt).iter().filter(|l| l.starts_with("hit:")).collect::<Vec<_>>(), ["hit:Game:1=a rat"]);
}

#[test]
fn every_matching_event_runs_in_the_order_added() {
    let rt = with_events(|mq| {
        mq.add_event_with_links("first", "#*#ping#*#", log_match("first")).unwrap();
        mq.add_event("second", "ping#*#", log_match("second")).unwrap();
        mq.add_event("third", "#*#pong", log_match("third")).unwrap();
        mq.add_event("fourth", "#*#", log_match("fourth")).unwrap();
    });
    incoming(rt, "ping!");
    let names: Vec<_> = log_of(rt).iter().skip(2).map(|l| l.split(':').next().unwrap().to_owned()).collect();
    assert_eq!(names, ["first", "second", "fourth"]);
}

#[test]
fn a_removed_events_waiting_matches_are_dropped() {
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.add_event("remover", "go", |plugin: &mut Recorder, mq, _| {
            plugin.log.push(format!("remover {} {}", mq.remove_event("removed"), mq.remove_event("removed")));
        })
        .unwrap();
        mq.add_event("removed", "go", log_match("removed")).unwrap();
    });
    rt.initialize();
    incoming(rt, "go");
    assert_eq!(log_of(rt), ["new", "incoming:go", "remover true false"]);
}

#[test]
fn a_handler_can_remove_its_own_event() {
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.add_event("once", "go", |plugin: &mut Recorder, mq, _| plugin.log.push(format!("once {}", mq.remove_event("once")))).unwrap();
    });
    rt.initialize();
    incoming(rt, "go");
    incoming(rt, "go");
    assert_eq!(log_of(rt), ["new", "incoming:go", "once true", "incoming:go"]);
}

#[test]
fn events_added_from_a_handler_see_the_next_line() {
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.add_event("adder", "add", |_: &mut Recorder, mq, _| mq.add_event("added", "#*#", log_match("added")).unwrap()).unwrap();
    });
    rt.initialize();
    incoming(rt, "add");
    incoming(rt, "later");
    assert_eq!(log_of(rt), ["new", "incoming:add", "incoming:later", "added:Game:*=later"]);
}

#[test]
fn bad_events_are_refused() {
    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| {
        let noop = |_: &mut Recorder, _: &Mq<Recorder>, _: &EventMatch| {};
        mq.add_event("taken", "x", noop).unwrap();
        let deep: String = std::iter::once("a".to_owned()).chain((0..60).map(|_| "#v#-".to_owned())).collect();
        let results = [
            mq.add_event("", "x", noop),
            mq.add_event("taken", "y", noop),
            mq.add_event("empty", "", noop),
            mq.add_event("bars", "#|", noop),
            mq.add_event("deep", &deep, noop),
            mq.add_event("vars", &"|a|".repeat(8), noop),
            mq.add_event("long", &"x".repeat(mq_emu_sys::constants::MAX_STRING), noop),
            // The same name with links is still the same name.
            mq.add_event_with_links("taken", "x", noop),
        ];
        plugin.log.push(format!("{results:?}"));
    });
    rt.initialize();
    let expected: [Result<(), EventError>; 8] = [
        Err(EventError::InvalidName),

        Err(EventError::AlreadyAdded),
        Err(EventError::NoText),
        Err(EventError::NoText),
        Err(EventError::TooDeep),
        Err(EventError::TooManyPrintVariables),
        Err(EventError::TooLong),
        Err(EventError::AlreadyAdded),
    ];
    assert_eq!(log_of(rt)[1], format!("{expected:?}"));
}

#[test]
fn a_panicking_event_handler_is_contained() {
    let rt = with_events(|mq| {
        mq.add_event("bad", "go", |_: &mut Recorder, _: &Mq<Recorder>, _: &EventMatch| panic!("bad event")).unwrap();
        mq.add_event("good", "go", log_match("good")).unwrap();
    });
    incoming(rt, "go");
    assert!(chat_lines().contains(&"[test] panicked in an event handler: bad event".to_owned()));
    assert_eq!(log_of(rt).last().unwrap(), "good:Game:");
}

#[test]
fn a_panicking_chat_callback_doesnt_lose_the_lines_events() {
    let rt = with_events(|mq| mq.add_event("seen", "seen", log_match("seen")).unwrap());
    set_hook(&ON_CHAT, |_, _| panic!("chat broke"));
    written(rt, "seen");
    assert_eq!(log_of(rt).last().unwrap(), "seen:MacroQuest:");
}

#[test]
fn an_event_feeding_itself_is_capped() {
    let rt = with_events(|mq| mq.add_event("echo", "loop", |_: &mut Recorder, mq, _| mq.chat("loop")).unwrap());
    written(rt, "loop");
    assert_eq!(rt.stats().cap_hits, 1);
    // Chat line and event alternate generations, so the cap allows half as many of each.
    let events = log_of(rt).iter().filter(|l| *l == "chat:loop").count();
    assert_eq!(events, MAX_NESTED_DELIVERIES / 2 + 1);
}

#[test]
fn shutdown_drops_every_event() {
    let rt = with_events(|mq| mq.add_event("any", "#*#", log_match("any")).unwrap());
    rt.shutdown();
    assert!(rt.events.borrow().slots.is_empty());
    assert!(rt.events.borrow().stripped.is_none());
}
