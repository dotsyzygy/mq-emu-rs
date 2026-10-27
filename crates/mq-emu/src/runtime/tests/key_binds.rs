//! Key binds against the fake MacroQuest key binds (`key_binds::fake`), which
//! follow its rules: names in any case, saved keys loaded when a bind is
//! added, and nothing removed unless asked, not even when a plugin unloads.

use std::ffi::c_char;

use super::*;
use crate::key_binds::{BindSlot, KeyBindError, KeyBinding, fake};
use crate::keys::{Key, KeyCombo};

fn combo(text: &str) -> KeyCombo {
    text.parse().unwrap()
}

fn log_bind(name: &'static str) -> impl Fn(&mut Recorder, &Mq<Recorder>, bool) + 'static {
    move |plugin, _mq, down| plugin.log.push(format!("{name}:{}", if down { "down" } else { "up" }))
}

fn with_binds(add: impl Fn(&mut Recorder, &Mq<Recorder>) + 'static) -> &'static Runtime<Recorder> {
    let rt = runtime();
    set_hook(&ON_NEW, add);
    rt.initialize();
    rt
}

/// A bind added by someone else (another plugin, or MacroQuest).
fn add_other(name: &str) {
    unsafe extern "C" fn other(_: *const c_char, _: bool) {}
    assert!((fake::API.add)(name, other));
}

#[test]
fn a_binds_keys_reach_its_handler_going_down_and_up() {
    let rt = with_binds(|_, mq| {
        mq.add_key_bind("RustBind", log_bind("bind")).unwrap();
        mq.set_key_bind("rustbind", BindSlot::Normal, combo("ctrl+F5")).unwrap();
        mq.set_key_bind("RUSTBIND", BindSlot::Alternate, combo("Num_5")).unwrap();
    });
    fake::key(combo("ctrl+F5"), true);
    fake::key(combo("ctrl+F5"), false);
    fake::key(combo("Num_5"), true);
    fake::key(combo("F5"), true);
    assert_eq!(log_of(rt), ["new", "bind:down", "bind:up", "bind:down"]);
}

#[test]
fn a_bind_starts_with_the_keys_saved_for_it_and_saves_new_ones() {
    fake::with(|binds| binds.saved.insert("rustbind_nrm".into(), combo("shift+A").to_raw()));
    let rt = with_binds(|plugin, mq| {
        mq.add_key_bind("RustBind", log_bind("bind")).unwrap();
        plugin.log.push(format!("{:?}", mq.key_bind("rustbind").map(|keys| keys.normal.to_string())));
        mq.set_key_bind("RustBind", BindSlot::Alternate, combo("B")).unwrap();
    });
    assert_eq!(log_of(rt)[1], r#"Some("shift+A")"#);
    assert_eq!(fake::with(|binds| binds.saved.get("rustbind_alt").copied()), Some(combo("B").to_raw()));
}

#[test]
fn any_binds_keys_can_be_read_and_set() {
    add_other("RANGED");
    let rt = with_binds(|plugin, mq| {
        let set = mq.set_key_bind("ranged", BindSlot::Normal, combo("alt+R"));
        let read = mq.key_bind("Ranged");
        let missing = (mq.key_bind("nope"), mq.set_key_bind("nope", BindSlot::Normal, combo("R")));
        plugin.log.push(format!("{set:?} {read:?} {missing:?}"));
    });
    let expected = (Ok::<(), KeyBindError>(()), Some(KeyBinding { normal: combo("alt+R"), alternate: KeyCombo::CLEAR }), (
        None::<KeyBinding>,
        Err::<(), _>(KeyBindError::NotFound),
    ));
    assert_eq!(log_of(rt)[1], format!("{:?} {:?} {:?}", expected.0, expected.1, expected.2));
}

#[test]
fn keys_macroquest_has_no_name_for_are_never_bound() {
    // MacroQuest saves a binding by its key's name, and has none for these.
    let unnamed = KeyCombo::new(Key::from_scan_code(0x56).unwrap()).with_ctrl();
    let rt = with_binds(move |plugin, mq| {
        mq.add_key_bind("RustBind", log_bind("bind")).unwrap();
        let results = [mq.set_key_bind("RustBind", BindSlot::Normal, unnamed), mq.set_eq_key_bind("jump", BindSlot::Normal, unnamed)];
        plugin.log.push(format!("{results:?}"));
    });
    assert_eq!(log_of(rt)[1], format!("{:?}", [Err::<(), _>(KeyBindError::UnnamedKey); 2]));
    assert_eq!(fake::with(|binds| binds.saved.len()), 0);
}

#[test]
fn pressing_a_bind_from_a_callback_runs_it_afterwards() {
    let rt = with_binds(|_, mq| mq.add_key_bind("RustBind", log_bind("bind")).unwrap());
    set_hook(&ON_PULSE, |plugin, mq| {
        let pressed = (mq.press_key_bind("rustbind"), mq.press_key_bind("nope"));
        plugin.log.push(format!("pressed {pressed:?}"));
    });
    rt.pulse();
    assert_eq!(log_of(rt), ["new", "pulse", "pressed (true, false)", "bind:down", "bind:up"]);
}

#[test]
fn holding_a_bind_only_presses_it() {
    let rt = with_binds(|_, mq| mq.add_key_bind("RustBind", log_bind("bind")).unwrap());
    set_hook(&ON_PULSE, |_, mq| assert!(mq.hold_key_bind("RustBind")));
    rt.pulse();
    assert_eq!(log_of(rt), ["new", "pulse", "bind:down"]);
}

#[test]
fn bad_names_and_taken_ones_are_refused() {
    add_other("OtherPlugin");
    let rt = with_binds(|plugin, mq| {
        let noop = |_: &mut Recorder, _: &Mq<Recorder>, _: bool| {};
        let mut results = Vec::new();
        for name in ["", "has space", "a=b", "[a]", "~alt", ";comment", "LIST", "eqlist", "clearcombo", "caf\u{e9}", "tab\t"] {
            results.push((name.to_owned(), mq.add_key_bind(name, noop)));
        }
        // Another plugin's, a game command's (which /bind would stop reaching),
        // and our own, in any case.
        mq.add_key_bind("Mine", noop).unwrap();
        for name in ["otherplugin", "FORWARD", "MINE"] {
            results.push((name.to_owned(), mq.add_key_bind(name, noop)));
        }
        plugin.log.push(format!("{results:?}"));
    });
    let log = log_of(rt);
    let results = &log[1];
    assert_eq!(results.matches("InvalidName").count(), 11, "{results}");
    assert_eq!(results.matches("Taken").count(), 3, "{results}");
    assert!(fake::has("Mine"));
}

#[test]
fn shutdown_removes_the_plugins_binds_and_only_those() {
    add_other("OtherPlugin");
    let rt = with_binds(|_, mq| {
        mq.add_key_bind("One", log_bind("one")).unwrap();
        mq.add_key_bind("Two", log_bind("two")).unwrap();
    });
    rt.shutdown();
    assert!(!fake::has("One") && !fake::has("Two"), "MacroQuest would keep calling into the unloaded plugin");
    assert!(fake::has("OtherPlugin"));
}

#[test]
fn removing_a_bind_only_removes_the_plugins_own() {
    add_other("OtherPlugin");
    let rt = with_binds(|plugin, mq| {
        mq.add_key_bind("Mine", log_bind("mine")).unwrap();
        let removed = (mq.remove_key_bind("MINE"), mq.remove_key_bind("Mine"), mq.remove_key_bind("OtherPlugin"));
        plugin.log.push(format!("{removed:?}"));
        // The name is free again.
        mq.add_key_bind("Mine", log_bind("again")).unwrap();
    });
    assert_eq!(log_of(rt)[1], "(true, false, false)");
    assert!(fake::has("OtherPlugin"));
    assert!(fake::has("Mine"));
}

#[test]
fn a_call_after_shutdown_reaches_nothing() {
    // As if MacroQuest still had a bind (it kept it, or a press was on its way).
    let rt = with_binds(|_, mq| mq.add_key_bind("Late", log_bind("late")).unwrap());
    rt.shutdown();
    unsafe { crate::key_binds::trampoline(c"Late".as_ptr(), true) };
    assert!(!log_of(rt).iter().any(|l| l.starts_with("late")));
}

#[test]
fn a_panicking_bind_handler_is_contained() {
    let rt = with_binds(|_, mq| {
        mq.add_key_bind("Bad", |_: &mut Recorder, _: &Mq<Recorder>, _: bool| panic!("bad bind")).unwrap();
        mq.set_key_bind("Bad", BindSlot::Normal, combo("F9")).unwrap();
    });
    fake::key(combo("F9"), true);
    assert!(chat_lines().contains(&"[test] panicked in a key bind handler: bad bind".to_owned()));
    rt.pulse();
    assert_eq!(log_of(rt).last().unwrap(), "pulse");
}

#[test]
fn the_games_own_bindings_are_read_and_set_by_command_name() {
    let rt = with_binds(|plugin, mq| {
        let before = mq.eq_key_bind("FORWARD");
        let set = mq.set_eq_key_bind("forward", BindSlot::Alternate, combo("shift+W"));
        let after = mq.eq_key_bind("forward");
        let missing = (mq.eq_key_bind("nope"), mq.set_eq_key_bind("nope", BindSlot::Normal, combo("W")));
        plugin.log.push(format!("{before:?}|{set:?}|{after:?}|{missing:?}"));
    });
    let up = KeyCombo::new(Key::named("Up").unwrap());
    let expected = format!(
        "{:?}|{:?}|{:?}|{:?}",
        Some(KeyBinding { normal: up, alternate: KeyCombo::CLEAR }),
        Ok::<(), KeyBindError>(()),
        Some(KeyBinding { normal: up, alternate: combo("shift+W") }),
        (None::<KeyBinding>, Err::<(), _>(KeyBindError::NotFound)),
    );
    assert_eq!(log_of(rt)[1], expected);
}

#[test]
fn the_games_bindings_are_listed_in_order() {
    let rt = with_binds(|plugin, mq| {
        let names: Vec<String> = mq.eq_key_binds().into_iter().map(|(name, keys)| format!("{name}={}", keys.normal)).collect();
        plugin.log.push(names.join(","));
    });
    assert_eq!(log_of(rt)[1], "forward=Up,jump=Space");
}

#[test]
fn the_games_bindings_are_unavailable_until_loaded() {
    fake::with(|binds| binds.eq_loaded = false);
    let rt = with_binds(|plugin, mq| {
        plugin.log.push(format!("{:?} {:?}", mq.eq_key_bind("jump"), mq.set_eq_key_bind("jump", BindSlot::Normal, combo("J"))));
        plugin.log.push(format!("{}", mq.eq_key_binds().len()));
    });
    assert_eq!(log_of(rt)[1..], [format!("None {:?}", Err::<(), _>(KeyBindError::Unavailable)), "0".to_owned()]);

}
