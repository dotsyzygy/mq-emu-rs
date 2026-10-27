//! MacroQuest's data: the top-level objects behind `${...}` (`Me`, `Target`,
//! `Spawn[...]`, `Zone`...), read as typed values, and TLOs of your own that
//! macros and Lua scripts can read (with datatypes of your own, see
//! [`Mq::add_type`]).
//!
//! ```ignore
//! let level = mq.tlo("Me").and_then(|me| me.member("Level")).and_then(|l| l.as_int());
//! let buff = mq.data("Me.Buff[1].Name")?.map(|name| name.text());
//!
//! mq.add_tlo("MyPlugin", |plugin: &mut MyPlugin, _mq, index| match index {
//!     "" => Some("ready".into()),
//!     "count" => Some(plugin.count.into()),
//!     _ => None, // NULL
//! })?;
//! ```
//!
//! Reading evaluates through MacroQuest's data system, the same way
//! `${Me.Level}` or Lua's `mq.TLO.Me.Level()` do. A [`Data`] value borrows
//! the [`Mq`] it came from: game data is only valid during the callback that
//! read it.

use std::ffi::{c_char, c_void};
use std::fmt;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use mq_emu_sys::plugin::MQPluginHandle;
use mq_emu_sys::shim::{self, MainInterface, mqemu_data, mqemu_scalar, mqemu_tlo_result, mqemu_type, tlo_kind};

use crate::datatypes::CustomValue;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::runtime::{KIND_TLO, Runtime};
use crate::text::{from_c, to_c};

/// A value from MacroQuest's data system, converted the way Lua's `mq.TLO`
/// converts it.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    /// `int`, `int64`, `byte` and `timestamp` values.
    Int(i64),
    /// `float` and `double` values.
    Float(f64),
    String(String),
    /// Any other type (`spawn`, `buff`...), as `${...}` shows it.
    Object { type_name: String, text: String },
    /// A value of one of the plugin's own datatypes, for its TLOs and
    /// members to return: see [`DataType::value`](crate::DataType::value).
    Custom(CustomValue),
}

impl Value {
    /// Converts raw data of MacroQuest type `type_name`.
    fn from_parts(type_name: &str, scalar: mqemu_scalar, text: impl FnOnce() -> String) -> Value {
        match type_name {
            // Old code stores ints as unsigned 32-bit patterns; MacroQuest reads them as int.
            "int" => Value::Int(i64::from(scalar.integer as i32)),
            "byte" => Value::Int(i64::from(scalar.integer as u8)),
            "int64" | "timestamp" => Value::Int(scalar.integer),
            "float" | "double" => Value::Float(scalar.real),
            // Bools are stored as bool or as a number, depending on the code that set them.
            "bool" => Value::Bool(scalar.boolean || scalar.integer != 0),
            "string" => Value::String(text()),
            _ => Value::Object { type_name: type_name.to_owned(), text: text() },
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }

    /// Floats, and ints converted.
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Int(n) => Some(*n as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    /// As `${...}` would show it. Floats are rounded to 2 places exactly;
    /// MacroQuest's own text can differ in the last digit in game (it formats
    /// with the FPU, which Direct3D runs at single precision). Use
    /// [`Data::text`] for MacroQuest's text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Bool(b) => f.write_str(if *b { "TRUE" } else { "FALSE" }),
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(x) => write!(f, "{x:.2}"),
            Value::String(s) => f.write_str(s),
            Value::Object { text, .. } => f.write_str(text),
            // Its text handler needs the plugin, so the type's name, as
            // MacroQuest shows a type without one.
            Value::Custom(custom) => f.write_str(custom.type_name()),
        }
    }
}

macro_rules! value_from {
    ($($t:ty => $variant:ident($conv:expr)),* $(,)?) => {$(
        impl From<$t> for Value {
            fn from(value: $t) -> Value {
                Value::$variant($conv(value))
            }
        }
    )*};
}
value_from!(
    bool => Bool(|b| b),
    i32 => Int(i64::from),
    u32 => Int(i64::from),
    i64 => Int(|n| n),
    u8 => Int(i64::from),
    f32 => Float(f64::from),
    f64 => Float(|x| x),
    &str => String(str::to_owned),
    String => String(|s| s),
);

/// The data operations, behind function pointers so tests can use a fake
/// (the real ones import MQ2Main.dll and eqlib.dll).
#[derive(Clone, Copy)]
pub(crate) struct DataApi {
    pub tlo: fn(*mut MainInterface, &str, &str) -> *mut mqemu_data,
    pub member: fn(*mut mqemu_data, &str, &str) -> *mut mqemu_data,
    pub free: fn(*mut mqemu_data),
    pub type_name: fn(*mut mqemu_data) -> String,
    pub text: fn(*mut mqemu_data) -> String,
    pub scalar: fn(*mut mqemu_data) -> mqemu_scalar,
    /// Ownership of the user pointer passes over; see `mqemu_add_tlo`.
    pub add_tlo: fn(*mut MainInterface, &MQPluginHandle, &str, shim::mqemu_tlo_fn, *mut c_void, shim::mqemu_drop_fn) -> bool,
    pub remove_tlo: fn(*mut MainInterface, &MQPluginHandle, &str) -> bool,
    pub set_result: fn(*mut mqemu_tlo_result, &Value) -> bool,
    /// Ownership of the user pointer passes over; see `mqemu_add_type`.
    pub add_type: fn(&MQPluginHandle, &str, i32, shim::mqemu_member_fn, shim::mqemu_type_text_fn, *mut c_void, shim::mqemu_drop_fn) -> *mut mqemu_type,
    pub type_add_member: fn(*mut mqemu_type, i32, &str, bool) -> bool,
    pub remove_type: fn(*mut mqemu_type),
    pub free_type: fn(*mut mqemu_type),
}

/// The real data API; generic for the reason given in `host`.
pub(crate) const fn real<P>() -> DataApi {
    DataApi {
        tlo: |main, name, index| unsafe { shim::mqemu_data_tlo(main, to_c(name).as_ptr(), to_c(index).as_ptr()) },
        member: |data, name, index| unsafe { shim::mqemu_data_member(data, to_c(name).as_ptr(), to_c(index).as_ptr()) },
        free: |data| unsafe { shim::mqemu_data_free(data) },
        type_name: |data| unsafe { from_c(shim::mqemu_data_type_name(data)) }.into_owned(),
        text: |data| {
            let mut buffer = vec![0 as c_char; mq_emu_sys::constants::MAX_STRING];
            let len = unsafe { shim::mqemu_data_text(data, buffer.as_mut_ptr(), buffer.len()) };
            String::from_utf8_lossy(&buffer[..len].iter().map(|c| *c as u8).collect::<Vec<_>>()).into_owned()
        },
        scalar: |data| {
            let mut out = mqemu_scalar::default();
            unsafe { shim::mqemu_data_scalar(data, &mut out) };
            out
        },
        add_tlo: |main, handle, name, callback, user, drop| unsafe {
            shim::mqemu_add_tlo(main, handle, to_c(name).as_ptr(), callback, user, Some(drop))
        },
        remove_tlo: |main, handle, name| unsafe { shim::mqemu_remove_tlo(main, handle, to_c(name).as_ptr()) },
        set_result: real_set_result::<P>,
        add_type: |handle, name, key_kind, member, text, user, drop| unsafe {
            shim::mqemu_add_type(handle, to_c(name).as_ptr(), key_kind, member, text, user, Some(drop))
        },
        type_add_member: |ty, id, name, method| unsafe { shim::mqemu_type_add_member(ty, id, to_c(name).as_ptr(), method) },
        remove_type: |ty| unsafe { shim::mqemu_remove_type(ty) },
        free_type: |ty| unsafe { shim::mqemu_free_type(ty) },
    }
}

#[allow(clippy::extra_unused_type_parameters)] // generic on purpose; see `host`
fn real_set_result<P>(result: *mut mqemu_tlo_result, value: &Value) -> bool {
    let set = |kind, integer, real, text: &str| unsafe { shim::mqemu_tlo_set(result, kind, integer, real, text.as_ptr().cast(), text.len()) };
    match value {
        Value::Bool(b) => set(tlo_kind::BOOL, i64::from(*b), 0.0, ""),
        Value::Int(n) if i32::try_from(*n).is_ok() => set(tlo_kind::INT, *n, 0.0, ""),
        Value::Int(n) => set(tlo_kind::INT64, *n, 0.0, ""),
        Value::Float(x) => set(tlo_kind::FLOAT, 0, *x, ""),
        Value::String(s) => set(tlo_kind::STRING, 0, 0.0, s),
        Value::Object { .. } => false,
        Value::Custom(custom) => {
            let ty = custom.raw_type();
            !ty.is_null() && custom.with_c_key(|key| unsafe { shim::mqemu_tlo_set_key(result, ty, key) })
        }
    }
}

/// A value read from MacroQuest's data system: a TLO, or a member of one.
/// Only valid during the callback that read it (it borrows the [`Mq`]).
pub struct Data<'a> {
    raw: *mut mqemu_data,
    api: &'a DataApi,
    _not_send: PhantomData<*mut ()>,
}

impl<'a> Data<'a> {
    fn new(raw: *mut mqemu_data, api: &'a DataApi) -> Option<Data<'a>> {
        (!raw.is_null()).then(|| Data { raw, api, _not_send: PhantomData })
    }

    /// `.Member`; None if the type has no such member or it's NULL.
    pub fn member(&self, name: &str) -> Option<Data<'a>> {
        self.member_at(name, "")
    }

    /// `.Member[index]`.
    pub fn member_at(&self, name: &str, index: impl AsRef<str>) -> Option<Data<'a>> {
        Data::new((self.api.member)(self.raw, name, index.as_ref()), self.api)
    }

    /// MacroQuest's name for the type: `"int"`, `"string"`, `"spawn"`...
    pub fn type_name(&self) -> String {
        (self.api.type_name)(self.raw)
    }

    /// As `${...}` shows it.
    pub fn text(&self) -> String {
        (self.api.text)(self.raw)
    }

    pub fn value(&self) -> Value {
        Value::from_parts(&self.type_name(), (self.api.scalar)(self.raw), || self.text())
    }

    pub fn as_int(&self) -> Option<i64> {
        self.value().as_int()
    }

    pub fn as_float(&self) -> Option<f64> {
        self.value().as_float()
    }

    pub fn as_bool(&self) -> Option<bool> {
        self.value().as_bool()
    }

    /// The text of a `string` value; None for other types (use [`Data::text`]).
    pub fn as_string(&self) -> Option<String> {
        match self.value() {
            Value::String(s) => Some(s),
            _ => None,
        }
    }
}

impl Drop for Data<'_> {
    fn drop(&mut self) {
        (self.api.free)(self.raw);
    }
}

impl fmt::Debug for Data<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Data").field(&self.value()).finish()
    }
}

/// Why a data path couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathError {
    pub path: String,
    pub reason: &'static str,
}

impl fmt::Display for PathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bad data path {:?}: {}", self.path, self.reason)
    }
}

impl std::error::Error for PathError {}

/// Splits `Me.Buff[1].Name` into `[("Me", ""), ("Buff", "1"), ("Name", "")]`.
/// Indexes may contain dots and nested brackets (`Spawn[npc radius 5.5]`).
pub(crate) fn parse_path(path: &str) -> Result<Vec<(&str, &str)>, PathError> {
    let error = |reason| PathError { path: path.to_owned(), reason };
    let mut steps = Vec::new();
    let mut rest = path;
    loop {
        let name_end = rest.find(['.', '[', ']']).unwrap_or(rest.len());
        let name = &rest[..name_end];
        if name.is_empty() {
            return Err(error("empty name"));
        }
        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(error("names are letters, digits and _"));
        }
        rest = &rest[name_end..];
        let mut index = "";
        if let Some(after) = rest.strip_prefix('[') {
            let mut depth = 1;
            let close = after.char_indices().find_map(|(i, c)| {
                match c {
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    _ => {}
                }
                (depth == 0).then_some(i)
            });
            let close = close.ok_or_else(|| error("unclosed ["))?;
            index = &after[..close];
            rest = &after[close + 1..];
        }
        steps.push((name, index));
        match rest.strip_prefix('.') {
            Some(next) => rest = next,
            None if rest.is_empty() => return Ok(steps),
            None if rest.starts_with(']') => return Err(error("unmatched ]")),
            None => return Err(error("expected . after ]")),
        }
    }
}

// ----- TLOs provided by the plugin ----------------------------------------------------

pub(crate) type TloHandler<P> = dyn Fn(&mut P, &Mq<P>, &str) -> Option<Value>;

/// One TLO, owned by the C++ side (dropped when MacroQuest lets go of it).
pub(crate) struct TloSlot<P: Plugin> {
    handler: Rc<TloHandler<P>>,
    runtime: &'static Runtime<P>,
}

unsafe extern "C" fn tlo_call<P: Plugin>(user: *mut c_void, index: *const c_char, result: *mut mqemu_tlo_result) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: `user` is the slot's Rc, alive until tlo_drop.
        let slot = unsafe { &*user.cast::<TloSlot<P>>() };
        let index = unsafe { from_c(index) };
        match slot.runtime.call_data(KIND_TLO, |plugin, mq| (slot.handler)(plugin, mq, &index)) {
            Some(value) => (slot.runtime.host().data.set_result)(result, &value),
            None => false,
        }
    }))
    .unwrap_or(false)
}

unsafe extern "C" fn tlo_drop<P: Plugin>(user: *mut c_void) {
    drop(unsafe { Rc::from_raw(user.cast::<TloSlot<P>>()) });
}

/// Why a TLO couldn't be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TloError {
    /// Names are letters, digits and `_`, starting with a letter.
    InvalidName,
    /// This plugin already added a TLO by that name.
    AlreadyAdded,
    /// MacroQuest refused it (e.g. another plugin has that name).
    Rejected,
    /// MacroQuest's API isn't available.
    Unavailable,
}

impl fmt::Display for TloError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            TloError::InvalidName => "TLO names are letters, digits and _, starting with a letter",
            TloError::AlreadyAdded => "this plugin already added that TLO",
            TloError::Rejected => "MacroQuest refused the TLO (is the name taken?)",
            TloError::Unavailable => "MacroQuest's API is unavailable",
        })
    }
}

impl std::error::Error for TloError {}

pub(crate) fn valid_tlo_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl<P: Plugin> Runtime<P> {
    pub(crate) fn add_tlo(&'static self, name: &str, handler: Rc<TloHandler<P>>) -> Result<(), TloError> {
        if !valid_tlo_name(name) {
            return Err(TloError::InvalidName);
        }
        if self.tlos().borrow().iter().any(|n| n.eq_ignore_ascii_case(name)) {
            return Err(TloError::AlreadyAdded);
        }
        let main = (self.host().main_interface)();
        if main.is_null() {
            return Err(TloError::Unavailable);
        }
        let user = Rc::into_raw(Rc::new(TloSlot { handler, runtime: self })).cast_mut().cast::<c_void>();
        // Ownership of `user` passes over even if this fails.
        if !(self.host().data.add_tlo)(main, &self.handle(), name, tlo_call::<P>, user, tlo_drop::<P>) {
            return Err(TloError::Rejected);
        }
        self.tlos().borrow_mut().push(name.to_owned());
        Ok(())
    }

    pub(crate) fn remove_tlo(&self, name: &str) -> bool {
        let position = self.tlos().borrow().iter().position(|n| n.eq_ignore_ascii_case(name));
        let Some(position) = position else { return false };
        let name = self.tlos().borrow_mut().remove(position);
        let main = (self.host().main_interface)();
        !main.is_null() && (self.host().data.remove_tlo)(main, &self.handle(), &name)
    }

    pub(crate) fn remove_all_tlos(&self) {
        let names: Vec<_> = self.tlos().borrow_mut().drain(..).collect();
        let main = (self.host().main_interface)();
        if !main.is_null() {
            for name in names {
                (self.host().data.remove_tlo)(main, &self.handle(), &name);
            }
        }
    }
}

impl<P: Plugin> Mq<P> {
    /// Evaluates a top-level object (`${Me}`); None if there's none by that
    /// name or it's NULL.
    pub fn tlo(&self, name: &str) -> Option<Data<'_>> {
        self.tlo_at(name, "")
    }

    /// Evaluates a top-level object with an index (`${Spawn[npc radius 50]}`).
    pub fn tlo_at(&self, name: &str, index: impl AsRef<str>) -> Option<Data<'_>> {
        let main = (self.host().main_interface)();
        if main.is_null() {
            return None;
        }
        let api = &self.host().data;
        Data::new((api.tlo)(main, name, index.as_ref()), api)
    }

    /// Reads a dotted path like `${...}` without the `${}`: `"Me.Buff[1].Name"`.
    /// `Ok(None)` if any step is NULL or doesn't exist. Indexes are passed as
    /// written; evaluate any `${...}` inside them with [`Mq::parse`] first.
    pub fn data(&self, path: &str) -> Result<Option<Data<'_>>, PathError> {
        let steps = parse_path(path)?;
        let ((name, index), members) = steps.split_first().expect("parse_path returns at least one step");
        let mut current = self.tlo_at(name, index);
        for (member, index) in members {
            current = match current {
                Some(data) => data.member_at(member, index),
                None => return Ok(None),
            };
        }
        Ok(current)
    }

    /// Adds a top-level object that macros and Lua read as `${Name}` or
    /// `${Name[index]}` (`mq.TLO.Name(index)()`): `handler` gets the index
    /// (`""` for none) and returns the value, or None for NULL. It's removed
    /// when the plugin unloads. A value with members is one of the plugin's
    /// own datatypes ([`Mq::add_type`]); MacroQuest's ([`Value::Object`])
    /// can't be returned (gives NULL).
    ///
    /// While one of this plugin's callbacks runs, the plugin can't be
    /// borrowed again, so its own TLOs read as NULL from inside it (e.g.
    /// `mq.parse("${Name}")`); macros and scripts aren't affected.
    pub fn add_tlo<F>(&self, name: &str, handler: F) -> Result<(), TloError>
    where
        F: Fn(&mut P, &Mq<P>, &str) -> Option<Value> + 'static,
    {
        self.runtime().add_tlo(name, Rc::new(handler))
    }

    /// Removes a TLO added with [`Mq::add_tlo`]. False if there was none.
    pub fn remove_tlo(&self, name: &str) -> bool {
        self.runtime().remove_tlo(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_split_into_steps() {
        assert_eq!(parse_path("Me").unwrap(), [("Me", "")]);
        assert_eq!(parse_path("Me.Buff[1].Name").unwrap(), [("Me", ""), ("Buff", "1"), ("Name", "")]);
        assert_eq!(parse_path("Spawn[npc radius 5.5].Name").unwrap(), [("Spawn", "npc radius 5.5"), ("Name", "")]);
        assert_eq!(parse_path("Ini[a[b]c].Length").unwrap(), [("Ini", "a[b]c"), ("Length", "")]);
        assert_eq!(parse_path("Me.Buff[]").unwrap(), [("Me", ""), ("Buff", "")]);
    }

    #[test]
    fn bad_paths_are_rejected() {
        for (path, reason) in [
            ("", "empty name"),
            ("Me.", "empty name"),
            ("Me..Name", "empty name"),
            (".Me", "empty name"),
            ("Me.Buff[1", "unclosed ["),
            ("Me.Buff[1]x", "expected . after ]"),
            ("Me]", "unmatched ]"),
            ("Me.Buff[1]]", "unmatched ]"),
            ("${Me}", "names are letters, digits and _"),
            ("Me Name", "names are letters, digits and _"),
        ] {
            assert_eq!(parse_path(path).map_err(|e| e.reason), Err(reason), "{path:?}");
        }
    }

    fn scalar(integer: i64, real: f64, boolean: bool) -> mqemu_scalar {
        mqemu_scalar { integer, real, boolean }
    }

    #[test]
    fn values_convert_by_type() {
        let none = || -> String { panic!("text read for a number") };
        assert_eq!(Value::from_parts("int", scalar(65, 65.0, true), none), Value::Int(65));
        // An int stored as an unsigned 32-bit pattern reads back negative.
        assert_eq!(Value::from_parts("int", scalar(0xFFFF_FFFB, 0.0, true), none), Value::Int(-5));
        assert_eq!(Value::from_parts("int64", scalar(1 << 40, 0.0, true), none), Value::Int(1 << 40));
        assert_eq!(Value::from_parts("byte", scalar(0x1FF, 0.0, true), none), Value::Int(255));
        assert_eq!(Value::from_parts("float", scalar(-12, -12.5, true), none), Value::Float(-12.5));
        assert_eq!(Value::from_parts("double", scalar(0, 0.25, true), none), Value::Float(0.25));
        // Bools set as a number or as a bool.
        assert_eq!(Value::from_parts("bool", scalar(1, 1.0, false), none), Value::Bool(true));
        assert_eq!(Value::from_parts("bool", scalar(0, 0.0, true), none), Value::Bool(true));
        assert_eq!(Value::from_parts("bool", scalar(0, 0.0, false), none), Value::Bool(false));
        assert_eq!(Value::from_parts("string", scalar(0, 0.0, false), || "Syzygy".into()), Value::String("Syzygy".into()));
        assert_eq!(
            Value::from_parts("spawn", scalar(0x1234, 0.0, true), || "Syzygy".into()),
            Value::Object { type_name: "spawn".into(), text: "Syzygy".into() }
        );
    }

    #[test]
    fn values_display_like_macroquest() {
        assert_eq!(Value::Bool(true).to_string(), "TRUE");
        assert_eq!(Value::Bool(false).to_string(), "FALSE");
        assert_eq!(Value::Float(1.23456).to_string(), "1.23");
        assert_eq!(Value::Int(-5).to_string(), "-5");
        assert_eq!(Value::from("hi").to_string(), "hi");
    }

    #[test]
    fn tlo_names() {
        assert!(valid_tlo_name("RustTest"));
        assert!(valid_tlo_name("My_TLO2"));
        assert!(!valid_tlo_name(""));
        assert!(!valid_tlo_name("2Fast"));
        assert!(!valid_tlo_name("Has Space"));
        assert!(!valid_tlo_name("Me.Name"));
        assert!(!valid_tlo_name("_x"));
    }
}
