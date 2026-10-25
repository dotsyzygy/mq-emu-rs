//! Conversions between Rust strings and the C strings MacroQuest uses.
//!
//! Text coming from the game isn't guaranteed to be UTF-8 (EverQuest chat can
//! contain Windows-1252 characters), so incoming text is converted lossily and
//! never panics. Outgoing text has interior NULs removed rather than failing.

use std::borrow::Cow;
use std::ffi::{CStr, CString, c_char};

use mq_emu_sys::layouts::{CStrRep, CXStr};

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

/// Where a `CStrRep`'s text starts (its `utf8`/`unicode` union).
pub(crate) const CSTRREP_TEXT_OFFSET: usize = 0x14;
/// `EStringEncoding::StringEncodingUtf16`.
const UTF16: i32 = 1;

/// Reads an eqlib `CXStr`, in either of its encodings. An empty `CXStr` (no
/// representation) reads as "".
pub(crate) fn from_cxstr(text: &CXStr) -> String {
    // SAFETY: a live CXStr's m_data is null or its representation, which holds
    // `length` code units after the header and lives as long as `text`.
    unsafe { from_cstrrep(text.m_data()) }
}

/// # Safety
/// `rep` must be null or a live `CStrRep` whose text holds `length` code units.
pub(crate) unsafe fn from_cstrrep(rep: *const CStrRep) -> String {
    let Some(header) = (unsafe { rep.as_ref() }) else { return String::new() };
    let length = header.length() as usize;
    let text = unsafe { rep.cast::<u8>().add(CSTRREP_TEXT_OFFSET) };
    if header.encoding().0 == UTF16 {
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text.cast::<u16>(), length) })
    } else {
        String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(text, length) }).into_owned()
    }
}

/// Converts to a C string, dropping interior NULs (which C would treat as the end).
pub(crate) fn to_c(text: &str) -> CString {
    match CString::new(text) {
        Ok(c) => c,
        Err(_) => CString::new(text.replace('\0', "")).expect("NULs removed"),
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
    fn cstrrep_text_offset_matches_the_layout() {
        let field = mq_emu_sys::layouts::FIELDS.iter().find(|f| f.record == "CStrRep" && f.field == "utf8").unwrap();
        assert_eq!(field.offset as usize, CSTRREP_TEXT_OFFSET);
    }

    /// A CStrRep followed by `text`, as eqlib allocates one.
    pub(crate) fn cstrrep(encoding: i32, length: u32, text: &[u8]) -> Vec<u64> {
        let mut storage = vec![0u64; (CSTRREP_TEXT_OFFSET + text.len()).div_ceil(8) + 1];
        let raw = storage.as_mut_ptr().cast::<CStrRep>();
        let header = unsafe { &*raw };
        header.set_length(length);
        header.set_encoding(mq_emu_sys::layouts::EStringEncoding(encoding));
        unsafe { raw.cast::<u8>().add(CSTRREP_TEXT_OFFSET).copy_from_nonoverlapping(text.as_ptr(), text.len()) };
        storage
    }

    #[test]
    fn cxstrs_read_both_encodings_up_to_their_length() {
        let utf8 = cstrrep(0, 5, b"Hello, ignored");
        assert_eq!(unsafe { from_cstrrep(utf8.as_ptr().cast()) }, "Hello");
        let utf16: Vec<u8> = "Bob\u{e9}".encode_utf16().flat_map(u16::to_le_bytes).collect();
        let wide = cstrrep(UTF16, 4, &utf16);
        assert_eq!(unsafe { from_cstrrep(wide.as_ptr().cast()) }, "Bob\u{e9}");
        assert_eq!(unsafe { from_cstrrep(std::ptr::null()) }, "");
    }

    #[test]
    fn interior_nuls_are_dropped() {
        assert_eq!(to_c("a\0b").as_bytes(), b"ab");
        assert_eq!(to_c("plain").as_bytes(), b"plain");
    }
}
