//! MacroQuest's chat event matching (`shim/events.cpp`): Blech, the matcher
//! behind macros' `#Event` and Lua's `mq.event`, compiled in from
//! MacroQuest's `contrib/Blech`.
//!
//! A pattern is text to match a whole chat line against, case-insensitively.
//! `#name#` captures any text (`#1#`, `#2#`... by number, `#*#` to skip it),
//! and `|text|` is a print variable: `text` expanded by the matcher's
//! [`mqemu_print_var_fn`] when a line is matched (MacroQuest's expands
//! `${...}` data). `##` and `||` stand for `#` and `|`.
//!
//! [`mqemu_matcher_add`] refuses the patterns Blech would crash on (see
//! [`mqemu_event_status`]). These functions import nothing from MacroQuest,
//! so they run anywhere, tests included.

use core::ffi::{c_char, c_uint, c_void};
use core::marker::{PhantomData, PhantomPinned};

/// A set of patterns to match chat lines against.
#[repr(C)]
pub struct mqemu_matcher {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// Expands a `|print variable|`: `name` is the text between the bars. Writes
/// the value into `value` (`cap` bytes with the NUL; [`PRINT_VAR_CAP`]) and
/// returns its length, which must be its `strlen`. Blech's `fBlechVariableValue`.
pub type mqemu_print_var_fn = unsafe extern "system" fn(name: *mut c_char, value: *mut c_char, cap: usize) -> c_uint;

/// One match: the event's key, and its `#variables#` as `count` names and
/// values (NUL-terminated, valid during the call) in the pattern's order.
pub type mqemu_event_match_fn =
    unsafe extern "C" fn(user: *mut c_void, key: u64, names: *const *const c_char, values: *const *const c_char, count: usize);

/// What [`mqemu_matcher_add`] returns.
pub mod mqemu_event_status {
    pub const OK: i32 = 0;
    /// No text between the delimiters (`""`, `"#"`, `"|#"`): Blech would
    /// write through a null node.
    pub const NO_TEXT: i32 = 1;
    /// The pattern would sit deeper in Blech's tree than [`MAX_DEPTH`](super::MAX_DEPTH)
    /// (or make another pattern do so): matching would write past Blech's
    /// fixed match stack.
    pub const TOO_DEEP: i32 = 2;
    /// More than [`MAX_PRINT_VARS`](super::MAX_PRINT_VARS) print variables:
    /// their values could overflow the buffer Blech gathers text in, which
    /// ends the process.
    pub const TOO_MANY_PRINT_VARS: i32 = 3;
    /// [`MAX_STRING`](crate::constants::MAX_STRING) bytes or longer.
    pub const TOO_LONG: i32 = 4;
    /// The matcher already has an event with this key.
    pub const DUPLICATE_KEY: i32 = 5;
    /// A C++ exception (out of memory).
    pub const FAILED: i32 = 6;
}

/// How deep a pattern may sit in Blech's tree: one node per piece of text
/// or variable, more where patterns share a beginning.
pub const MAX_DEPTH: i32 = 98;
/// Most `|print variables|` a pattern may have.
pub const MAX_PRINT_VARS: usize = 7;
/// The room a print variable's value gets, NUL included.
pub const PRINT_VAR_CAP: usize = crate::constants::MAX_STRING;

unsafe extern "C" {
    /// A matcher whose print variables `print_var` expands; null if it's null.
    pub fn mqemu_matcher_new(print_var: Option<mqemu_print_var_fn>) -> *mut mqemu_matcher;
    pub fn mqemu_matcher_free(matcher: *mut mqemu_matcher);
    /// Adds `pattern` as event `key`; an [`mqemu_event_status`]. A refused
    /// pattern leaves the matcher as it was.
    pub fn mqemu_matcher_add(matcher: *mut mqemu_matcher, pattern: *const c_char, key: u64) -> i32;
    /// Removes event `key`; false if there's none.
    pub fn mqemu_matcher_remove(matcher: *mut mqemu_matcher, key: u64) -> bool;
    pub fn mqemu_matcher_is_empty(matcher: *const mqemu_matcher) -> bool;
    /// How deep the deepest pattern sits in Blech's tree.
    pub fn mqemu_matcher_depth(matcher: *const mqemu_matcher) -> i32;
    /// Matches `line`, then calls `on_match` for each event that matched, in
    /// the order the events were added. `on_match` must not use the matcher.
    /// False if matching failed (out of memory).
    pub fn mqemu_matcher_feed(matcher: *mut mqemu_matcher, line: *const c_char, on_match: mqemu_event_match_fn, user: *mut c_void) -> bool;
}

/// For tests only.
pub mod testing {
    use core::ffi::c_char;

    use super::mqemu_print_var_fn;

    unsafe extern "C" {
        /// Blech as MacroQuest uses it, without the shim's guards: adds
        /// `patterns`, then feeds `line`. Turns off Windows' crash dialog
        /// first; meant for a child process showing what a guard prevents.
        pub fn mqemu_test_blech_unguarded(patterns: *const *const c_char, count: usize, line: *const c_char, print_var: mqemu_print_var_fn) -> u32;
    }
}
