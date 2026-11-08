//! The C ABI between eqclient.dll and the plugins that use it. Plugins built
//! with different eq-client versions share one loaded DLL, so this only grows:
//! a new version adds fields at the end and raises [`API_VERSION`].

use std::ffi::CStr;
use std::fmt;

/// The version this eq-client speaks; a loaded DLL must be at least this.
pub const API_VERSION: u32 = 1;
/// The DLL's file name.
pub const DLL_NAME: &str = "eqclient.dll";
/// `extern "C" fn() -> *const Api`, exported by the DLL.
pub const EXPORT: &CStr = c"eqclient_api";
/// A [`DllVersion`], exported by the DLL as data so tools can read it from
/// the file without loading it (`cargo mq deploy` keeps the newer DLL).
pub const VERSION_EXPORT: &CStr = c"eqclient_version";

/// Which eqclient.dll is newer: its [`API_VERSION`], then the version of the
/// eq-client it was built from, compared in that order. Fixed layout: four
/// little-endian `u32`s.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DllVersion {
    pub api: u32,
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl DllVersion {
    /// This eq-client's.
    pub const CURRENT: DllVersion = {
        let v = env!("CARGO_PKG_VERSION").as_bytes();
        let (major, i) = number(v, 0);
        let (minor, i) = number(v, i + 1);
        let (patch, _) = number(v, i + 1);
        DllVersion { api: API_VERSION, major, minor, patch }
    };
}

/// The decimal number at `v[i..]`, and the index after it.
const fn number(v: &[u8], mut i: usize) -> (u32, usize) {
    let mut n = 0;
    while i < v.len() && v[i].is_ascii_digit() {
        n = n * 10 + (v[i] - b'0') as u32;
        i += 1;
    }
    (n, i)
}

/// What the DLL offers. Valid for the life of the process: the DLL never unloads.
#[repr(C)]
pub struct Api {
    /// The version the DLL speaks; fields past what it knows aren't there.
    pub version: u32,
    /// Starts recording these string IDs, hooking the client on first use.
    pub watch_string_ids: unsafe extern "C" fn(ids: *const u32, count: usize) -> Status,
    /// Where a new reader of recorded IDs starts.
    pub string_ids_head: unsafe extern "C" fn() -> u64,
    /// Copies up to `capacity` IDs recorded since `*cursor` to `out`, moves
    /// `*cursor` past them, and adds any it missed (fell too far behind) to
    /// `*missed`. Returns how many it copied.
    pub poll_string_ids: unsafe extern "C" fn(cursor: *mut u64, out: *mut u32, capacity: usize, missed: *mut u64) -> usize,
}

/// How [`Api::watch_string_ids`] went. Not an enum, so values from a newer
/// DLL can't be undefined behaviour.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Status(pub i32);

impl Status {
    /// The hook is in and the IDs are recorded.
    pub const HOOKED: Status = Status(0);
    /// This process isn't eqgame.exe.
    pub const NOT_THE_CLIENT: Status = Status(1);
    /// The client's code isn't what eq-client knows (another build, or already
    /// hooked by something else); nothing was patched.
    pub const UNKNOWN_CLIENT: Status = Status(2);
    /// Code memory couldn't be allocated or unprotected.
    pub const MEMORY: Status = Status(3);
    /// An ID was too large to watch; the others are watched.
    pub const ID_TOO_LARGE: Status = Status(4);

    pub fn is_hooked(self) -> bool {
        self == Status::HOOKED
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Status::HOOKED => f.write_str("hooked"),
            Status::NOT_THE_CLIENT => f.write_str("not running in eqgame.exe"),
            Status::UNKNOWN_CLIENT => f.write_str("the client's code isn't the RoF2 build eq-client knows, or something else hooked it first"),
            Status::MEMORY => f.write_str("couldn't allocate or unprotect code memory"),
            Status::ID_TOO_LARGE => f.write_str("a string ID was too large to watch"),
            Status(other) => write!(f, "unknown status {other}"),
        }
    }
}
