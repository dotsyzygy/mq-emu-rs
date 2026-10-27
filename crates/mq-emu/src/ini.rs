//! Settings in INI files, read and written the way MacroQuest does it.
//!
//! MacroQuest's own helpers (`GetPrivateProfileString`, `WritePrivateProfileBool`
//! and friends in `mq/base/Config.h`) are inline wrappers around Windows'
//! `*PrivateProfile*A` functions, so this calls those functions directly. A
//! file written here reads the same through `${Ini[...]}`, `/ini`, Lua's
//! `mq.TLO.Ini` and C++ plugins, and the other way round. Every call reads or
//! writes the file itself (nothing is cached), so a change made by another
//! client, or by hand while the game runs, is seen by the next read.
//!
//! ```ignore
//! let ini = mq.ini(); // config\MQ2Example.ini
//! let radius: f32 = ini.get_parsed("Settings", "Radius").unwrap_or(50.0);
//! ini.set("Settings", "Radius", radius)?;
//! ```
//!
//! Text is passed to Windows as UTF-8 bytes, as MacroQuest does, and read back
//! lossily (a file saved by an editor in Windows-1252 still reads, with
//! replacement characters for what isn't UTF-8).

use std::ffi::{CString, OsString, c_char};
use std::fmt;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetPrivateProfileStringA(
        section: *const c_char,
        key: *const c_char,
        default: *const c_char,
        out: *mut c_char,
        size: u32,
        file: *const c_char,
    ) -> u32;
    fn WritePrivateProfileStringA(section: *const c_char, key: *const c_char, value: *const c_char, file: *const c_char) -> i32;
    fn GetPrivateProfileSectionNamesA(out: *mut c_char, size: u32, file: *const c_char) -> u32;
    fn WritePrivateProfileSectionA(section: *const c_char, entries: *const c_char, file: *const c_char) -> i32;
    fn MultiByteToWideChar(code_page: u32, flags: u32, text: *const c_char, len: i32, out: *mut u16, out_len: i32) -> i32;
    fn WideCharToMultiByte(
        code_page: u32,
        flags: u32,
        text: *const u16,
        len: i32,
        out: *mut c_char,
        out_len: i32,
        default_char: *const c_char,
        used_default: *mut i32,
    ) -> i32;
}

/// The ANSI code page, which the `A` functions take paths in.
const CP_ACP: u32 = 0;
/// Fail rather than substitute a look-alike for a character the code page lacks.
const WC_NO_BEST_FIT_CHARS: u32 = 0x400;
/// What MacroQuest reads into (`MAX_STRING`); longer text grows the buffer.
const FIRST_BUFFER: usize = 2048;

/// Why an INI file couldn't be used or written.
#[derive(Debug)]
pub enum IniError {
    /// The path has characters the system's ANSI code page can't represent,
    /// which Windows' INI functions (and so MacroQuest) can't open.
    UnrepresentablePath,
    /// Section names must be non-empty, one line, without `]` and without
    /// spaces at either end; key names likewise, without `=` and not starting
    /// with `[` or `;`.
    InvalidName,
    /// Values must be one line without NULs.
    InvalidValue,
    /// Windows couldn't write the file (a missing directory, say, or access denied).
    Io(io::Error),
}

impl fmt::Display for IniError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IniError::UnrepresentablePath => f.write_str("the path can't be represented in the system's ANSI code page"),
            IniError::InvalidName => f.write_str("INI section and key names must be one trimmed line without [ ] = or ;"),
            IniError::InvalidValue => f.write_str("INI values must be one line without NULs"),
            IniError::Io(error) => write!(f, "couldn't write the INI file: {error}"),
        }
    }
}

impl std::error::Error for IniError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IniError::Io(error) => Some(error),
            _ => None,
        }
    }
}

/// An INI file. Get the plugin's own with [`Mq::ini`](crate::Mq::ini).
///
/// Section and key names are case-insensitive, as in Windows. Reading a file
/// that doesn't exist finds nothing; the first write creates it (but not its
/// directory).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ini {
    path: PathBuf,
    /// `path` in the ANSI code page, for the `A` functions.
    ansi_path: CString,
}

impl Ini {
    /// The INI file at `path`. A relative path is taken from the current
    /// directory (Windows would otherwise look in its own directory); for one
    /// next to MacroQuest's, join it to [`Mq::config_dir`](crate::Mq::config_dir).
    pub fn new(path: impl AsRef<Path>) -> Result<Ini, IniError> {
        let path = std::path::absolute(path).map_err(IniError::Io)?;
        let ansi_path = to_ansi(path.as_os_str()).ok_or(IniError::UnrepresentablePath)?;
        Ok(Ini { path, ansi_path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A value, as `${Ini[file,section,key]}` reads it: spaces at either end
    /// and one pair of surrounding quotes are removed. `None` if the key isn't
    /// there; a key with nothing after its `=` is `Some("")`.
    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        let (section, key) = (name(section)?, name(key)?);
        let value = read(|out, size| unsafe {
            GetPrivateProfileStringA(section.as_ptr(), key.as_ptr(), c"".as_ptr(), out, size, self.ansi_path.as_ptr())
        });
        let value = String::from_utf8_lossy(&value).into_owned();
        // An empty result is also what a missing key gives.
        if value.is_empty() && !self.has_key(section.to_str().ok()?, key.to_str().ok()?) {
            return None;
        }
        Some(value)
    }

    /// A value parsed with [`FromStr`] after trimming. `None` if the key isn't
    /// there or doesn't parse. Stricter than MacroQuest's `GetPrivateProfileInt`,
    /// which reads `12abc` as 12 and anything else as 0.
    pub fn get_parsed<T: FromStr>(&self, section: &str, key: &str) -> Option<T> {
        self.get(section, key)?.trim().parse().ok()
    }

    /// A value read as a switch, by MacroQuest's rules (`GetPrivateProfileBool`):
    /// `true`/`on`/`yes` and `false`/`off`/`no` in any case, or a number,
    /// true unless 0. `None` if the key isn't there or is none of those.
    pub fn get_bool(&self, section: &str, key: &str) -> Option<bool> {
        parse_bool(&self.get(section, key)?)
    }

    /// Writes a value, creating the file, section or key if needed. Anything
    /// [`Display`](fmt::Display) works; `true`/`false` read back with
    /// [`get_bool`](Ini::get_bool) and in macros. Values that would come back
    /// changed (spaces at either end, or surrounded by quotes) are written in
    /// quotes, so [`get`](Ini::get) returns exactly what was set.
    pub fn set(&self, section: &str, key: &str, value: impl fmt::Display) -> Result<(), IniError> {
        let (section, key) = (name_checked(section, false)?, name_checked(key, true)?);
        let value = value_checked(&value.to_string())?;
        write(unsafe { WritePrivateProfileStringA(section.as_ptr(), key.as_ptr(), value.as_ptr(), self.ansi_path.as_ptr()) })
    }

    /// Deletes a key. Deleting one that isn't there isn't an error.
    pub fn remove(&self, section: &str, key: &str) -> Result<(), IniError> {
        let (section, key) = (name_checked(section, false)?, name_checked(key, true)?);
        write(unsafe { WritePrivateProfileStringA(section.as_ptr(), key.as_ptr(), std::ptr::null(), self.ansi_path.as_ptr()) })
    }

    /// The file's section names, in file order.
    pub fn sections(&self) -> Vec<String> {
        split(&read(|out, size| unsafe { GetPrivateProfileSectionNamesA(out, size, self.ansi_path.as_ptr()) }))
    }

    /// Deletes a section and everything in it. Deleting one that isn't there
    /// isn't an error.
    pub fn remove_section(&self, section: &str) -> Result<(), IniError> {
        let section = name_checked(section, false)?;
        write(unsafe { WritePrivateProfileStringA(section.as_ptr(), std::ptr::null(), std::ptr::null(), self.ansi_path.as_ptr()) })
    }

    /// A section's key names, in file order. Empty if there's no such section.
    pub fn keys(&self, section: &str) -> Vec<String> {
        let Some(section) = name(section) else { return Vec::new() };
        split(&read(|out, size| unsafe {
            GetPrivateProfileStringA(section.as_ptr(), std::ptr::null(), c"".as_ptr(), out, size, self.ansi_path.as_ptr())
        }))
    }

    /// A section's keys and their values (as [`get`](Ini::get) reads them),
    /// in file order.
    pub fn section(&self, section: &str) -> Vec<(String, String)> {
        let keys = self.keys(section);
        keys.into_iter()
            .filter_map(|key| {
                let value = self.get(section, &key)?;
                Some((key, value))
            })
            .collect()
    }

    /// Replaces a section's contents with `entries`, in order, creating the
    /// section if needed. Nothing is written if any name or value is invalid.
    pub fn set_section<K, V>(&self, section: &str, entries: impl IntoIterator<Item = (K, V)>) -> Result<(), IniError>
    where
        K: AsRef<str>,
        V: fmt::Display,
    {
        let section = name_checked(section, false)?;
        // "key=value\0key=value\0\0"
        let mut block = Vec::new();
        for (key, value) in entries {
            block.extend_from_slice(name_checked(key.as_ref(), true)?.as_bytes());
            block.push(b'=');
            block.extend_from_slice(value_checked(&value.to_string())?.as_bytes());
            block.push(0);
        }
        block.extend_from_slice(&[0, 0]);
        write(unsafe { WritePrivateProfileSectionA(section.as_ptr(), block.as_ptr().cast(), self.ansi_path.as_ptr()) })
    }

    fn has_key(&self, section: &str, key: &str) -> bool {
        self.keys(section).iter().any(|k| k.eq_ignore_ascii_case(key))
    }
}

/// Reads with a Windows INI function that fills `out` (`size` bytes) and
/// returns the length, growing the buffer while the result was cut short (the
/// functions return `size - 1` or `size - 2` then).
fn read(mut f: impl FnMut(*mut c_char, u32) -> u32) -> Vec<u8> {
    let mut buffer = vec![0u8; FIRST_BUFFER];
    loop {
        let len = f(buffer.as_mut_ptr().cast(), buffer.len() as u32) as usize;
        if len + 2 < buffer.len() {
            buffer.truncate(len);
            return buffer;
        }
        buffer = vec![0; buffer.len() * 2];
    }
}

fn write(ok: i32) -> Result<(), IniError> {
    if ok != 0 { Ok(()) } else { Err(IniError::Io(io::Error::last_os_error())) }
}

/// A NUL-separated list, as the key and section name functions return.
fn split(list: &[u8]) -> Vec<String> {
    list.split(|b| *b == 0).filter(|s| !s.is_empty()).map(|s| String::from_utf8_lossy(s).into_owned()).collect()
}

/// A name for lookups; `None` for one that can't be in a file, so reads find nothing.
fn name(name: &str) -> Option<CString> {
    CString::new(name).ok()
}

/// A name for writing, refusing what would land in the file as something else.
fn name_checked(name: &str, is_key: bool) -> Result<CString, IniError> {
    let forbidden = if is_key { ['=', '\r', '\n', '\0'] } else { [']', '\r', '\n', '\0'] };
    let valid = !name.is_empty()
        && name.trim() == name
        && !name.contains(forbidden)
        && !(is_key && (name.starts_with('[') || name.starts_with(';')));
    if !valid {
        return Err(IniError::InvalidName);
    }
    Ok(CString::new(name).expect("NULs refused"))
}

/// A value for writing, quoted when reading it back would otherwise change it.
fn value_checked(value: &str) -> Result<CString, IniError> {
    if value.contains(['\r', '\n', '\0']) {
        return Err(IniError::InvalidValue);
    }
    let quoted = |q: char| value.len() >= 2 && value.starts_with(q) && value.ends_with(q);
    let value = if value.trim() != value || quoted('"') || quoted('\'') { format!("\"{value}\"") } else { value.to_owned() };
    Ok(CString::new(value).expect("NULs refused"))
}

/// MacroQuest's `GetBoolFromString`, except that text it can't read is `None`
/// rather than the default.
fn parse_bool(text: &str) -> Option<bool> {
    let text = text.trim();
    for (word, value) in [("true", true), ("false", false), ("on", true), ("off", false), ("yes", true), ("no", false)] {
        if text.eq_ignore_ascii_case(word) {
            return Some(value);
        }
    }
    text.parse::<i32>().ok().map(|n| n != 0)
}

/// A path in the ANSI code page; `None` if it has characters the code page lacks.
fn to_ansi(path: &std::ffi::OsStr) -> Option<CString> {
    let wide: Vec<u16> = path.encode_wide().collect();
    if wide.is_empty() || wide.contains(&0) {
        return None;
    }
    let mut used_default = 0;
    let len = unsafe {
        WideCharToMultiByte(CP_ACP, WC_NO_BEST_FIT_CHARS, wide.as_ptr(), wide.len() as i32, std::ptr::null_mut(), 0, std::ptr::null(), &mut used_default)
    };
    let mut out = vec![0u8; usize::try_from(len).ok().filter(|len| *len > 0)?];
    unsafe {
        WideCharToMultiByte(
            CP_ACP,
            WC_NO_BEST_FIT_CHARS,
            wide.as_ptr(),
            wide.len() as i32,
            out.as_mut_ptr().cast(),
            len,
            std::ptr::null(),
            &mut used_default,
        )
    };
    if used_default != 0 {
        return None;
    }
    CString::new(out).ok()
}

/// A path MacroQuest keeps in the ANSI code page (`gPathConfig` and the like).
pub(crate) fn from_ansi(path: &[c_char]) -> PathBuf {
    let len = path.iter().position(|c| *c == 0).unwrap_or(path.len());
    if len == 0 {
        return PathBuf::new();
    }
    let wide_len = unsafe { MultiByteToWideChar(CP_ACP, 0, path.as_ptr(), len as i32, std::ptr::null_mut(), 0) };
    let mut wide = vec![0u16; wide_len.max(0) as usize];
    unsafe { MultiByteToWideChar(CP_ACP, 0, path.as_ptr(), len as i32, wide.as_mut_ptr(), wide_len) };
    PathBuf::from(OsString::from_wide(&wide))
}

#[cfg(test)]
mod tests;
