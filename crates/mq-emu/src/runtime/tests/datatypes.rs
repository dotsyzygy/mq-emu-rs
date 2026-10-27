//! The plugin's own datatypes, against a fake of MacroQuest's type registry:
//! a type holds its members, callbacks and user pointer as the C++
//! `mqemu_type` does, and a value holds a type and a key, as an `MQTypeVar`
//! does (so it outlives the plugin's handle on the type).

use std::cell::OnceCell;
use std::ffi::{c_char, c_void};

use mq_emu_sys::plugin::MQPluginHandle;
use mq_emu_sys::shim::{key_kind, mqemu_drop_fn, mqemu_key, mqemu_member_fn, mqemu_tlo_result, mqemu_type, mqemu_type_text_fn};

use super::data::fake_eval;
use super::*;
use crate::data::{Value, parse_path};
use crate::datatypes::{CustomValue, DataType, DataTypeDef, DataTypeError};

pub(super) struct FakeType {
    name: String,
    /// Name, method or not, ID.
    members: Vec<(String, bool, i32)>,
    member: mqemu_member_fn,
    text: mqemu_type_text_fn,
    /// Taken when the type is removed.
    user: Option<(*mut c_void, mqemu_drop_fn)>,
    registered: bool,
}

/// A value in the fake data system: one of MacroQuest's types, or one of the
/// plugin's (its type and key).
#[derive(Debug, Clone, PartialEq)]
pub(super) enum FakeVal {
    Plain(Value),
    Keyed { ty: *mut FakeType, key: FakeKey },
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum FakeKey {
    Int(i64),
    Text(String),
}

impl FakeVal {
    pub(super) fn plain(self) -> Value {
        match self {
            FakeVal::Plain(value) => value,
            keyed => panic!("the fake doesn't read {keyed:?} back"),
        }
    }
}

impl FakeKey {
    fn with_c<R>(&self, f: impl FnOnce(&mqemu_key) -> R) -> R {
        f(&match self {
            FakeKey::Int(n) => mqemu_key { kind: key_kind::INT, integer: *n, text: ptr::null(), len: 0 },
            FakeKey::Text(s) => mqemu_key { kind: key_kind::TEXT, integer: 0, text: s.as_ptr().cast(), len: s.len() },
        })
    }
}

thread_local! {
    /// Types added and not freed.
    static FAKE_TYPES: RefCell<Vec<*mut FakeType>> = const { RefCell::new(Vec::new()) };
    static TYPE_DROPS: Cell<u32> = const { Cell::new(0) };
    static TYPE_FREES: Cell<u32> = const { Cell::new(0) };
}

fn counted_drop(drop: mqemu_drop_fn, user: *mut c_void) {
    TYPE_DROPS.with(|drops| drops.set(drops.get() + 1));
    unsafe { drop(user) };
}

fn registered_names() -> Vec<String> {
    FAKE_TYPES.with(|types| types.borrow().iter().map(|&t| unsafe { &*t }).filter(|t| t.registered).map(|t| t.name.clone()).collect())
}

pub(super) fn fake_add_type(
    _: &MQPluginHandle,
    name: &str,
    _key_kind: i32,
    member: mqemu_member_fn,
    text: mqemu_type_text_fn,
    user: *mut c_void,
    drop: mqemu_drop_fn,
) -> *mut mqemu_type {
    // MacroQuest's registry is case-sensitive.
    if registered_names().iter().any(|n| n == name) {
        counted_drop(drop, user); // ownership passed even though adding failed
        return ptr::null_mut();
    }
    let ty = Box::into_raw(Box::new(FakeType { name: name.into(), members: Vec::new(), member, text, user: Some((user, drop)), registered: true }));
    FAKE_TYPES.with(|types| types.borrow_mut().push(ty));
    ty.cast()
}

pub(super) fn fake_type_add_member(ty: *mut mqemu_type, id: i32, name: &str, method: bool) -> bool {
    let ty = unsafe { &mut *ty.cast::<FakeType>() };
    if ty.members.iter().any(|(n, ..)| n == name) {
        return false;
    }
    ty.members.push((name.into(), method, id));
    true
}

pub(super) fn fake_remove_type(ty: *mut mqemu_type) {
    let ty = unsafe { &mut *ty.cast::<FakeType>() };
    ty.registered = false;
    if let Some((user, drop)) = ty.user.take() {
        counted_drop(drop, user);
    }
}

pub(super) fn fake_free_type(ty: *mut mqemu_type) {
    fake_remove_type(ty);
    FAKE_TYPES.with(|types| types.borrow_mut().retain(|&t| t != ty.cast()));
    drop(unsafe { Box::from_raw(ty.cast::<FakeType>()) });
    TYPE_FREES.with(|frees| frees.set(frees.get() + 1));
}

/// What `mqemu_tlo_set_key` puts in a result. None if the type was removed.
pub(super) fn fake_keyed(custom: &CustomValue) -> Option<FakeVal> {
    let ty = custom.raw_type().cast::<FakeType>();
    if ty.is_null() || !unsafe { &*ty }.registered {
        return None;
    }
    let key = custom.with_c_key(|key| match key.kind {
        key_kind::INT => FakeKey::Int(key.integer),
        _ => FakeKey::Text(String::from_utf8(unsafe { std::slice::from_raw_parts(key.text.cast::<u8>(), key.len) }.to_vec()).unwrap()),
    });
    Some(FakeVal::Keyed { ty, key })
}

/// `value.Member[index]`, as `mqemu_type::GetMember` evaluates it.
fn member(value: &FakeVal, name: &str, index: &str) -> Option<FakeVal> {
    let FakeVal::Keyed { ty, key } = value else { panic!("the fake has no members for {value:?}") };
    let ty = unsafe { &**ty };
    let (user, _) = ty.user?;
    let &(_, _, id) = ty.members.iter().find(|(n, ..)| n == name)?;
    let index = CString::new(index).unwrap();
    let mut result: Option<FakeVal> = None;
    let set = key.with_c(|key| unsafe { (ty.member)(user, key, id, index.as_ptr(), (&raw mut result).cast()) });
    // A method sets nothing: a macro keeps the value it was called on.
    set.then(|| result.unwrap_or_else(|| value.clone()))
}

/// The value as `${...}` shows it; None for NULL.
fn text(value: &FakeVal) -> Option<String> {
    let FakeVal::Keyed { ty, key } = value else { return Some(value.clone().plain().to_string()) };
    let ty = unsafe { &**ty };
    let (user, _) = ty.user?;
    let mut buffer = [0 as c_char; 64];
    let set = key.with_c(|key| unsafe { (ty.text)(user, key, buffer.as_mut_ptr(), buffer.len()) });
    set.then(|| unsafe { crate::text::from_c(buffer.as_ptr()) }.into_owned())
}

/// Reads `${path}` as a macro would; None for NULL.
fn read(path: &str) -> Option<String> {
    let steps = parse_path(path).unwrap();
    let ((name, index), members) = steps.split_first().unwrap();
    let mut value = fake_eval(name, index)?;
    for (name, index) in members {
        value = member(&value, name, index)?;
    }
    text(&value)
}

fn drops() -> u32 {
    TYPE_DROPS.with(Cell::get)
}

fn frees() -> u32 {
    TYPE_FREES.with(Cell::get)
}

// ----- the test type -------------------------------------------------------------------

/// The `Counter` type, keyed by an i32, whose members do arithmetic on it.
fn add_counter_type(mq: &Mq<Recorder>) -> DataType<i32> {
    // `Next` returns the type's own values.
    let own: Rc<OnceCell<DataType<i32>>> = Rc::default();
    let next = Rc::clone(&own);
    let counter = mq
        .add_type(
            "Counter",
            DataTypeDef::new()
                .member("Value", |_, _, n: &i32, _| Some((*n).into()))
                .member("Label", |_, _, n, _| Some(format!("counter {n}").into()))
                .member("Plus", |_, _, n, index| Some((n + index.parse::<i32>().ok()?).into()))
                .member("Next", move |_, _, n, _| Some(next.get()?.value(n + 1)))
                .member("Panic", |_, _, _, _| panic!("member panic"))
                .method("Bump", |plugin: &mut Recorder, _, n, index| plugin.log.push(format!("bump {n} {index:?}")))
                .method("Remove", |_, mq, _, _| assert!(mq.remove_type("Counter")))
                .text(|_, _, n| format!("#{n}")),
        )
        .unwrap();
    own.set(counter.clone()).unwrap();
    counter
}

/// `${Counter[n]}`.
fn add_counter(mq: &Mq<Recorder>) {
    let counter = add_counter_type(mq);
    mq.add_tlo("Counter", move |_, _, index| Some(counter.value(index.parse().ok()?))).unwrap();
}

fn runtime_with_counter() -> &'static Runtime<Recorder> {
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| add_counter(mq));
    rt.initialize();
    rt
}

// ----- tests ---------------------------------------------------------------------------

#[test]
fn members_methods_and_text_answer_from_the_plugin() {
    let _main = FakeMain::install();
    let rt = runtime_with_counter();
    assert_eq!(read("Counter[5]").as_deref(), Some("#5"));
    assert_eq!(member(&fake_eval("Counter", "5").unwrap(), "Value", ""), Some(FakeVal::Plain(Value::Int(5))));
    assert_eq!(read("Counter[5].Label").as_deref(), Some("counter 5"));
    assert_eq!(read("Counter[5].Plus[10]").as_deref(), Some("15"));
    assert_eq!(read("Counter[5].Plus[ten]"), None, "a member's None is NULL");
    assert_eq!(read("Counter[5].Next.Next.Value").as_deref(), Some("7"), "members return values of the type");
    assert_eq!(read("Counter[5].Bump[hi]").as_deref(), Some("#5"), "a method keeps the value it was called on");
    assert_eq!(read("Counter[5].label"), None, "member names keep their case");
    assert_eq!(read("Counter[5].Nope"), None);
    assert_eq!(read("Counter[x]"), None);
    assert_eq!(log_of(rt)[1..], [r#"bump 5 "hi""#]);
}

#[test]
fn text_and_unit_keys() {
    let _main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        // No text: shown as the type's name, as MacroQuest shows a type without one.
        let bot = mq.add_type("Bot", DataTypeDef::new().member("Name", |_: &mut Recorder, _, name: &String, _| Some(name.as_str().into()))).unwrap();
        mq.add_tlo("Bot", move |_, _, index| Some(bot.value(index.to_owned()))).unwrap();
        let camp = mq.add_type("Camp", DataTypeDef::new().member("Ready", |_: &mut Recorder, _, _: &(), _| Some(true.into()))).unwrap();
        mq.add_tlo("Camp", move |_, _, _| Some(camp.value(()))).unwrap();
    });
    rt.initialize();
    assert_eq!(read("Bot[Tank]").as_deref(), Some("Bot"));
    assert_eq!(read("Bot[Tank].Name").as_deref(), Some("Tank"));
    assert_eq!(read("Bot[].Name").as_deref(), Some(""));
    assert_eq!(read("Camp.Ready").as_deref(), Some("TRUE"));
}

#[test]
fn custom_values_compare_by_type_and_key() {
    let _main = FakeMain::install();
    in_pulse(|_, mq| {
        let a = mq.add_type("A", DataTypeDef::<Recorder, i32>::new()).unwrap();
        let b = mq.add_type("B", DataTypeDef::<Recorder, i32>::new()).unwrap();
        assert_eq!(a.value(1), a.value(1));
        assert_ne!(a.value(1), a.value(2));
        assert_ne!(a.value(1), b.value(1));
        let Value::Custom(custom) = a.value(7) else { unreachable!() };
        assert_eq!((custom.type_name(), custom.key::<i32>(), custom.key::<String>()), ("A", Some(7), None));
        assert_eq!(a.value(7).to_string(), "A");
        assert_eq!(format!("{:?}", a.value(7)), r#"Custom(CustomValue { type_name: "A", key: 7 })"#);
    });
}

#[test]
fn keys_of_another_kind_are_null() {
    let _main = FakeMain::install();
    runtime_with_counter();
    // What a Lua script returning ('Counter', 'five') would make.
    let Some(FakeVal::Keyed { ty, .. }) = fake_eval("Counter", "5") else { panic!() };
    let wrong = FakeVal::Keyed { ty, key: FakeKey::Text("five".into()) };
    assert_eq!(member(&wrong, "Value", ""), None);
    assert_eq!(text(&wrong), None);
    let too_big = FakeVal::Keyed { ty, key: FakeKey::Int(1 << 40) };
    assert_eq!(member(&too_big, "Value", ""), None);
}

#[test]
fn own_types_are_null_inside_the_plugins_callbacks() {
    let _main = FakeMain::install();
    let rt = runtime_with_counter();
    let five = fake_eval("Counter", "5").unwrap();
    set_hook(&ON_PULSE, move |plugin, _| {
        // This callback holds the plugin, so the members can't run.
        plugin.log.push(format!("{:?} {:?}", member(&five, "Value", ""), text(&five)));
    });
    rt.pulse();
    assert_eq!(log_of(rt).last().unwrap(), "None None");
    assert_eq!(rt.stats().dropped, 2);
    assert_eq!(read("Counter[5].Value").as_deref(), Some("5"), "from outside, they answer");
}

#[test]
fn a_panicking_member_is_null_and_reported_once() {
    let _main = FakeMain::install();
    let rt = runtime_with_counter();
    assert_eq!(read("Counter[1].Panic"), None);
    assert_eq!(read("Counter[1].Panic"), None);
    assert_eq!(rt.stats().panics, 2);
    assert_eq!(chat_lines(), ["[test] panicked in a datatype: member panic"]);
    assert_eq!(read("Counter[1].Value").as_deref(), Some("1"), "still works afterwards");
}

#[test]
fn type_and_member_names_are_checked_and_duplicates_refused() {
    let _main = FakeMain::install();
    // Another plugin's type.
    unsafe extern "C" fn no_member(_: *mut c_void, _: *const mqemu_key, _: i32, _: *const c_char, _: *mut mqemu_tlo_result) -> bool {
        false
    }
    unsafe extern "C" fn no_text(_: *mut c_void, _: *const mqemu_key, _: *mut c_char, _: usize) -> bool {
        false
    }
    unsafe extern "C" fn nothing(_: *mut c_void) {}
    fake_add_type(&MQPluginHandle { pluginID: 1 }, "Taken", key_kind::INT, no_member, no_text, ptr::null_mut(), nothing);

    let rt = in_pulse(|plugin, mq| {
        let def = DataTypeDef::<Recorder, i32>::new;
        let none = |_: &mut Recorder, _: &Mq<Recorder>, _: &i32, _: &str| None;
        for (name, def) in [
            ("Bad Name", def()),
            ("T1", def().member("Bad.Member", none)),
            ("T2", def().member("2nd", none)),
            ("T3", def().member("Same", none).method("Same", |_, _, _, _| {})),
            ("Mine", def().member("Same", none).member("same", none)),
            ("mine", def()),
            ("Taken", def()),
        ] {
            plugin.log.push(format!("{name}: {:?}", mq.add_type(name, def).map(|ty| ty.name().to_owned())));
        }
    });
    assert_eq!(
        log_of(rt)[2..], // after "new" and "pulse"
        [
            format!("Bad Name: {:?}", Err::<String, _>(DataTypeError::InvalidName)),
            format!("T1: {:?}", Err::<String, _>(DataTypeError::InvalidMember("Bad.Member".into()))),
            format!("T2: {:?}", Err::<String, _>(DataTypeError::InvalidMember("2nd".into()))),
            format!("T3: {:?}", Err::<String, _>(DataTypeError::InvalidMember("Same".into()))),
            r#"Mine: Ok("Mine")"#.to_owned(),
            format!("mine: {:?}", Err::<String, _>(DataTypeError::AlreadyAdded)),
            format!("Taken: {:?}", Err::<String, _>(DataTypeError::Rejected)),
        ]
    );
    // The refused "Taken" handlers were dropped, not leaked; "Mine" is held.
    assert_eq!(drops(), 1);
    assert_eq!(registered_names(), ["Taken", "Mine"]);
}

#[test]
fn adding_a_type_without_macroquest_is_unavailable() {
    // No FakeMain: MacroQuest's API isn't there.
    let rt = in_pulse(|plugin, mq| {
        plugin.log.push(format!("{:?}", mq.add_type("Mine", DataTypeDef::<Recorder, ()>::new()).map(|_| ())));
    });
    assert_eq!(log_of(rt).last().unwrap(), &format!("{:?}", Err::<(), _>(DataTypeError::Unavailable)));
    assert!(registered_names().is_empty());
}

#[test]
fn removed_types_read_null_and_are_freed_on_unload() {
    let _main = FakeMain::install();
    let rt = runtime_with_counter();
    let before = fake_eval("Counter", "5").unwrap();
    set_hook(&ON_PULSE, |plugin, mq| plugin.log.push(format!("{} {}", mq.remove_type("counter"), mq.remove_type("Counter"))));
    rt.pulse();
    assert_eq!(log_of(rt).last().unwrap(), "true false");
    assert!(registered_names().is_empty());
    assert_eq!((member(&before, "Value", ""), text(&before)), (None, None), "values still around read NULL");
    assert_eq!(fake_eval("Counter", "5"), None, "a removed type's values can't be returned");
    assert_eq!((drops(), frees()), (1, 0), "kept for the values that point at it");

    // Added again under the same name.
    set_hook(&ON_PULSE, |_, mq| {
        mq.remove_tlo("Counter");
        add_counter(mq);
    });
    rt.pulse();
    assert_eq!(read("Counter[5].Value").as_deref(), Some("5"));

    rt.shutdown();
    assert!(registered_names().is_empty(), "unloading removes the plugin's types");
    assert_eq!((drops(), frees()), (2, 2), "each dropped and freed exactly once");
}

#[test]
fn a_member_can_remove_its_own_type() {
    let _main = FakeMain::install();
    let rt = runtime_with_counter();
    // The method runs, then the macro shows the value, whose type is gone.
    assert_eq!(read("Counter[5].Remove"), None);
    assert!(registered_names().is_empty());
    assert_eq!(drops(), 1);
    assert_eq!(rt.stats().panics, 0);
}
