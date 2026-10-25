//! Test-only: MQ2Lua's actor message schema handled by real C++ protobuf
//! (MacroQuest's generated `Actor.pb.cc`), to check mq-emu's hand-written
//! Rust encoding against. Never part of a plugin.

use std::ffi::{CString, c_char};

unsafe extern "C" {
    fn lua_oracle_from_text(text: *const c_char, out: *mut c_char, cap: usize) -> usize;
    fn lua_oracle_to_text(data: *const c_char, len: usize, out: *mut c_char, cap: usize) -> usize;
    fn lua_oracle_reencode(data: *const c_char, len: usize, out: *mut c_char, cap: usize) -> usize;
}

const CAP: usize = 64 * 1024;

fn call(f: impl FnOnce(*mut c_char, usize) -> usize) -> Option<Vec<u8>> {
    let mut out = vec![0u8; CAP];
    let len = f(out.as_mut_ptr().cast(), CAP);
    (len != usize::MAX).then(|| {
        out.truncate(len);
        out
    })
}

/// Encodes a `Variant` given in protobuf text format, e.g. `str: "hi"` or
/// `table { entries { key: "id" value { str: "ping" } } }`. None if C++
/// protobuf can't parse the text.
pub fn encode_text(text: &str) -> Option<Vec<u8>> {
    let text = CString::new(text).ok()?;
    call(|out, cap| unsafe { lua_oracle_from_text(text.as_ptr(), out, cap) })
}

/// Decodes bytes as a `Variant` and prints it in single-line text format
/// (maps sorted by key). None if C++ protobuf rejects the bytes.
pub fn decode_to_text(bytes: &[u8]) -> Option<String> {
    call(|out, cap| unsafe { lua_oracle_to_text(bytes.as_ptr().cast(), bytes.len(), out, cap) })
        .map(|text| String::from_utf8_lossy(&text).into_owned())
}

/// Decodes and re-encodes, as a C++ sender (MQ2Lua) would write the same value.
pub fn reencode(bytes: &[u8]) -> Option<Vec<u8>> {
    call(|out, cap| unsafe { lua_oracle_reencode(bytes.as_ptr().cast(), bytes.len(), out, cap) })
}
