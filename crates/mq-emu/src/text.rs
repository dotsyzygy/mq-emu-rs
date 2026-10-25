//! Conversions between Rust strings and the C strings MacroQuest uses.
//!
//! Text coming from the game isn't guaranteed to be UTF-8 (EverQuest chat can
//! contain Windows-1252 characters), so incoming text is converted lossily and
//! never panics. Outgoing text has interior NULs removed rather than failing.

use std::borrow::Cow;
use std::ffi::{CStr, CString, c_char};

/// Reads a NUL-terminated C string, replacing invalid UTF-8. Null gives "".
///
/// # Safety
/// `ptr` must be null or point to a NUL-terminated string valid for `'a`.
pub(crate) unsafe fn from_c<'a>(ptr: *const c_char) -> Cow<'a, str> {
    if ptr.is_null() {
        return Cow::Borrowed("");
    }
    unsafe { CStr::from_ptr(ptr) }.to_string_lossy()
}

/// Reads a fixed-size C char array (e.g. a name field), stopping at the first
/// NUL or at the end of the array if there is none.
pub(crate) fn from_c_array(chars: &[c_char]) -> String {
    let bytes: Vec<u8> = chars.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Converts to a C string, dropping interior NULs (which C would treat as the end).
pub(crate) fn to_c(text: &str) -> CString {
    match CString::new(text) {
        Ok(c) => c,
        Err(_) => CString::new(text.replace('\0', "")).expect("NULs removed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_reads_as_empty() {
        assert_eq!(unsafe { from_c(std::ptr::null()) }, "");
    }

    #[test]
    fn invalid_utf8_is_replaced_not_rejected() {
        // "caf\xe9" in Windows-1252, as EverQuest chat may send it.
        let bytes = b"caf\xe9\0";
        assert_eq!(unsafe { from_c(bytes.as_ptr().cast()) }, "caf\u{fffd}");
    }

    #[test]
    fn arrays_stop_at_nul_or_end() {
        let mut name = [0 as c_char; 8];
        for (d, s) in name.iter_mut().zip(b"Bob\0junk") {
            *d = *s as c_char;
        }
        assert_eq!(from_c_array(&name), "Bob");
        let full = [b'A' as c_char; 4];
        assert_eq!(from_c_array(&full), "AAAA");
    }

    #[test]
    fn interior_nuls_are_dropped() {
        assert_eq!(to_c("a\0b").as_bytes(), b"ab");
        assert_eq!(to_c("plain").as_bytes(), b"plain");
    }
}
