//! The plugin's own datatypes, against a fake of MacroQuest's type registry:
//! a type holds its members, callbacks, parent and user pointer as the C++
//! `mqemu_type` does, and a value holds a type and a key, as an `MQTypeVar`
//! does (so it outlives the plugin's handle on the type). Extensions of
//! other types are faked as `mqemu_extension` is.

use std::cell::OnceCell;
use std::ffi::{c_char, c_void};

use mq_emu_sys::plugin::MQPluginHandle;
use mq_emu_sys::shim::{
    ANY_MEMBER, inherit, key_kind, mqemu_any_member_fn, mqemu_downcast_fn, mqemu_drop_fn, mqemu_extension, mqemu_extension_member_fn,
    mqemu_key, mqemu_member_fn, mqemu_tlo_result, mqemu_type, mqemu_type_text_fn,
};

use super::data::{FAKE_MQ_TYPES, FakeData, fake_eval, fake_mq_member, live_data};
use super::*;
use crate::data::{Data, Value, parse_path};
use crate::datatypes::{CustomValue, DataType, DataTypeDef, DataTypeError, TypeExtension};
use crate::game::SpawnId;

pub(super) struct FakeType {
    name: String,
    /// The adding plugin's ID.
    plugin: u64,
    /// Its parent's name, found again by name each time as the shim does,
    /// and the downcast.
    parent: Option<(String, mqemu_downcast_fn)>,
    /// Name, method or not, ID.
    members: Vec<(String, bool, i32)>,
    member: mqemu_member_fn,
    any_member: Option<mqemu_any_member_fn>,
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
    /// Extensions made and not freed.
    static FAKE_EXTENSIONS: RefCell<Vec<*mut FakeExtension>> = const { RefCell::new(Vec::new()) };
    static EXTENSION_FREES: Cell<u32> = const { Cell::new(0) };
}

fn counted_drop(drop: mqemu_drop_fn, user: *mut c_void) {
    TYPE_DROPS.with(|drops| drops.set(drops.get() + 1));
    unsafe { drop(user) };
}

fn registered_names() -> Vec<String> {
    FAKE_TYPES.with(|types| types.borrow().iter().map(|&t| unsafe { &*t }).filter(|t| t.registered).map(|t| t.name.clone()).collect())
}

pub(super) fn fake_add_type(
    handle: &MQPluginHandle,
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
    let ty = FakeType { name: name.into(), plugin: handle.pluginID, parent: None, members: Vec::new(), member, any_member: None, text, user: Some((user, drop)), registered: true };
    let ty = Box::into_raw(Box::new(ty));
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

pub(super) fn fake_type_any_member(ty: *mut mqemu_type, any_member: mqemu_any_member_fn) {
    unsafe { &mut *ty.cast::<FakeType>() }.any_member = Some(any_member);
}

fn registered_type(name: &str) -> Option<*mut FakeType> {
    FAKE_TYPES.with(|types| types.borrow().iter().copied().find(|&t| unsafe { &*t }.registered && unsafe { &*t }.name == name))
}

fn is_mq_type(name: &str) -> bool {
    FAKE_MQ_TYPES.iter().any(|(n, _)| *n == name)
}

pub(super) fn fake_type_inherit(ty: *mut mqemu_type, parent: &str, downcast: mqemu_downcast_fn) -> i32 {
    let ty = unsafe { &mut *ty.cast::<FakeType>() };
    if !is_mq_type(parent) {
        let Some(found) = registered_type(parent) else { return inherit::NO_TYPE };
        // Itself, or another plugin's.
        if std::ptr::eq(found, ty) || unsafe { &*found }.plugin != ty.plugin {
            return inherit::REFUSED;
        }
    }
    ty.parent = Some((parent.into(), downcast));
    inherit::OK
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
    let (ty, key) = match value {
        FakeVal::Keyed { ty, key } => (ty, key),
        // One of MacroQuest's: its extensions first, then the type's own.
        FakeVal::Plain(object @ Value::Object { type_name, text }) => {
            return extension_member(type_name, text, name, index).or_else(|| fake_mq_member(object, name, index).map(FakeVal::Plain));
        }
        _ => panic!("the fake has no members for {value:?}"),
    };
    let raw_ty = *ty;
    let ty = unsafe { &*raw_ty };
    let (user, _) = ty.user?;
    let found = ty.members.iter().find(|(n, ..)| n == name).map(|&(_, _, id)| id).filter(|id| *id != ANY_MEMBER);
    let Some(id) = found else {
        if let Some(any_member) = ty.any_member {
            let (c_name, c_index) = (CString::new(name).unwrap(), CString::new(index).unwrap());
            let mut result: Option<FakeVal> = None;
            if key.with_c(|key| unsafe { any_member(user, key, c_name.as_ptr(), c_index.as_ptr(), (&raw mut result).cast()) }) {
                return result;
            }
            // Listed, so that MacroQuest doesn't print "No such member".
            fake_type_add_member(raw_ty.cast(), ANY_MEMBER, name, false);
        }
        // The parent's members, as `mqemu_type::GetMember` passes them on.
        let (parent, downcast) = unsafe { &*raw_ty }.parent.as_ref()?;
        let mut up: Option<FakeVal> = None;
        if !key.with_c(|key| unsafe { downcast(user, key, (&raw mut up).cast()) }) {
            return None;
        }
        return match up? {
            // As EvaluateMacroDataMember evaluates it: extensions too.
            FakeVal::Plain(object @ Value::Object { .. }) if is_mq_type(parent) => {
                let Value::Object { type_name, .. } = &object else { unreachable!() };
                if type_name != parent {
                    return None;
                }
                member(&FakeVal::Plain(object), name, index)
            }
            keyed @ FakeVal::Keyed { ty, .. } if registered_type(parent) == Some(ty) => member(&keyed, name, index),
            _ => None, // a value of another type
        };
    };
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

// ----- extensions --------------------------------------------------------------------

struct FakeExtension {
    type_name: String,
    /// Name, ID.
    members: Vec<(String, i32)>,
    member: mqemu_extension_member_fn,
    user: Option<(*mut c_void, mqemu_drop_fn)>,
    registered: bool,
}

/// The members a type has itself.
fn own_members(type_name: &str) -> Vec<String> {
    match FAKE_MQ_TYPES.iter().find(|(n, _)| *n == type_name) {
        Some((_, members)) => members.iter().map(|m| (*m).to_owned()).collect(),
        None => registered_type(type_name).map(|t| unsafe { &*t }.members.iter().map(|(n, ..)| n.clone()).collect()).unwrap_or_default(),
    }
}

pub(super) fn fake_add_extension(
    _: &MQPluginHandle,
    type_name: &str,
    member: mqemu_extension_member_fn,
    user: *mut c_void,
    drop: mqemu_drop_fn,
) -> *mut mqemu_extension {
    if !is_mq_type(type_name) && registered_type(type_name).is_none() {
        counted_drop(drop, user); // ownership passed even though adding failed
        return ptr::null_mut();
    }
    let extension = FakeExtension { type_name: type_name.into(), members: Vec::new(), member, user: Some((user, drop)), registered: false };
    let extension = Box::into_raw(Box::new(extension));
    FAKE_EXTENSIONS.with(|extensions| extensions.borrow_mut().push(extension));
    extension.cast()
}

pub(super) fn fake_extension_add_member(extension: *mut mqemu_extension, id: i32, name: &str) -> bool {
    let extension = unsafe { &mut *extension.cast::<FakeExtension>() };
    if extension.members.iter().any(|(n, _)| n == name) || own_members(&extension.type_name).iter().any(|n| n == name) {
        return false;
    }
    extension.members.push((name.into(), id));
    true
}

pub(super) fn fake_extension_register(extension: *mut mqemu_extension) -> bool {
    unsafe { &mut *extension.cast::<FakeExtension>() }.registered = true;
    true
}

pub(super) fn fake_remove_extension(extension: *mut mqemu_extension) {
    let extension = unsafe { &mut *extension.cast::<FakeExtension>() };
    extension.registered = false;
    if let Some((user, drop)) = extension.user.take() {
        counted_drop(drop, user);
    }
}

pub(super) fn fake_free_extension(extension: *mut mqemu_extension) {
    fake_remove_extension(extension);
    FAKE_EXTENSIONS.with(|extensions| extensions.borrow_mut().retain(|&e| e != extension.cast()));
    drop(unsafe { Box::from_raw(extension.cast::<FakeExtension>()) });
    EXTENSION_FREES.with(|frees| frees.set(frees.get() + 1));
}

/// `${value.member[index]}` of a `type_name` value shown as `text`, as
/// MacroQuest asks the type's extensions; None for NULL or no such member.
fn extension_member(type_name: &str, text: &str, name: &str, index: &str) -> Option<FakeVal> {
    let found = FAKE_EXTENSIONS.with(|extensions| {
        extensions.borrow().iter().map(|&e| unsafe { &*e }).filter(|e| e.registered && e.type_name == type_name).find_map(|e| {
            let &(_, id) = e.members.iter().find(|(n, _)| n == name)?;
            Some((e.member, e.user?.0, id))
        })
    });
    let (callback, user, id) = found?;
    let value = FakeData::boxed(type_name, text, 0, 0.0, true); // the callback frees it
    let index = CString::new(index).unwrap();
    let mut result: Option<FakeVal> = None;
    let set = unsafe { callback(user, value, id, index.as_ptr(), (&raw mut result).cast()) };
    if set { result } else { None }
}

fn registered_extensions() -> Vec<String> {
    FAKE_EXTENSIONS.with(|e| e.borrow().iter().map(|&e| unsafe { &*e }).filter(|e| e.registered).map(|e| e.type_name.clone()).collect())
}

fn extension_frees() -> u32 {
    EXTENSION_FREES.with(Cell::get)
}

fn drops() -> u32 {
    TYPE_DROPS.with(Cell::get)
}

fn frees() -> u32 {
    TYPE_FREES.with(Cell::get)
}

/// Whether a name is on MacroQuest's list of the type's members, which is
/// what Lua asks before reading one, and what stops a macro's NULL printing
/// "No such member".
fn listed(value: &FakeVal, name: &str) -> bool {
    let FakeVal::Keyed { ty, .. } = value else { panic!("not one of the plugin's: {value:?}") };
    unsafe { &**ty }.members.iter().any(|(n, ..)| n == name)
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

/// `${Remote[path]}`, whose catch-all adds to the path: `Gone` is NULL, and
/// `Listed` is on the type's list. `Name` is a member of its own.
fn add_remote(mq: &Mq<Recorder>, parent: Option<DataType<i32>>) {
    let own: Rc<OnceCell<DataType<String>>> = Rc::default();
    let next = Rc::clone(&own);
    let mut def = DataTypeDef::new()
        .member("Name", |_: &mut Recorder, _, _: &String, _| Some("named".into()))
        .any_member(&["Listed"], move |_, _, path, name, index| match name {
            "Gone" => None,
            "Panic" => panic!("catch-all panic"),
            _ if index.is_empty() => Some(next.get()?.value(format!("{path}.{name}"))),
            _ => Some(next.get()?.value(format!("{path}.{name}[{index}]"))),
        })
        .text(|_, _, path| path.clone());
    if let Some(parent) = parent {
        def = def.inherit("Counter", move |_, _, _| Some(parent.value(5)));
    }
    let remote = mq.add_type("Remote", def).unwrap();
    own.set(remote.clone()).unwrap();
    mq.add_tlo("Remote", move |_, _, index| Some(remote.value(index.to_owned()))).unwrap();
}

#[test]
fn a_catch_all_answers_names_it_never_listed() {
    let _main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| add_remote(mq, None));
    rt.initialize();
    assert_eq!(read("Remote[Bob].Me.Buff[3].Duration").as_deref(), Some("Bob.Me.Buff[3].Duration"));
    assert_eq!(read("Remote[Bob].Name").as_deref(), Some("named"), "a member of its own comes first");
    assert_eq!(read("Remote[Bob].me").as_deref(), Some("Bob.me"), "names arrive as written");
    assert_eq!(read("Remote[Bob].Panic"), None, "a panic is NULL");
}

#[test]
fn a_catch_all_lists_only_the_names_it_was_given_and_those_it_said_null_to() {
    let _main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| add_remote(mq, None));
    rt.initialize();
    let bob = fake_eval("Remote", "Bob").unwrap();
    assert!(listed(&bob, "Listed"), "Lua can read this one");
    assert!(!listed(&bob, "Me"), "Lua can't read this one");

    // Listed, so MacroQuest takes this NULL as a value and prints no error...
    assert_eq!(read("Remote[Bob].Gone"), None);
    assert!(listed(&bob, "Gone"));
    // ...and it still goes to the catch-all, not to a member with that ID.
    assert_eq!(read("Remote[Bob].Listed").as_deref(), Some("Bob.Listed"));
    assert_eq!(read("Remote[Bob].Gone"), None);

    // A value isn't listed: only a NULL needs to be.
    assert_eq!(read("Remote[Bob].Me").as_deref(), Some("Bob.Me"));
    assert!(!listed(&bob, "Me"));
}

/// Negative: what the catch-all's listing is there for. A type without one
/// leaves an unknown name off its list, which MacroQuest reports in red.
#[test]
fn without_a_catch_all_an_unknown_name_stays_unlisted() {
    let _main = FakeMain::install();
    runtime_with_counter();
    assert_eq!(read("Counter[5].Gone"), None);
    assert!(!listed(&fake_eval("Counter", "5").unwrap(), "Gone"));
}

#[test]
fn a_catch_all_comes_before_the_parent_and_null_falls_through_to_it() {
    let _main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        let counter = add_counter_type(mq);
        add_remote(mq, Some(counter));
    });
    rt.initialize();
    assert_eq!(read("Remote[Bob].Value").as_deref(), Some("Bob.Value"), "the catch-all answers first");
    // `Gone` isn't the parent's either, so it's NULL.
    assert_eq!(read("Remote[Bob].Gone"), None);
}

#[test]
fn a_catch_alls_names_are_checked_like_members() {
    let _main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| {
        let any = |names: &[&str]| DataTypeDef::<Recorder, i32>::new().member("Name", |_, _, _, _| None).any_member(names, |_, _, _, _, _| None);
        for (type_name, names) in [("A", &["1st"][..]), ("B", &["Twice", "Twice"]), ("C", &["Name"]), ("D", &["Fine"])] {
            plugin.log.push(format!("{type_name} {:?}", mq.add_type(type_name, any(names)).map(|_| ())));
        }
    });
    rt.initialize();
    let log = log_of(rt);
    assert_eq!(log[log.len() - 4..], [
        r#"A Err(InvalidMember("1st"))"#,
        r#"B Err(InvalidMember("Twice"))"#,
        r#"C Err(InvalidMember("Name"))"#,
        "D Ok(())",
    ]);
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

// ----- inheritance ---------------------------------------------------------------------

/// `Bot`, keyed by a name, whose parent value is the spawn of that name
/// (only Qtee, `Spawn[id 7]`, here).
fn add_bot_type(mq: &Mq<Recorder>) {
    let bot = mq
        .add_type(
            "Bot",
            DataTypeDef::new()
                .member("Role", |_: &mut Recorder, _, name: &String, _| Some(format!("{name} tanks").into()))
                .member("Name", |_, _, _, _| Some("own Name".into()))
                .inherit("spawn", |_, _, name| match name.as_str() {
                    "Qtee" => Some(Value::spawn(SpawnId(7))),
                    "Number" => Some(5.into()),
                    _ => None,
                }),
        )
        .unwrap();
    mq.add_tlo("Bot", move |_, _, index| Some(bot.value(index.to_owned()))).unwrap();
}

#[test]
fn inherited_members_come_from_the_parents_value() {
    let _main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| add_bot_type(mq));
    rt.initialize();
    assert_eq!(read("Bot[Qtee].Role").as_deref(), Some("Qtee tanks"));
    assert_eq!(read("Bot[Qtee].Level").as_deref(), Some("65"), "the spawn's");
    assert_eq!(read("Bot[Qtee].Buff[1].Name").as_deref(), Some("Aegolism"));
    assert_eq!(read("Bot[Qtee].Name").as_deref(), Some("own Name"), "its own members come first");
    assert_eq!(read("Bot[Qtee]").as_deref(), Some("Bot"), "and its own text");
    assert_eq!(read("Bot[Qtee].Nope"), None);
    assert_eq!(read("Bot[Nobody].Level"), None, "no parent value: NULL");
    assert_eq!(read("Bot[Number].Level"), None, "a value of another type: NULL");
    assert_eq!(live_data(), 0, "the spawns read are freed");
    assert_eq!(rt.stats().panics, 0);
}

#[test]
fn a_type_can_inherit_the_plugins_own_even_when_added_again() {
    let _main = FakeMain::install();
    let rt = runtime();
    // Big[n] is Counter[n * 10], whichever Counter type is there.
    let counter: Rc<RefCell<Option<DataType<i32>>>> = Rc::default();
    let current = Rc::clone(&counter);
    set_hook(&ON_NEW, move |_, mq| {
        *current.borrow_mut() = Some(add_counter_type(mq));
        let parent = Rc::clone(&current);
        let def = DataTypeDef::new().inherit("Counter", move |_: &mut Recorder, _, n: &i32| Some(parent.borrow().as_ref()?.value(n * 10)));
        let big = mq.add_type("Big", def).unwrap();
        mq.add_tlo("Big", move |_, _, index| Some(big.value(index.parse().ok()?))).unwrap();
    });
    rt.initialize();
    assert_eq!(read("Big[2].Value").as_deref(), Some("20"));
    assert_eq!(read("Big[2].Next.Value").as_deref(), Some("21"));

    set_hook(&ON_PULSE, |_, mq| assert!(mq.remove_type("Counter")));
    rt.pulse();
    assert_eq!(read("Big[2].Value"), None, "the parent is gone");

    let current = Rc::clone(&counter);
    set_hook(&ON_PULSE, move |_, mq| *current.borrow_mut() = Some(add_counter_type(mq)));
    rt.pulse();
    assert_eq!(read("Big[3].Value").as_deref(), Some("30"), "found again by name");
}

#[test]
fn parents_must_exist_and_be_macroquests_or_the_plugins() {
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
        for (name, parent) in [("A", "NoSuch"), ("B", "B"), ("C", "Taken"), ("D", "spawn")] {
            let def = DataTypeDef::<Recorder, i32>::new().inherit(parent, |_, _, _| None);
            plugin.log.push(format!("{name}: {:?}", mq.add_type(name, def).map(|ty| ty.name().to_owned())));
        }
    });
    assert_eq!(
        log_of(rt)[2..], // after "new" and "pulse"
        [
            format!("A: {:?}", Err::<String, _>(DataTypeError::NoSuchType("NoSuch".into()))),
            format!("B: {:?}", Err::<String, _>(DataTypeError::ParentNotAllowed("B".into()))),
            format!("C: {:?}", Err::<String, _>(DataTypeError::ParentNotAllowed("Taken".into()))),
            r#"D: Ok("D")"#.to_owned(),
        ]
    );
    // The refused types were taken out again and freed, their handlers dropped.
    assert_eq!(registered_names(), ["Taken", "D"]);
    assert_eq!((drops(), frees()), (3, 3));
}

// ----- extensions ----------------------------------------------------------------------

/// Adds `Threat`, `Twice`, `Home` and `Drop` to MacroQuest's `spawn`.
fn extend_spawn(mq: &Mq<Recorder>) -> Result<(), DataTypeError> {
    let extension = TypeExtension::new()
        .member("Threat", |plugin: &mut Recorder, _, spawn, index| {
            plugin.log.push(format!("threat {index:?}"));
            Some(format!("threat on {}", spawn.member("Name")?.text()).into())
        })
        .member("Twice", |_, _, spawn, _| Some((spawn.member("Level")?.as_int()? * 2).into()))
        .member("Home", |_, _, _, _| Value::path("Me").ok())
        .member("Drop", |_, mq, _, _| Some(mq.remove_type_extension("spawn").into()));
    mq.extend_type("spawn", extension)
}

#[test]
fn extensions_add_members_to_macroquests_types() {
    let _main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        extend_spawn(mq).unwrap();
        add_bot_type(mq);
    });
    rt.initialize();
    let plain = |value: Value| Some(FakeVal::Plain(value));
    assert_eq!(extension_member("spawn", "Qtee", "Threat", "x"), plain("threat on Qtee".into()));
    assert_eq!(extension_member("spawn", "Qtee", "Twice", ""), plain(130.into()));
    assert_eq!(extension_member("spawn", "Qtee", "Home", ""), plain(Value::Object { type_name: "spawn".into(), text: "Syzygy".into() }));
    assert_eq!(extension_member("buff", "Aegolism", "Threat", ""), None, "only the type extended");
    assert_eq!(extension_member("spawn", "Qtee", "Nope", ""), None);
    // A spawn reached any way has them: here, the parent of the plugin's own type.
    assert_eq!(read("Bot[Qtee].Threat").as_deref(), Some("threat on Qtee"));
    assert_eq!(log_of(rt)[1..], [r#"threat "x""#, r#"threat """#]);
    assert_eq!(live_data(), 0, "each value passed to a member is freed");
}

#[test]
fn extension_members_are_checked_and_one_extension_per_type() {
    let _main = FakeMain::install();
    let rt = in_pulse(|plugin, mq| {
        let none = |_: &mut Recorder, _: &Mq<Recorder>, _: &Data<'_>, _: &str| None;
        for (type_name, extension) in [
            ("nosuch", TypeExtension::new().member("A", none)),
            ("spawn", TypeExtension::new().member("Level", none)),
            ("spawn", TypeExtension::new().member("Bad Name", none)),
            ("spawn", TypeExtension::new().member("A", none).member("A", none)),
            ("spawn", TypeExtension::new().member("A", none)),
            ("spawn", TypeExtension::new().member("B", none)),
        ] {
            plugin.log.push(format!("{type_name}: {:?}", mq.extend_type(type_name, extension)));
        }
    });
    assert_eq!(
        log_of(rt)[2..],
        [
            format!("nosuch: {:?}", Err::<(), _>(DataTypeError::NoSuchType("nosuch".into()))),
            format!("spawn: {:?}", Err::<(), _>(DataTypeError::InvalidMember("Level".into()))),
            format!("spawn: {:?}", Err::<(), _>(DataTypeError::InvalidMember("Bad Name".into()))),
            format!("spawn: {:?}", Err::<(), _>(DataTypeError::InvalidMember("A".into()))),
            "spawn: Ok(())".to_owned(),
            format!("spawn: {:?}", Err::<(), _>(DataTypeError::AlreadyAdded)),
        ]
    );
    assert_eq!(registered_extensions(), ["spawn"]);
    // "nosuch" and "Level" reached the shim, which dropped their handlers; "Level"'s was freed.
    assert_eq!((drops(), extension_frees()), (2, 1));
}

#[test]
fn extending_without_macroquest_is_unavailable() {
    // No FakeMain: MacroQuest's API isn't there.
    let rt = in_pulse(|plugin, mq| plugin.log.push(format!("{:?}", extend_spawn(mq))));
    assert_eq!(log_of(rt).last().unwrap(), &format!("{:?}", Err::<(), _>(DataTypeError::Unavailable)));
    assert!(registered_extensions().is_empty());
}

#[test]
fn extensions_are_removed_and_freed_on_unload() {
    let _main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| extend_spawn(mq).unwrap());
    rt.initialize();
    set_hook(&ON_PULSE, |plugin, mq| {
        plugin.log.push(format!("{} {}", mq.remove_type_extension("spawn"), mq.remove_type_extension("spawn")));
    });
    rt.pulse();
    assert_eq!(log_of(rt).last().unwrap(), "true false");
    assert_eq!(extension_member("spawn", "Qtee", "Twice", ""), None);
    assert_eq!((drops(), extension_frees()), (1, 0), "kept until unload: its member may have been running");

    set_hook(&ON_PULSE, |_, mq| extend_spawn(mq).unwrap());
    rt.pulse();
    assert_eq!(extension_member("spawn", "Qtee", "Twice", ""), Some(FakeVal::Plain(130.into())), "added again");

    rt.shutdown();
    assert!(registered_extensions().is_empty(), "unloading removes the plugin's extensions");
    assert_eq!((drops(), extension_frees()), (2, 2), "each dropped and freed exactly once");
}

#[test]
fn an_extension_member_can_remove_its_own_extension() {
    let _main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| extend_spawn(mq).unwrap());
    rt.initialize();
    assert_eq!(extension_member("spawn", "Qtee", "Drop", ""), Some(FakeVal::Plain(true.into())));
    assert!(registered_extensions().is_empty());
    assert_eq!(drops(), 1);
    assert_eq!(rt.stats().panics, 0);
    assert_eq!(live_data(), 0);
}
