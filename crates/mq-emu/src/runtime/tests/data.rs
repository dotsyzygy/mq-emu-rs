//! Reading MacroQuest's data and providing TLOs, against a fake data system:
//! a few values (`Me`, `Spawn[id 7]`, members, a buff) and a TLO registry
//! that holds callbacks the way MacroQuest does.

use std::ffi::{c_char, c_void};

use mq_emu_sys::shim::{mqemu_data, mqemu_drop_fn, mqemu_scalar, mqemu_tlo_fn, mqemu_tlo_result};

use super::*;
use crate::data::{DataApi, TloError, Value};

/// A value in the fake data system.
struct FakeData {
    type_name: &'static str,
    text: String,
    scalar: mqemu_scalar,
}

impl FakeData {
    fn boxed(type_name: &'static str, text: &str, integer: i64, real: f64, boolean: bool) -> *mut mqemu_data {
        LIVE_DATA.with(|live| live.set(live.get() + 1));
        let data = FakeData { type_name, text: text.into(), scalar: mqemu_scalar { integer, real, boolean } };
        Box::into_raw(Box::new(data)).cast()
    }

    fn of<'a>(data: *mut mqemu_data) -> &'a FakeData {
        unsafe { &*data.cast::<FakeData>() }
    }
}

struct FakeTlo {
    name: String,
    callback: mqemu_tlo_fn,
    user: *mut c_void,
    drop: mqemu_drop_fn,
}

thread_local! {
    /// FakeData values not yet freed.
    static LIVE_DATA: Cell<i64> = const { Cell::new(0) };
    /// TLOs added through the fake, as MacroQuest would hold them.
    static FAKE_TLOS: RefCell<Vec<FakeTlo>> = const { RefCell::new(Vec::new()) };
    /// How many TLO user pointers have been dropped.
    static TLO_DROPS: Cell<u32> = const { Cell::new(0) };
}

/// Evaluates a TLO added through the fake, as a macro would (from outside
/// the plugin's callbacks). None for NULL or no such TLO.
fn fake_eval_tlo(name: &str, index: &str) -> Option<Value> {
    let found = FAKE_TLOS.with(|tlos| tlos.borrow().iter().find(|t| t.name.eq_ignore_ascii_case(name)).map(|t| (t.callback, t.user)));
    let (callback, user) = found?;
    let index = CString::new(index).unwrap();
    let mut result: Option<Value> = None;
    let set = unsafe { callback(user, index.as_ptr(), (&raw mut result).cast()) };
    if set { result } else { None }
}

fn fake_tlo_names() -> Vec<String> {
    FAKE_TLOS.with(|tlos| tlos.borrow().iter().map(|t| t.name.clone()).collect())
}

fn fake_member(data: *mut mqemu_data, member: &str, index: &str) -> *mut mqemu_data {
    let data = FakeData::of(data);
    match (data.type_name, member, index) {
        ("spawn", "Name", "") => FakeData::boxed("string", &data.text, 0, 0.0, false),
        ("spawn", "Level", "") => FakeData::boxed("int", "65", 65, 65.0, true),
        ("spawn", "X", "") => FakeData::boxed("float", "-12.50", -12, -12.5, true),
        // Set the old way, as a number.
        ("spawn", "Grouped", "") => FakeData::boxed("bool", "TRUE", 1, 1.0, false),
        ("spawn", "Buff", "1") => FakeData::boxed("buff", "Aegolism", 0x1000, 0.0, true),
        ("buff", "Name", "") => FakeData::boxed("string", &data.text, 0, 0.0, false),
        _ => ptr::null_mut(),
    }
}

/// Counts drops of TLO user pointers, then drops them for real.
fn counted_drop(drop: mqemu_drop_fn, user: *mut c_void) {
    TLO_DROPS.with(|drops| drops.set(drops.get() + 1));
    unsafe { drop(user) };
}

pub(super) const FAKE_DATA: DataApi = DataApi {
    tlo: |_, name, index| match (name, index) {
        ("Me", "") => FakeData::boxed("spawn", "Syzygy", 0x2000, 0.0, true),
        ("Spawn", "id 7") => FakeData::boxed("spawn", "Qtee", 0x3000, 0.0, true),
        _ => match fake_eval_tlo(name, index) {
            Some(Value::String(s)) => FakeData::boxed("string", &s, 0, 0.0, false),
            Some(Value::Int(n)) => FakeData::boxed("int", &n.to_string(), n, n as f64, n != 0),
            Some(other) => panic!("the fake doesn't model {other:?}"),
            None => ptr::null_mut(),
        },
    },
    member: fake_member,
    free: |data| {
        LIVE_DATA.with(|live| live.set(live.get() - 1));
        drop(unsafe { Box::from_raw(data.cast::<FakeData>()) });
    },
    type_name: |data| FakeData::of(data).type_name.to_owned(),
    text: |data| FakeData::of(data).text.clone(),
    scalar: |data| FakeData::of(data).scalar,
    add_tlo: |_, _, name, callback, user, drop| {
        if fake_tlo_names().iter().any(|n| n.eq_ignore_ascii_case(name)) {
            counted_drop(drop, user); // ownership passed even though adding failed
            return false;
        }
        FAKE_TLOS.with(|tlos| tlos.borrow_mut().push(FakeTlo { name: name.into(), callback, user, drop }));
        true
    },
    remove_tlo: |_, _, name| {
        let removed = FAKE_TLOS.with(|tlos| {
            let mut tlos = tlos.borrow_mut();
            let at = tlos.iter().position(|t| t.name.eq_ignore_ascii_case(name))?;
            Some(tlos.remove(at))
        });
        removed.map(|tlo| counted_drop(tlo.drop, tlo.user)).is_some()
    },
    set_result: |result, value| {
        if matches!(value, Value::Object { .. }) {
            return false;
        }
        unsafe { *result.cast::<Option<Value>>() = Some(value.clone()) };
        true
    },
};

/// A TLO that "another plugin" registered.
fn add_foreign_tlo(name: &str) {
    unsafe extern "C" fn never(_: *mut c_void, _: *const c_char, _: *mut mqemu_tlo_result) -> bool {
        false
    }
    unsafe extern "C" fn nothing(_: *mut c_void) {}
    FAKE_TLOS.with(|tlos| tlos.borrow_mut().push(FakeTlo { name: name.into(), callback: never, user: ptr::null_mut(), drop: nothing }));
}

fn live_data() -> i64 {
    LIVE_DATA.with(Cell::get)
}

// ----- reading -------------------------------------------------------------------------

#[test]
fn data_is_read_typed_and_freed() {
    let _main = FakeMain::install();
    in_pulse(|_, mq| {
        let me = mq.tlo("Me").expect("Me");
        assert_eq!(me.type_name(), "spawn");
        assert_eq!(me.value(), Value::Object { type_name: "spawn".into(), text: "Syzygy".into() });
        assert_eq!(me.member("Name").and_then(|d| d.as_string()).as_deref(), Some("Syzygy"));
        assert_eq!(me.member("Level").and_then(|d| d.as_int()), Some(65));
        assert_eq!(me.member("X").and_then(|d| d.as_float()), Some(-12.5));
        assert_eq!(me.member("Grouped").and_then(|d| d.as_bool()), Some(true));
        assert_eq!(mq.data("Me.Buff[1].Name").unwrap().map(|d| d.text()).as_deref(), Some("Aegolism"));
        assert_eq!(mq.tlo_at("Spawn", "id 7").and_then(|s| s.member("Name")).map(|d| d.text()).as_deref(), Some("Qtee"));
    });
    assert_eq!(live_data(), 0, "every value freed");
}

#[test]
fn missing_data_is_none_and_bad_paths_are_errors() {
    let _main = FakeMain::install();
    in_pulse(|_, mq| {
        assert!(mq.tlo("NoSuchTlo").is_none());
        assert!(mq.tlo_at("Spawn", "id 8").is_none());
        let me = mq.tlo("Me").unwrap();
        assert!(me.member("NoSuchMember").is_none());
        assert!(me.member("Level").unwrap().as_string().is_none(), "an int isn't a string");
        assert!(me.member("Name").unwrap().as_int().is_none(), "a string isn't an int");
        // A NULL step ends the path without an error.
        assert!(mq.data("Me.Buff[2].Name").unwrap().is_none());
        assert!(mq.data("Nope.Name").unwrap().is_none());
        assert_eq!(mq.data("Me..Name").unwrap_err().reason, "empty name");
        assert_eq!(mq.data("Me.Buff[1").unwrap_err().reason, "unclosed [");
    });
    assert_eq!(live_data(), 0);
}

#[test]
fn reading_without_macroquest_is_none() {
    // No FakeMain: MacroQuest's API isn't there.
    in_pulse(|_, mq| assert!(mq.tlo("Me").is_none()));
    assert_eq!(live_data(), 0);
}

// ----- providing -----------------------------------------------------------------------

fn add_test_tlo(mq: &Mq<Recorder>) {
    mq.add_tlo("RustTest", |plugin: &mut Recorder, _, index| {
        plugin.log.push(format!("tlo {index:?}"));
        match index {
            "" => Some("ready".into()),
            "count" => Some(Value::Int(plugin.log.len() as i64)),
            "object" => Some(Value::Object { type_name: "spawn".into(), text: "x".into() }),
            "panic" => panic!("tlo panic"),
            _ => None,
        }
    })
    .unwrap();
}

fn runtime_with_test_tlo() -> &'static Runtime<Recorder> {
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| add_test_tlo(mq));
    rt.initialize();
    rt
}

#[test]
fn provided_tlos_answer_from_the_plugin() {
    let _main = FakeMain::install();
    let rt = runtime_with_test_tlo();
    assert_eq!(fake_eval_tlo("RustTest", ""), Some(Value::from("ready")));
    assert_eq!(fake_eval_tlo("rusttest", "count"), Some(Value::Int(3)), "names are case-insensitive");
    assert_eq!(fake_eval_tlo("RustTest", "unknown"), None);
    // Objects can't be provided: NULL.
    assert_eq!(fake_eval_tlo("RustTest", "object"), None);
    assert_eq!(log_of(rt)[1..], [r#"tlo """#, r#"tlo "count""#, r#"tlo "unknown""#, r#"tlo "object""#]);
}

#[test]
fn a_panicking_tlo_is_null_and_reported_once() {
    let _main = FakeMain::install();
    let rt = runtime_with_test_tlo();
    assert_eq!(fake_eval_tlo("RustTest", "panic"), None);
    assert_eq!(fake_eval_tlo("RustTest", "panic"), None);
    assert_eq!(rt.stats().panics, 2);
    assert_eq!(chat_lines(), ["[test] panicked in a TLO: tlo panic"]);
    assert_eq!(fake_eval_tlo("RustTest", ""), Some(Value::from("ready")), "still works afterwards");
}

#[test]
fn own_tlos_are_null_inside_the_plugins_callbacks() {
    let _main = FakeMain::install();
    let rt = runtime_with_test_tlo();
    set_hook(&ON_PULSE, |plugin, mq| {
        // This callback holds the plugin, so the TLO's handler can't run.
        let own = mq.tlo("RustTest").map(|d| d.value());
        plugin.log.push(format!("own tlo: {own:?}"));
    });
    rt.pulse();
    assert_eq!(log_of(rt).last().unwrap(), "own tlo: None");
    assert_eq!(rt.stats().dropped, 1, "counted as a re-entrant event that couldn't be served");
    // From outside (a macro), it works.
    assert_eq!(fake_eval_tlo("RustTest", ""), Some(Value::from("ready")));
    assert_eq!(live_data(), 0);
}

#[test]
fn tlo_names_are_checked_and_duplicates_refused() {
    let _main = FakeMain::install();
    add_foreign_tlo("Taken");
    let rt = in_pulse(|plugin, mq| {
        let none = |_: &mut Recorder, _: &Mq<Recorder>, _: &str| None;
        for name in ["Bad Name", "Mine", "mine", "Taken"] {
            plugin.log.push(format!("{name}: {:?}", mq.add_tlo(name, none)));
        }
    });
    assert_eq!(
        log_of(rt)[2..], // after "new" and "pulse"
        [
            format!("Bad Name: {:?}", Err::<(), _>(TloError::InvalidName)),
            "Mine: Ok(())".to_owned(),
            format!("mine: {:?}", Err::<(), _>(TloError::AlreadyAdded)),
            format!("Taken: {:?}", Err::<(), _>(TloError::Rejected)),
        ]
    );
    // The refused "Taken" handler was dropped, not leaked; "Mine" is held.
    assert_eq!(TLO_DROPS.with(Cell::get), 1);
    assert_eq!(fake_tlo_names(), ["Taken", "Mine"]);
}

#[test]
fn tlos_are_removed_once_and_on_unload() {
    let _main = FakeMain::install();
    let rt = runtime_with_test_tlo();
    set_hook(&ON_PULSE, |plugin, mq| {
        mq.add_tlo("Second", |_: &mut Recorder, _, _| Some(Value::Bool(true))).unwrap();
        plugin.log.push(format!("{} {}", mq.remove_tlo("second"), mq.remove_tlo("Second")));
    });
    rt.pulse();
    assert_eq!(log_of(rt).last().unwrap(), "true false");
    assert_eq!(TLO_DROPS.with(Cell::get), 1);
    assert_eq!(fake_tlo_names(), ["RustTest"]);

    rt.shutdown();
    assert!(fake_tlo_names().is_empty(), "unloading removes the plugin's TLOs");
    assert_eq!(TLO_DROPS.with(Cell::get), 2, "each dropped exactly once");
    assert_eq!(fake_eval_tlo("RustTest", ""), None);
}
