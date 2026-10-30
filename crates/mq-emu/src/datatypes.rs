//! Datatypes of the plugin's own: values with members, which the plugin's
//! TLOs (and members) return and macros and Lua scripts read, as a C++
//! plugin's `MQ2Type` or Lua's `mq.DataType`.
//!
//! ```ignore
//! let bot = mq.add_type(
//!     "Bot",
//!     DataTypeDef::new()
//!         .member("Name", |_, _, name: &String, _| Some(name.as_str().into()))
//!         .member("HP", |plugin: &mut MyPlugin, _, name, _| Some(plugin.bots.get(name)?.hp.into()))
//!         .method("Stop", |plugin, mq, name, _| plugin.stop(mq, name))
//!         .text(|_, _, name| name.clone()),
//! )?;
//! mq.add_tlo("Bot", move |plugin: &mut MyPlugin, _, index| {
//!     plugin.bots.contains_key(index).then(|| bot.value(index.to_owned()))
//! })?;
//! // ${Bot[Tank].HP}, mq.TLO.Bot('Tank').HP(), mq.TLO.Bot('Tank').Stop()
//! ```
//!
//! A value of a datatype is a key (a number or text, see [`TypeKey`]) that
//! its members look up in the plugin, as MacroQuest's own `spawn` holds a
//! spawn and `buff` a slot: what a member says is what the plugin has when
//! it's read. Keys are all MacroQuest keeps, so it can free a value without
//! the plugin, even after the plugin unloads.
//!
//! A datatype can inherit another's members ([`DataTypeDef::inherit`]): a
//! `Bot` that is a spawn has `${Bot[Tank].PctHPs}` as well as its own. And
//! the plugin can add members to a type that isn't its own
//! ([`Mq::extend_type`]), such as MacroQuest's `spawn`:
//!
//! ```ignore
//! mq.extend_type("spawn", TypeExtension::new().member("Threat", |plugin: &mut MyPlugin, _, spawn, _| {
//!     let id = spawn.member("ID")?.as_int()?;
//!     Some(plugin.threat.get(&id).copied().unwrap_or(0).into())
//! }))?;
//! // ${Target.Threat}, ${Spawn[npc].Threat}, mq.TLO.Me.Threat()
//! ```
//!
//! A datatype can also answer names it can't list in advance
//! ([`DataTypeDef::any_member`]).

use std::cell::Cell;
use std::ffi::{c_char, c_void};
use std::fmt;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::rc::Rc;

use mq_emu_sys::shim::{ANY_MEMBER, inherit, key_kind, mqemu_data, mqemu_key, mqemu_tlo_result, mqemu_type};

use crate::data::{Data, Value, valid_tlo_name};
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::runtime::{KIND_DATATYPE, Runtime};
use crate::text::from_c;

mod sealed {
    /// A value's key as MacroQuest holds it.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum RawKey {
        Int(i64),
        Text(String),
    }

    pub trait Sealed: Sized {
        /// The `key_kind` MacroQuest stores, for `/varset` from text.
        const KIND: i32;
        fn to_raw(&self) -> RawKey;
        /// None for a key that isn't one of these (a Lua script can return
        /// any value as the type).
        fn from_raw(raw: &RawKey) -> Option<Self>;
    }
}

use sealed::{RawKey, Sealed};

/// What a value of a datatype holds, for its members to look up: `()` for a
/// type with one value, an integer (`i32`, `u32`, `i64`) or `String`.
pub trait TypeKey: Sealed + 'static {}

impl Sealed for () {
    const KIND: i32 = key_kind::INT;
    fn to_raw(&self) -> RawKey {
        RawKey::Int(0)
    }
    fn from_raw(raw: &RawKey) -> Option<()> {
        matches!(raw, RawKey::Int(_)).then_some(())
    }
}
impl TypeKey for () {}

macro_rules! int_keys {
    ($($t:ty),*) => {$(
        impl Sealed for $t {
            const KIND: i32 = key_kind::INT;
            fn to_raw(&self) -> RawKey {
                RawKey::Int(i64::from(*self))
            }
            fn from_raw(raw: &RawKey) -> Option<$t> {
                match raw {
                    RawKey::Int(n) => <$t>::try_from(*n).ok(),
                    RawKey::Text(_) => None,
                }
            }
        }
        impl TypeKey for $t {}
    )*};
}
int_keys!(i32, u32, i64);

impl Sealed for String {
    const KIND: i32 = key_kind::TEXT;
    fn to_raw(&self) -> RawKey {
        RawKey::Text(self.clone())
    }
    fn from_raw(raw: &RawKey) -> Option<String> {
        match raw {
            RawKey::Text(s) => Some(s.clone()),
            RawKey::Int(_) => None,
        }
    }
}
impl TypeKey for String {}

impl RawKey {
    /// # Safety
    /// `key` must be null or valid, with `len` bytes at `text` for text.
    unsafe fn from_c(key: *const mqemu_key) -> Option<RawKey> {
        let key = unsafe { key.as_ref() }?;
        match key.kind {
            key_kind::INT => Some(RawKey::Int(key.integer)),
            key_kind::TEXT if key.text.is_null() => Some(RawKey::Text(String::new())),
            key_kind::TEXT => {
                let bytes = unsafe { std::slice::from_raw_parts(key.text.cast::<u8>(), key.len) };
                Some(RawKey::Text(String::from_utf8_lossy(bytes).into_owned()))
            }
            _ => None,
        }
    }
}

/// A datatype as values and the runtime see it.
pub(crate) struct TypeShared {
    name: String,
    /// Null once removed.
    raw: Cell<*mut mqemu_type>,
}

/// A value of one of the plugin's datatypes ([`Value::Custom`]).
#[derive(Clone)]
pub struct CustomValue {
    ty: Rc<TypeShared>,
    key: RawKey,
}

impl CustomValue {
    /// The datatype's name.
    pub fn type_name(&self) -> &str {
        &self.ty.name
    }

    /// The key it holds, if it's a `K`.
    pub fn key<K: TypeKey>(&self) -> Option<K> {
        K::from_raw(&self.key)
    }

    pub(crate) fn raw_type(&self) -> *mut mqemu_type {
        self.ty.raw.get()
    }

    pub(crate) fn with_c_key<R>(&self, f: impl FnOnce(&mqemu_key) -> R) -> R {
        let key = match &self.key {
            RawKey::Int(n) => mqemu_key { kind: key_kind::INT, integer: *n, text: ptr::null(), len: 0 },
            RawKey::Text(s) => mqemu_key { kind: key_kind::TEXT, integer: 0, text: s.as_ptr().cast(), len: s.len() },
        };
        f(&key)
    }
}

/// The same datatype (not just the same name) and key.
impl PartialEq for CustomValue {
    fn eq(&self, other: &CustomValue) -> bool {
        Rc::ptr_eq(&self.ty, &other.ty) && self.key == other.key
    }
}

impl fmt::Debug for CustomValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("CustomValue");
        s.field("type_name", &self.ty.name);
        match &self.key {
            RawKey::Int(n) => s.field("key", n),
            RawKey::Text(text) => s.field("key", text),
        };
        s.finish()
    }
}

/// A datatype the plugin added ([`Mq::add_type`]), to make values of.
pub struct DataType<K: TypeKey> {
    shared: Rc<TypeShared>,
    _key: PhantomData<fn(K) -> K>,
}

impl<K: TypeKey> DataType<K> {
    /// A value of this type holding `key`, for a TLO or member to return.
    /// Once the type is removed, returning it gives NULL.
    pub fn value(&self, key: K) -> Value {
        Value::Custom(CustomValue { ty: Rc::clone(&self.shared), key: key.to_raw() })
    }

    pub fn name(&self) -> &str {
        &self.shared.name
    }
}

impl<K: TypeKey> Clone for DataType<K> {
    fn clone(&self) -> Self {
        DataType { shared: Rc::clone(&self.shared), _key: PhantomData }
    }
}

impl<K: TypeKey> fmt::Debug for DataType<K> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("DataType").field(&self.shared.name).finish()
    }
}

type MemberHandler<P> = dyn Fn(&mut P, &Mq<P>, &RawKey, &str) -> Option<Value>;
type AnyMemberHandler<P> = dyn Fn(&mut P, &Mq<P>, &RawKey, &str, &str) -> Option<Value>;
type TextHandler<P> = dyn Fn(&mut P, &Mq<P>, &RawKey) -> Option<String>;
type DowncastHandler<P> = dyn Fn(&mut P, &Mq<P>, &RawKey) -> Option<Value>;

struct MemberDef<P: Plugin> {
    name: String,
    method: bool,
    handler: Box<MemberHandler<P>>,
}

/// A datatype's members and text, for [`Mq::add_type`]. Handlers get the
/// plugin, the value's key and the member's index (`""` for none).
pub struct DataTypeDef<P: Plugin, K: TypeKey> {
    members: Vec<MemberDef<P>>,
    any_member: Option<(Vec<String>, Box<AnyMemberHandler<P>>)>,
    text: Option<Box<TextHandler<P>>>,
    parent: Option<(String, Box<DowncastHandler<P>>)>,
    _key: PhantomData<fn(K)>,
}

impl<P: Plugin, K: TypeKey> DataTypeDef<P, K> {
    /// No members yet, shown as the type's name.
    pub fn new() -> DataTypeDef<P, K> {
        DataTypeDef { members: Vec::new(), any_member: None, text: None, parent: None, _key: PhantomData }
    }

    /// Inherits the members of datatype `parent` (`"spawn"`, `"item"`...),
    /// as MacroQuest's `character` inherits `spawn`'s: a member this type
    /// doesn't have is read from the parent's value `downcast` gives for the
    /// key (None, or a value of another type, makes it NULL). A macro
    /// variable of the parent type can be set from a value of this one
    /// (`/vardata`), through the same `downcast`.
    ///
    /// ```ignore
    /// DataTypeDef::new()
    ///     .member("Role", |plugin: &mut MyPlugin, _, id: &u32, _| Some(plugin.bots.get(id)?.role.as_str().into()))
    ///     .inherit("spawn", |_, _, id| Some(Value::spawn(SpawnId(*id))))
    /// // ${Bot[Tank].Role}, and ${Bot[Tank].PctHPs} from the spawn
    /// ```
    ///
    /// The parent is one of MacroQuest's own types or one of this plugin's
    /// (found again by name each time, so it may be removed and added back):
    /// another plugin's could be unloaded while this one points at it.
    /// Members only: text is this type's own ([`DataTypeDef::text`]).
    pub fn inherit<F>(mut self, parent: &str, downcast: F) -> DataTypeDef<P, K>
    where
        F: Fn(&mut P, &Mq<P>, &K) -> Option<Value> + 'static,
    {
        let downcast = move |plugin: &mut P, mq: &Mq<P>, key: &RawKey| downcast(plugin, mq, &K::from_raw(key)?);
        self.parent = Some((parent.to_owned(), Box::new(downcast)));
        self
    }

    /// `${Value.Name}` or `${Value.Name[index]}`: the value, or None for
    /// NULL. Names are letters, digits and `_`, starting with a letter, and
    /// matched with their case, as MacroQuest's own.
    pub fn member<F>(mut self, name: &str, handler: F) -> DataTypeDef<P, K>
    where
        F: Fn(&mut P, &Mq<P>, &K, &str) -> Option<Value> + 'static,
    {
        let handler = move |plugin: &mut P, mq: &Mq<P>, key: &RawKey, index: &str| handler(plugin, mq, &K::from_raw(key)?, index);
        self.members.push(MemberDef { name: name.to_owned(), method: false, handler: Box::new(handler) });
        self
    }

    /// A method: an action, as `${Me.Buff[1].Remove}` or Lua's
    /// `mq.TLO.Me.Buff(1).Remove()`. A macro reading it gets the value it
    /// was called on, as with MacroQuest's methods; Lua gets nil.
    pub fn method<F>(mut self, name: &str, handler: F) -> DataTypeDef<P, K>
    where
        F: Fn(&mut P, &Mq<P>, &K, &str) + 'static,
    {
        let handler = move |plugin: &mut P, mq: &Mq<P>, key: &RawKey, index: &str| {
            handler(plugin, mq, &K::from_raw(key)?, index);
            Some(Value::Bool(true))
        };
        self.members.push(MemberDef { name: name.to_owned(), method: true, handler: Box::new(handler) });
        self
    }

    /// Answers every member the type has no [`member`](Self::member) or
    /// [`method`](Self::method) for, whatever its name, for a type whose paths
    /// can't all be known in advance. The handler also gets the name, as
    /// written (MacroQuest doesn't change its case). None makes it NULL, or the
    /// parent's member with [`inherit`](Self::inherit).
    ///
    /// ```ignore
    /// DataTypeDef::new().any_member(&["PctHPs", "Buff", "Duration"], |plugin: &mut MyPlugin, _, path: &String, name, index| {
    ///     plugin.remote.lookup(path, name, index)
    /// })
    /// // ${Remote[Bob].Buff[3].Duration}
    /// ```
    ///
    /// Macros can read any name, but Lua (`mq.TLO`) only the ones in `names`:
    /// it asks MacroQuest's list of the type's members before reading one and
    /// gets nil if the name isn't there, so a name not listed is unreadable
    /// from Lua however the handler would answer. They follow the rules for
    /// [`member`](Self::member)'s names, and can't be a member's. A name a
    /// macro has read and got NULL for joins the list, so that MacroQuest
    /// treats the NULL as a value rather than printing "No such member".
    pub fn any_member<F>(mut self, names: &[&str], handler: F) -> DataTypeDef<P, K>
    where
        F: Fn(&mut P, &Mq<P>, &K, &str, &str) -> Option<Value> + 'static,
    {
        let handler = move |plugin: &mut P, mq: &Mq<P>, key: &RawKey, name: &str, index: &str| handler(plugin, mq, &K::from_raw(key)?, name, index);
        self.any_member = Some((names.iter().map(|name| (*name).to_owned()).collect(), Box::new(handler)));
        self
    }

    /// How a value shows as text (`${Value}`, Lua's `tostring`); without
    /// this, as the type's name, as MacroQuest shows a type without one.
    pub fn text<F>(mut self, handler: F) -> DataTypeDef<P, K>
    where
        F: Fn(&mut P, &Mq<P>, &K) -> String + 'static,
    {
        self.text = Some(Box::new(move |plugin: &mut P, mq: &Mq<P>, key: &RawKey| Some(handler(plugin, mq, &K::from_raw(key)?))));
        self
    }
}

impl<P: Plugin, K: TypeKey> Default for DataTypeDef<P, K> {
    fn default() -> Self {
        DataTypeDef::new()
    }
}

/// Why a datatype couldn't be added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataTypeError {
    /// Names are letters, digits and `_`, starting with a letter.
    InvalidName,
    /// This member's name isn't valid or is used twice.
    InvalidMember(String),
    /// This plugin already added a datatype by that name.
    AlreadyAdded,
    /// MacroQuest refused it (a datatype by that name exists).
    Rejected,
    /// There's no datatype by this name to inherit from or extend.
    NoSuchType(String),
    /// The parent is the type itself, or another plugin's (or Lua's) type,
    /// which could be unloaded while this one points at it.
    ParentNotAllowed(String),
    /// MacroQuest's API isn't available.
    Unavailable,
}

impl fmt::Display for DataTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataTypeError::InvalidName => f.write_str("datatype names are letters, digits and _, starting with a letter"),
            DataTypeError::InvalidMember(name) => write!(f, "member {name:?} isn't a valid name or is used twice"),
            DataTypeError::AlreadyAdded => f.write_str("this plugin already added that datatype"),
            DataTypeError::Rejected => f.write_str("MacroQuest refused the datatype (is the name taken?)"),
            DataTypeError::NoSuchType(name) => write!(f, "there's no datatype {name:?}"),
            DataTypeError::ParentNotAllowed(name) => {
                write!(f, "{name:?} can't be a parent: only MacroQuest's datatypes and the plugin's own can")
            }
            DataTypeError::Unavailable => f.write_str("MacroQuest's API is unavailable"),
        }
    }
}

impl std::error::Error for DataTypeError {}

/// One datatype's handlers, owned by the C++ side until the type is removed.
struct TypeSlot<P: Plugin> {
    name: String,
    /// By member ID.
    members: Vec<MemberDef<P>>,
    any_member: Option<Box<AnyMemberHandler<P>>>,
    text: Option<Box<TextHandler<P>>>,
    downcast: Option<Box<DowncastHandler<P>>>,
    runtime: &'static Runtime<P>,
}

impl<P: Plugin> TypeSlot<P> {
    /// The slot behind a callback's `user`, held for the call: a handler may
    /// remove its own type.
    ///
    /// # Safety
    /// `user` must be the slot's Rc, alive until `type_drop`.
    unsafe fn hold(user: *mut c_void) -> Rc<TypeSlot<P>> {
        let user = user.cast::<TypeSlot<P>>().cast_const();
        unsafe {
            Rc::increment_strong_count(user);
            Rc::from_raw(user)
        }
    }
}

unsafe extern "C" fn member_call<P: Plugin>(
    user: *mut c_void,
    key: *const mqemu_key,
    member: i32,
    index: *const c_char,
    result: *mut mqemu_tlo_result,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        let slot = unsafe { TypeSlot::<P>::hold(user) };
        let Some(member) = usize::try_from(member).ok().and_then(|id| slot.members.get(id)) else { return false };
        let Some(key) = (unsafe { RawKey::from_c(key) }) else { return false };
        let index = unsafe { from_c(index) };
        match slot.runtime.call_data(KIND_DATATYPE, |plugin, mq| (member.handler)(plugin, mq, &key, &index)) {
            Some(_) if member.method => true,
            Some(value) => slot.runtime.set_value(result, &value),
            None => false,
        }
    }))
    .unwrap_or(false)
}

unsafe extern "C" fn any_member_call<P: Plugin>(
    user: *mut c_void,
    key: *const mqemu_key,
    member: *const c_char,
    index: *const c_char,
    result: *mut mqemu_tlo_result,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        let slot = unsafe { TypeSlot::<P>::hold(user) };
        let Some(handler) = &slot.any_member else { return false };
        let Some(key) = (unsafe { RawKey::from_c(key) }) else { return false };
        let (member, index) = unsafe { (from_c(member), from_c(index)) };
        match slot.runtime.call_data(KIND_DATATYPE, |plugin, mq| handler(plugin, mq, &key, &member, &index)) {
            Some(value) => slot.runtime.set_value(result, &value),
            None => false,
        }
    }))
    .unwrap_or(false)
}

unsafe extern "C" fn downcast_call<P: Plugin>(user: *mut c_void, key: *const mqemu_key, result: *mut mqemu_tlo_result) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        let slot = unsafe { TypeSlot::<P>::hold(user) };
        let Some(downcast) = &slot.downcast else { return false };
        let Some(key) = (unsafe { RawKey::from_c(key) }) else { return false };
        match slot.runtime.call_data(KIND_DATATYPE, |plugin, mq| downcast(plugin, mq, &key)) {
            Some(value) => slot.runtime.set_value(result, &value),
            None => false,
        }
    }))
    .unwrap_or(false)
}

unsafe extern "C" fn text_call<P: Plugin>(user: *mut c_void, key: *const mqemu_key, out: *mut c_char, cap: usize) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        let slot = unsafe { TypeSlot::<P>::hold(user) };
        let Some(key) = (unsafe { RawKey::from_c(key) }) else { return false };
        let text = match &slot.text {
            None => Some(slot.name.clone()),
            Some(handler) => slot.runtime.call_data(KIND_DATATYPE, |plugin, mq| handler(plugin, mq, &key)),
        };
        text.is_some_and(|text| write_c(&text, out, cap))
    }))
    .unwrap_or(false)
}

unsafe extern "C" fn type_drop<P: Plugin>(user: *mut c_void) {
    drop(unsafe { Rc::from_raw(user.cast::<TypeSlot<P>>()) });
}

/// Writes `text` NUL-terminated into `cap` bytes, cut at a character.
fn write_c(text: &str, out: *mut c_char, cap: usize) -> bool {
    if out.is_null() || cap == 0 {
        return false;
    }
    let mut len = text.len().min(cap - 1);
    while !text.is_char_boundary(len) {
        len -= 1;
    }
    // SAFETY: the caller gives `cap` writable bytes at `out`.
    unsafe {
        ptr::copy_nonoverlapping(text.as_ptr(), out.cast::<u8>(), len);
        *out.add(len) = 0;
    }
    true
}

impl<P: Plugin> Runtime<P> {
    fn add_type<K: TypeKey>(&'static self, name: &str, def: DataTypeDef<P, K>) -> Result<DataType<K>, DataTypeError> {
        if !valid_tlo_name(name) {
            return Err(DataTypeError::InvalidName);
        }
        for (i, member) in def.members.iter().enumerate() {
            if !valid_tlo_name(&member.name) || def.members[..i].iter().any(|other| other.name == member.name) {
                return Err(DataTypeError::InvalidMember(member.name.clone()));
            }
        }
        let (any_names, any_member) = def.any_member.map_or((Vec::new(), None), |(names, handler)| (names, Some(handler)));
        for (i, name) in any_names.iter().enumerate() {
            if !valid_tlo_name(name) || any_names[..i].contains(name) || def.members.iter().any(|member| member.name == *name) {
                return Err(DataTypeError::InvalidMember(name.clone()));
            }
        }
        if self.datatypes.borrow().iter().any(|t| t.name.eq_ignore_ascii_case(name)) {
            return Err(DataTypeError::AlreadyAdded);
        }
        if (self.host().main_interface)().is_null() {
            return Err(DataTypeError::Unavailable);
        }
        let members: Vec<(String, bool)> = def.members.iter().map(|m| (m.name.clone(), m.method)).collect();
        let (parent, downcast) = def.parent.map_or((None, None), |(parent, downcast)| (Some(parent), Some(downcast)));
        let has_any_member = any_member.is_some();
        let slot = TypeSlot { name: name.to_owned(), members: def.members, any_member, text: def.text, downcast, runtime: self };
        let user = Rc::into_raw(Rc::new(slot)).cast_mut().cast::<c_void>();
        let api = &self.host().data;
        // Ownership of `user` passes over even if this fails.
        let raw = (api.add_type)(&self.handle(), name, K::KIND, member_call::<P>, text_call::<P>, user, type_drop::<P>);
        if raw.is_null() {
            return Err(DataTypeError::Rejected);
        }
        for (id, (member, method)) in members.iter().enumerate() {
            // Names were checked above, so the type takes each.
            (api.type_add_member)(raw, id as i32, member, *method);
        }
        if has_any_member {
            (api.type_any_member)(raw, any_member_call::<P>);
            for name in &any_names {
                (api.type_add_member)(raw, ANY_MEMBER, name, false);
            }
        }
        if let Some(parent) = parent {
            let refused = match (api.type_inherit)(raw, &parent, downcast_call::<P>) {
                inherit::OK => None,
                inherit::NO_TYPE => Some(DataTypeError::NoSuchType(parent)),
                _ => Some(DataTypeError::ParentNotAllowed(parent)),
            };
            if let Some(error) = refused {
                // Nothing has a value of it yet.
                (api.free_type)(raw);
                return Err(error);
            }
        }
        let shared = Rc::new(TypeShared { name: name.to_owned(), raw: Cell::new(raw) });
        self.datatypes.borrow_mut().push(Rc::clone(&shared));
        Ok(DataType { shared, _key: PhantomData })
    }

    fn remove_type(&self, name: &str) -> bool {
        let position = self.datatypes.borrow().iter().position(|t| t.name.eq_ignore_ascii_case(name));
        let Some(position) = position else { return false };
        let shared = self.datatypes.borrow_mut().remove(position);
        let raw = shared.raw.replace(ptr::null_mut());
        (self.host().data.remove_type)(raw);
        self.removed_datatypes.borrow_mut().push(raw);
        true
    }

    /// At shutdown, with the extensions: MacroQuest can't use the types'
    /// values once the plugin's code is gone anyway.
    pub(crate) fn free_all_datatypes(&self) {
        let extensions: Vec<_> = self.extensions.borrow_mut().drain(..).map(|(_, raw)| raw).collect();
        let removed_extensions: Vec<_> = self.removed_extensions.borrow_mut().drain(..).collect();
        for raw in extensions.into_iter().chain(removed_extensions) {
            (self.host().data.free_extension)(raw);
        }
        let added: Vec<_> = self.datatypes.borrow_mut().drain(..).collect();
        let removed: Vec<_> = self.removed_datatypes.borrow_mut().drain(..).collect();
        let free = self.host().data.free_type;
        for shared in added {
            free(shared.raw.replace(ptr::null_mut()));
        }
        for raw in removed {
            free(raw);
        }
    }
}

impl<P: Plugin> Mq<P> {
    /// Adds a datatype: values with the members `def` gives them, which the
    /// plugin's TLOs and members return ([`DataType::value`]) and macros and
    /// Lua scripts read (`${Bot[Tank].HP}`, `mq.TLO.Bot('Tank').HP()`). Each
    /// value holds a key of type `K` for the members to look up. Macro
    /// variables can be declared with the type and set from another value or
    /// from a key's text (`/declare b Bot`, `/varset b Tank`).
    ///
    /// It's removed when the plugin unloads (see [`Mq::remove_type`]). As
    /// with a C++ plugin's types, a value a Lua script keeps
    /// (`local hp = mq.TLO.Bot('Tank').HP`) can't be used after that:
    /// MacroQuest would call into the unloaded plugin.
    ///
    /// Like the plugin's TLOs, members read as NULL while one of the
    /// plugin's callbacks runs (the plugin is borrowed).
    pub fn add_type<K: TypeKey>(&self, name: &str, def: DataTypeDef<P, K>) -> Result<DataType<K>, DataTypeError> {
        self.runtime().add_type(name, def)
    }

    /// Removes a datatype added with [`Mq::add_type`]: MacroQuest no longer
    /// finds it, and values of it that are still around read as NULL. Macro
    /// variables of it become strings holding their keys (arrays of it,
    /// empty strings), since deleting a variable calls its type, which may
    /// be after the plugin has unloaded. False if there was none.
    pub fn remove_type(&self, name: &str) -> bool {
        self.runtime().remove_type(name)
    }

    /// Adds members to datatype `type_name`, which isn't the plugin's own:
    /// one of MacroQuest's (`"spawn"`, `"character"`, `"item"`...) or
    /// another plugin's. Macros and Lua read them as the type's own
    /// (`${Target.Threat}`, `mq.TLO.Target.Threat()`); the handlers get the
    /// value as [`Data`], to read its members.
    ///
    /// Members are matched by the type's name only: `character` (`${Me}`)
    /// doesn't get `spawn`'s, so extend each type that should have them.
    /// Names the type already has are refused, since the extension would
    /// hide them from every macro and plugin. MacroQuest doesn't call an
    /// extension's methods, so there are none. One extension per type: to
    /// change it, remove it ([`Mq::remove_type_extension`]) and add another.
    /// It's removed when the plugin unloads.
    ///
    /// Like the plugin's TLOs, its members read as NULL while one of the
    /// plugin's callbacks runs (the plugin is borrowed).
    pub fn extend_type(&self, type_name: &str, extension: TypeExtension<P>) -> Result<(), DataTypeError> {
        self.runtime().extend_type(type_name, extension)
    }

    /// Removes the plugin's extension of `type_name` ([`Mq::extend_type`]).
    /// False if there was none.
    pub fn remove_type_extension(&self, type_name: &str) -> bool {
        self.runtime().remove_type_extension(type_name)
    }
}

// ----- extensions of other types ------------------------------------------------------

type ExtensionHandler<P> = dyn for<'a> Fn(&mut P, &'a Mq<P>, &Data<'a>, &str) -> Option<Value>;

/// Members to add to a datatype that isn't the plugin's own, for
/// [`Mq::extend_type`]. Handlers get the plugin, the value (as MacroQuest
/// has it) and the member's index (`""` for none).
pub struct TypeExtension<P: Plugin> {
    members: Vec<(String, Box<ExtensionHandler<P>>)>,
}

impl<P: Plugin> TypeExtension<P> {
    pub fn new() -> TypeExtension<P> {
        TypeExtension { members: Vec::new() }
    }

    /// `${Value.Name}` or `${Value.Name[index]}`: the value, or None for
    /// NULL. Names are letters, digits and `_`, starting with a letter.
    pub fn member<F>(mut self, name: &str, handler: F) -> TypeExtension<P>
    where
        F: for<'a> Fn(&mut P, &'a Mq<P>, &Data<'a>, &str) -> Option<Value> + 'static,
    {
        self.members.push((name.to_owned(), Box::new(handler)));
        self
    }
}

impl<P: Plugin> Default for TypeExtension<P> {
    fn default() -> Self {
        TypeExtension::new()
    }
}

/// One extension's handlers, owned by the C++ side until it's removed.
struct ExtensionSlot<P: Plugin> {
    /// By member ID.
    members: Vec<Box<ExtensionHandler<P>>>,
    runtime: &'static Runtime<P>,
}

unsafe extern "C" fn extension_call<P: Plugin>(
    user: *mut c_void,
    value: *mut mqemu_data,
    member: i32,
    index: *const c_char,
    result: *mut mqemu_tlo_result,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        // Held for the call, as a type's slot is: a handler may remove its extension.
        let slot = unsafe {
            let user = user.cast::<ExtensionSlot<P>>().cast_const();
            Rc::increment_strong_count(user);
            Rc::from_raw(user)
        };
        // Ours to free, which dropping it does.
        let Some(value) = Data::new(value, &slot.runtime.host().data) else { return false };
        let Some(handler) = usize::try_from(member).ok().and_then(|id| slot.members.get(id)) else { return false };
        let index = unsafe { from_c(index) };
        match slot.runtime.call_data(KIND_DATATYPE, |plugin, mq| handler(plugin, mq, &value, &index)) {
            Some(value) => slot.runtime.set_value(result, &value),
            None => false,
        }
    }))
    .unwrap_or(false)
}

unsafe extern "C" fn extension_drop<P: Plugin>(user: *mut c_void) {
    drop(unsafe { Rc::from_raw(user.cast::<ExtensionSlot<P>>()) });
}

impl<P: Plugin> Runtime<P> {
    fn extend_type(&'static self, type_name: &str, extension: TypeExtension<P>) -> Result<(), DataTypeError> {
        let names: Vec<String> = extension.members.iter().map(|(name, _)| name.clone()).collect();
        for (i, name) in names.iter().enumerate() {
            if !valid_tlo_name(name) || names[..i].contains(name) {
                return Err(DataTypeError::InvalidMember(name.clone()));
            }
        }
        // MacroQuest matches extensions to types by the exact name.
        if self.extensions.borrow().iter().any(|(extended, _)| extended == type_name) {
            return Err(DataTypeError::AlreadyAdded);
        }
        if (self.host().main_interface)().is_null() {
            return Err(DataTypeError::Unavailable);
        }
        let slot = ExtensionSlot { members: extension.members.into_iter().map(|(_, handler)| handler).collect(), runtime: self };
        let user = Rc::into_raw(Rc::new(slot)).cast_mut().cast::<c_void>();
        let api = &self.host().data;
        // Ownership of `user` passes over even if this fails.
        let raw = (api.add_extension)(&self.handle(), type_name, extension_call::<P>, user, extension_drop::<P>);
        if raw.is_null() {
            return Err(DataTypeError::NoSuchType(type_name.to_owned()));
        }
        for (id, name) in names.iter().enumerate() {
            // Taken by the type itself.
            if !(api.extension_add_member)(raw, id as i32, name) {
                (api.free_extension)(raw);
                return Err(DataTypeError::InvalidMember(name.clone()));
            }
        }
        if !(api.extension_register)(raw) {
            (api.free_extension)(raw);
            return Err(DataTypeError::Rejected);
        }
        self.extensions.borrow_mut().push((type_name.to_owned(), raw));
        Ok(())
    }

    fn remove_type_extension(&self, type_name: &str) -> bool {
        let position = self.extensions.borrow().iter().position(|(extended, _)| extended == type_name);
        let Some(position) = position else { return false };
        let (_, raw) = self.extensions.borrow_mut().remove(position);
        (self.host().data.remove_extension)(raw);
        // Its member may be running.
        self.removed_extensions.borrow_mut().push(raw);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_refuse_other_kinds() {
        assert_eq!(<()>::from_raw(&().to_raw()), Some(()));
        assert_eq!(i32::from_raw(&(-5i32).to_raw()), Some(-5));
        assert_eq!(u32::from_raw(&u32::MAX.to_raw()), Some(u32::MAX));
        assert_eq!(i64::from_raw(&i64::MIN.to_raw()), Some(i64::MIN));
        assert_eq!(String::from_raw(&"Tank".to_owned().to_raw()).as_deref(), Some("Tank"));
        // What a Lua script or /varset can put in a value of the type.
        assert_eq!(i32::from_raw(&RawKey::Int(1 << 40)), None);
        assert_eq!(u32::from_raw(&RawKey::Int(-1)), None);
        assert_eq!(i64::from_raw(&RawKey::Text("5".into())), None);
        assert_eq!(String::from_raw(&RawKey::Int(5)), None);
        assert_eq!(<()>::from_raw(&RawKey::Text(String::new())), None);
    }

    #[test]
    fn text_is_cut_to_fit_at_a_character() {
        let mut buffer = [0x55 as c_char; 6];
        assert!(write_c("héllo", buffer.as_mut_ptr(), 4));
        // "h" + the 2-byte "é" fit in 3 bytes; the NUL takes the 4th.
        assert_eq!(&buffer[..4].iter().map(|c| *c as u8).collect::<Vec<_>>(), &[b'h', 0xC3, 0xA9, 0]);
        assert!(write_c("héllo", buffer.as_mut_ptr(), 3));
        assert_eq!(buffer[..2].iter().map(|c| *c as u8).collect::<Vec<_>>(), [b'h', 0]);
        assert!(!write_c("x", buffer.as_mut_ptr(), 0));
    }
}
