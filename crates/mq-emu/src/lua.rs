//! Talking to Lua scripts that use MacroQuest's `actors` module.
//!
//! Lua scripts send and receive one value per message: a number, boolean,
//! string, table, or ImGui vector, encoded as MQ2Lua's `Variant` protobuf
//! (`src/plugins/lua/Actor.proto`). [`LuaValue`] mirrors that, and encodes and
//! decodes it without any dependencies.
//!
//! ```ignore
//! // Rust -> Lua: a script that did `actors.register(handler)`.
//! let ping = LuaTable::new().with("id", "ping").with("level", 65);
//! mq.send(&Address::lua_script("boxer"), LuaValue::from(ping).encode())?;
//!
//! // Lua -> Rust: in on_message or a mailbox handler.
//! let value = message.lua()?;
//! if value.get("id").and_then(LuaValue::as_str) == Some("buffs") { ... }
//! ```
//!
//! Lua's own encoder sends `nil` and empty tables as an empty message, and
//! drops table keys that aren't strings or whole numbers; decoding an empty
//! message gives [`LuaValue::Nil`].

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

use crate::actors::{Address, Message};

/// MQ2Lua's name, which prefixes Lua scripts' mailboxes: `lua` as MacroQuest
/// names it when loaded by its stock MacroQuest.ini (`mq2lua=1`). MacroQuest
/// names a plugin after how it was loaded (`/plugin MQ2Lua` makes it `Lua`)
/// and delivers only to the exact mailbox name, so sending to an address
/// from [`Address::lua_script`] uses whatever name MQ2Lua has on this client.
pub const LUA_PLUGIN: &str = "lua";

/// A value a Lua script can send or receive.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum LuaValue {
    #[default]
    Nil,
    Number(f64),
    Boolean(bool),
    String(String),
    Table(LuaTable),
    /// An `ImVec2`.
    Vec2([f32; 2]),
    /// An `ImVec4`.
    Vec4([f32; 4]),
}

/// A Lua table: string keys in `fields`, positive integer keys in `items`
/// (Lua arrays start at 1).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LuaTable {
    pub fields: BTreeMap<String, LuaValue>,
    pub items: BTreeMap<u32, LuaValue>,
}

impl LuaTable {
    pub fn new() -> LuaTable {
        LuaTable::default()
    }

    /// Sets a string key, builder style.
    pub fn with(mut self, key: impl Into<String>, value: impl Into<LuaValue>) -> LuaTable {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Appends at the next array index (1 for an empty array).
    pub fn push(&mut self, value: impl Into<LuaValue>) {
        let next = self.items.keys().next_back().map_or(1, |last| last + 1);
        self.items.insert(next, value.into());
    }

    pub fn get(&self, key: &str) -> Option<&LuaValue> {
        self.fields.get(key)
    }

    /// `table[index]`, counting from 1 as Lua does.
    pub fn index(&self, index: u32) -> Option<&LuaValue> {
        self.items.get(&index)
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.items.is_empty()
    }
}

impl LuaValue {
    pub fn is_nil(&self) -> bool {
        matches!(self, LuaValue::Nil)
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            LuaValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            LuaValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            LuaValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_table(&self) -> Option<&LuaTable> {
        match self {
            LuaValue::Table(t) => Some(t),
            _ => None,
        }
    }

    /// `value[key]` if this is a table.
    pub fn get(&self, key: &str) -> Option<&LuaValue> {
        self.as_table()?.get(key)
    }

    /// Encodes as MQ2Lua's `Variant`, ready to send.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        encode_variant(self, &mut out);
        out
    }

    /// Decodes a `Variant` as a Lua script sent it.
    pub fn decode(bytes: &[u8]) -> Result<LuaValue, DecodeError> {
        decode_variant(bytes, 0)
    }
}

macro_rules! from_number {
    ($($t:ty),*) => {$(
        impl From<$t> for LuaValue {
            fn from(value: $t) -> LuaValue {
                LuaValue::Number(f64::from(value))
            }
        }
    )*};
}
from_number!(f64, f32, i32, u32, i16, u16, i8, u8);

impl From<bool> for LuaValue {
    fn from(value: bool) -> LuaValue {
        LuaValue::Boolean(value)
    }
}

impl From<&str> for LuaValue {
    fn from(value: &str) -> LuaValue {
        LuaValue::String(value.to_owned())
    }
}

impl From<String> for LuaValue {
    fn from(value: String) -> LuaValue {
        LuaValue::String(value)
    }
}

impl From<LuaTable> for LuaValue {
    fn from(value: LuaTable) -> LuaValue {
        LuaValue::Table(value)
    }
}

impl<T: Into<LuaValue>> From<Option<T>> for LuaValue {
    fn from(value: Option<T>) -> LuaValue {
        value.map_or(LuaValue::Nil, Into::into)
    }
}

impl<T: Into<LuaValue>> FromIterator<T> for LuaTable {
    /// Collects into an array (indices 1, 2, ...).
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> LuaTable {
        let mut table = LuaTable::new();
        for value in iter {
            table.push(value);
        }
        table
    }
}

/// Why a payload isn't a valid Lua value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The payload ends in the middle of a field.
    Truncated,
    /// Something that can't appear in a well-formed payload.
    Malformed(&'static str),
    /// Tables nested deeper than [`MAX_DEPTH`].
    TooDeep,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::Truncated => f.write_str("truncated Lua message"),
            DecodeError::Malformed(what) => write!(f, "malformed Lua message: {what}"),
            DecodeError::TooDeep => write!(f, "Lua message nested deeper than {MAX_DEPTH} tables"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// The deepest table nesting [`LuaValue::decode`] accepts.
pub const MAX_DEPTH: usize = 64;

impl Message {
    /// Decodes the payload as a value from a Lua script.
    pub fn lua(&self) -> Result<LuaValue, DecodeError> {
        LuaValue::decode(self.payload())
    }
}

impl Address {
    /// A Lua script's default mailbox (`actors.register(handler)` in script
    /// `script`), on any character unless narrowed with `.character(...)`.
    pub fn lua_script(script: impl AsRef<str>) -> Address {
        Address { mailbox: Some(format!("{LUA_PLUGIN}:{}", script.as_ref())), absolute_mailbox: true, ..Address::default() }
    }

    /// A Lua script's named mailbox (`actors.register(name, handler)`).
    pub fn lua_mailbox(script: impl AsRef<str>, name: impl AsRef<str>) -> Address {
        Address {
            mailbox: Some(format!("{LUA_PLUGIN}:{}:{}", script.as_ref(), name.as_ref())),
            absolute_mailbox: true,
            ..Address::default()
        }
    }

    /// `self` with a Lua script's mailbox renamed for MQ2Lua's `loaded_name`
    /// (see [`LUA_PLUGIN`]). Other addresses are returned as they are.
    pub(crate) fn for_loaded_lua(&self, loaded_name: impl FnOnce() -> Option<String>) -> Cow<'_, Address> {
        let lua_mailbox = self
            .mailbox
            .as_deref()
            .filter(|_| self.absolute_mailbox)
            .and_then(|mailbox| mailbox.split_once(':'))
            .filter(|(plugin, _)| plugin.eq_ignore_ascii_case(LUA_PLUGIN));
        let Some((plugin, rest)) = lua_mailbox else { return Cow::Borrowed(self) };
        match loaded_name() {
            Some(name) if name != plugin => Cow::Owned(Address { mailbox: Some(format!("{name}:{rest}")), ..self.clone() }),
            _ => Cow::Borrowed(self),
        }
    }

    /// The same address on one character.
    pub fn on_character(mut self, character: impl Into<String>) -> Address {
        self.character = Some(character.into());
        self
    }
}

// ----- protobuf wire format ----------------------------------------------------------
//
// message Variant { oneof value { double number = 1; bool boolean = 5;
//   string str = 8; Table table = 10; ImVec2 imvec2 = 11; ImVec4 imvec4 = 12; } }
// message Table { map<string, Variant> entries = 1; map<uint32, Variant> arr = 2; }
// message ImVec2 { optional float x = 1; optional float y = 2; }
// message ImVec4 { optional float x = 1; ... optional float w = 4; }

const VARINT: u8 = 0;
const FIXED64: u8 = 1;
const LEN: u8 = 2;
const FIXED32: u8 = 5;

fn put_varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn put_tag(field: u32, wire: u8, out: &mut Vec<u8>) {
    put_varint(u64::from(field << 3 | u32::from(wire)), out);
}

fn put_bytes(field: u32, bytes: &[u8], out: &mut Vec<u8>) {
    put_tag(field, LEN, out);
    put_varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}

fn put_floats(values: &[f32], out: &mut Vec<u8>) {
    for (i, value) in values.iter().enumerate() {
        put_tag(i as u32 + 1, FIXED32, out);
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn encode_variant(value: &LuaValue, out: &mut Vec<u8>) {
    match value {
        LuaValue::Nil => {}
        LuaValue::Number(n) => {
            put_tag(1, FIXED64, out);
            out.extend_from_slice(&n.to_le_bytes());
        }
        LuaValue::Boolean(b) => {
            put_tag(5, VARINT, out);
            out.push(u8::from(*b));
        }
        LuaValue::String(s) => put_bytes(8, s.as_bytes(), out),
        LuaValue::Table(table) => {
            let mut body = Vec::new();
            for (key, item) in &table.fields {
                let mut entry = Vec::new();
                put_bytes(1, key.as_bytes(), &mut entry);
                put_bytes(2, &item.encode(), &mut entry);
                put_bytes(1, &entry, &mut body);
            }
            for (key, item) in &table.items {
                let mut entry = Vec::new();
                put_tag(1, VARINT, &mut entry);
                put_varint(u64::from(*key), &mut entry);
                put_bytes(2, &item.encode(), &mut entry);
                put_bytes(2, &entry, &mut body);
            }
            put_bytes(10, &body, out);
        }
        LuaValue::Vec2(v) => {
            let mut body = Vec::new();
            put_floats(v, &mut body);
            put_bytes(11, &body, out);
        }
        LuaValue::Vec4(v) => {
            let mut body = Vec::new();
            put_floats(v, &mut body);
            put_bytes(12, &body, out);
        }
    }
}

/// Reads protobuf fields from a buffer.
struct Reader<'a> {
    bytes: &'a [u8],
}

enum Field<'a> {
    Varint(u64),
    Fixed64([u8; 8]),
    Bytes(&'a [u8]),
    Fixed32([u8; 4]),
}

impl<'a> Reader<'a> {
    fn varint(&mut self) -> Result<u64, DecodeError> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let (&byte, rest) = self.bytes.split_first().ok_or(DecodeError::Truncated)?;
            self.bytes = rest;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(DecodeError::Malformed("varint longer than 10 bytes"))
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        if self.bytes.len() < n {
            return Err(DecodeError::Truncated);
        }
        let (head, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        Ok(head)
    }

    /// The next (field number, value), or None at the end.
    fn next(&mut self) -> Result<Option<(u32, Field<'a>)>, DecodeError> {
        if self.bytes.is_empty() {
            return Ok(None);
        }
        let tag = self.varint()?;
        let field = u32::try_from(tag >> 3).map_err(|_| DecodeError::Malformed("field number too large"))?;
        if field == 0 {
            return Err(DecodeError::Malformed("field number 0"));
        }
        let value = match (tag & 7) as u8 {
            VARINT => Field::Varint(self.varint()?),
            FIXED64 => Field::Fixed64(self.take(8)?.try_into().unwrap()),
            LEN => {
                let len = usize::try_from(self.varint()?).map_err(|_| DecodeError::Malformed("length too large"))?;
                Field::Bytes(self.take(len)?)
            }
            FIXED32 => Field::Fixed32(self.take(4)?.try_into().unwrap()),
            _ => return Err(DecodeError::Malformed("unsupported wire type")),
        };
        Ok(Some((field, value)))
    }
}

fn decode_variant(bytes: &[u8], depth: usize) -> Result<LuaValue, DecodeError> {
    if depth > MAX_DEPTH {
        return Err(DecodeError::TooDeep);
    }
    let mut reader = Reader { bytes };
    // A oneof: the last member present wins, as in protobuf.
    let mut value = LuaValue::Nil;
    while let Some((field, data)) = reader.next()? {
        value = match (field, data) {
            (1, Field::Fixed64(raw)) => LuaValue::Number(f64::from_le_bytes(raw)),
            (5, Field::Varint(raw)) => LuaValue::Boolean(raw != 0),
            (8, Field::Bytes(raw)) => {
                LuaValue::String(String::from_utf8(raw.to_vec()).map_err(|_| DecodeError::Malformed("string is not UTF-8"))?)
            }
            (10, Field::Bytes(raw)) => LuaValue::Table(decode_table(raw, depth + 1)?),
            (11, Field::Bytes(raw)) => LuaValue::Vec2(decode_floats::<2>(raw)?),
            (12, Field::Bytes(raw)) => LuaValue::Vec4(decode_floats::<4>(raw)?),
            (1 | 5 | 8 | 10 | 11 | 12, _) => return Err(DecodeError::Malformed("wrong wire type for a Variant field")),
            _ => value, // unknown field: skip, as protobuf does
        };
    }
    Ok(value)
}

fn decode_table(bytes: &[u8], depth: usize) -> Result<LuaTable, DecodeError> {
    let mut table = LuaTable::new();
    let mut reader = Reader { bytes };
    while let Some((field, data)) = reader.next()? {
        let Field::Bytes(entry) = data else {
            return Err(DecodeError::Malformed("table entry is not a message"));
        };
        // Map entries: key = 1, value = 2; either may be absent (default).
        let mut entry_reader = Reader { bytes: entry };
        let (mut key_text, mut key_number, mut value) = (String::new(), 0u32, LuaValue::Nil);
        while let Some((entry_field, entry_data)) = entry_reader.next()? {
            match (field, entry_field, entry_data) {
                (1, 1, Field::Bytes(raw)) => {
                    key_text = String::from_utf8(raw.to_vec()).map_err(|_| DecodeError::Malformed("table key is not UTF-8"))?;
                }
                (2, 1, Field::Varint(raw)) => {
                    key_number = u32::try_from(raw).map_err(|_| DecodeError::Malformed("array index too large"))?;
                }
                (_, 2, Field::Bytes(raw)) => value = decode_variant(raw, depth)?,
                (_, 1 | 2, _) => return Err(DecodeError::Malformed("wrong wire type in a table entry")),
                _ => {}
            }
        }
        match field {
            1 => {
                table.fields.insert(key_text, value);
            }
            2 => {
                table.items.insert(key_number, value);
            }
            _ => {} // unknown field
        }
    }
    Ok(table)
}

fn decode_floats<const N: usize>(bytes: &[u8]) -> Result<[f32; N], DecodeError> {
    let mut values = [0f32; N];
    let mut reader = Reader { bytes };
    while let Some((field, data)) = reader.next()? {
        match data {
            Field::Fixed32(raw) if (1..=N as u32).contains(&field) => values[field as usize - 1] = f32::from_le_bytes(raw),
            Field::Fixed32(_) => {}
            _ if (1..=N as u32).contains(&field) => return Err(DecodeError::Malformed("vector component is not a float")),
            _ => {}
        }
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(value: LuaValue) {
        assert_eq!(LuaValue::decode(&value.encode()), Ok(value.clone()), "{value:?}");
    }

    #[test]
    fn scalars_round_trip() {
        for value in [
            LuaValue::Nil,
            LuaValue::Number(0.0),
            LuaValue::Number(-12.5),
            LuaValue::Number(f64::MAX),
            LuaValue::Boolean(true),
            LuaValue::Boolean(false),
            LuaValue::from(""),
            LuaValue::from("héllo"),
            LuaValue::Vec2([1.5, -2.0]),
            LuaValue::Vec4([1.0, 2.0, 3.0, 4.0]),
        ] {
            round_trip(value);
        }
    }

    #[test]
    fn nested_tables_round_trip() {
        let mut list = LuaTable::new();
        list.push("a");
        list.push(2);
        let value = LuaValue::from(
            LuaTable::new()
                .with("id", "buffs")
                .with("level", 65)
                .with("ok", true)
                .with("list", list)
                .with("empty", LuaTable::new())
                .with("nothing", LuaValue::Nil),
        );
        round_trip(value);
    }

    #[test]
    fn known_encodings() {
        // Hand-checked against the protobuf wire format.
        assert_eq!(LuaValue::Nil.encode(), b"");
        assert_eq!(LuaValue::Boolean(true).encode(), [0x28, 0x01]);
        assert_eq!(LuaValue::from("hi").encode(), [0x42, 0x02, b'h', b'i']);
        assert_eq!(LuaValue::Number(1.0).encode(), [0x09, 0, 0, 0, 0, 0, 0, 0xf0, 0x3f]);
        // { [1] = true }: table(10) { arr(2) entry { key(1)=1, value(2) { boolean(5)=1 } } }
        let array: LuaTable = [true].into_iter().collect();
        assert_eq!(LuaValue::from(array).encode(), [0x52, 0x08, 0x12, 0x06, 0x08, 0x01, 0x12, 0x02, 0x28, 0x01]);
    }

    #[test]
    fn push_and_collect_number_from_one() {
        let table: LuaTable = ["a", "b"].into_iter().collect();
        assert_eq!(table.index(1), Some(&LuaValue::from("a")));
        assert_eq!(table.index(2), Some(&LuaValue::from("b")));
        assert_eq!(table.index(0), None);
    }

    #[test]
    fn last_oneof_member_wins_and_unknown_fields_are_skipped() {
        let mut bytes = LuaValue::from("first").encode();
        bytes.extend(LuaValue::Number(2.0).encode());
        // Unknown field 15, varint 7.
        bytes.extend([0x78, 0x07]);
        assert_eq!(LuaValue::decode(&bytes), Ok(LuaValue::Number(2.0)));
    }

    #[test]
    fn missing_map_keys_and_values_default() {
        // fields entry with no key and no value: { [""] = nil }
        let bytes = [0x52, 0x02, 0x0a, 0x00];
        let table = LuaValue::decode(&bytes).unwrap();
        assert_eq!(table.get(""), Some(&LuaValue::Nil));
    }

    // Negative tests.

    #[test]
    fn truncated_payloads_are_rejected() {
        let full = LuaValue::from(LuaTable::new().with("id", "ping")).encode();
        for cut in 1..full.len() {
            assert!(LuaValue::decode(&full[..cut]).is_err(), "cut at {cut} decoded");
        }
    }

    #[test]
    fn malformed_payloads_are_rejected() {
        assert_eq!(LuaValue::decode(&[0x0a]), Err(DecodeError::Truncated));
        // Field 1 (number) must be a double, not a varint.
        assert_eq!(LuaValue::decode(&[0x08, 0x01]), Err(DecodeError::Malformed("wrong wire type for a Variant field")));
        assert_eq!(LuaValue::decode(&[0x42, 0x01, 0xff]), Err(DecodeError::Malformed("string is not UTF-8")));
        assert_eq!(LuaValue::decode(&[0x00]), Err(DecodeError::Malformed("field number 0")));
        assert_eq!(LuaValue::decode(&[0x0b]), Err(DecodeError::Malformed("unsupported wire type")));
        assert_eq!(LuaValue::decode(&[0xff; 11]), Err(DecodeError::Malformed("varint longer than 10 bytes")));
    }

    #[test]
    fn deep_nesting_is_rejected() {
        let mut value = LuaValue::Boolean(true);
        for _ in 0..=MAX_DEPTH {
            value = LuaValue::from(LuaTable::new().with("x", value));
        }
        assert_eq!(LuaValue::decode(&value.encode()), Err(DecodeError::TooDeep));
    }

    #[test]
    fn lua_addresses() {
        let script = Address::lua_script("boxer").on_character("Qtee");
        assert_eq!(script.mailbox.as_deref(), Some("lua:boxer"));
        assert!(script.absolute_mailbox);
        assert_eq!(script.character.as_deref(), Some("Qtee"));
        assert_eq!(Address::lua_mailbox("boxer", "buffs").mailbox.as_deref(), Some("lua:boxer:buffs"));
    }

    #[test]
    fn lua_mailboxes_follow_the_loaded_name() {
        let script = Address::lua_script("boxer").on_character("Qtee");
        let renamed = script.for_loaded_lua(|| Some("Lua".into()));
        assert_eq!(renamed.mailbox.as_deref(), Some("Lua:boxer"));
        assert_eq!(renamed.character.as_deref(), Some("Qtee"));
        assert_eq!(Address::lua_mailbox("boxer", "buffs").for_loaded_lua(|| Some("LUA".into())).mailbox.as_deref(), Some("LUA:boxer:buffs"));
        // Same name, or MQ2Lua not loaded here: unchanged.
        assert!(matches!(script.for_loaded_lua(|| Some("lua".into())), Cow::Borrowed(_)));
        assert!(matches!(script.for_loaded_lua(|| None), Cow::Borrowed(_)));
    }

    #[test]
    fn other_mailboxes_are_never_renamed() {
        let loaded = || -> Option<String> { panic!("looked up MQ2Lua for a non-Lua address") };
        for address in [
            Address::character("Bob"),
            Address::character("Bob").mailbox("lua:boxer"), // relative: the sender's own plugin
            Address::character("Bob").mailbox("luac:boxer").absolute(),
            Address::character("Bob").mailbox("Other:lua").absolute(),
            Address::character("Bob").mailbox("lua").absolute(),
        ] {
            assert!(matches!(address.for_loaded_lua(loaded), Cow::Borrowed(_)), "{address}");
        }
    }
}
