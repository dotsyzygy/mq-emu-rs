//! The event matcher shim (shim/events.cpp): MacroQuest's Blech, matching as
//! macros and Lua do, with the patterns that crash Blech refused. Each guard
//! has a test running Blech without it in a child process (Blech crashes the
//! process, so it can't be shown in this one).

use core::ffi::{c_char, c_uint, c_void};
use std::ffi::{CStr, CString};
use std::process::Command;

use mq_emu_sys::events::testing::mqemu_test_blech_unguarded;
use mq_emu_sys::events::*;

/// Expands `|Me|` to "Tester" and `|wide|` to 2047 x's; anything else to itself.
unsafe extern "system" fn print_var(name: *mut c_char, value: *mut c_char, cap: usize) -> c_uint {
    let name = unsafe { CStr::from_ptr(name) }.to_bytes();
    let expanded: Vec<u8> = match name {
        b"Me" => b"Tester".to_vec(),
        b"wide" => vec![b'x'; PRINT_VAR_CAP - 1],
        other => other.to_vec(),
    };
    let len = expanded.len().min(cap - 1);
    unsafe {
        std::ptr::copy_nonoverlapping(expanded.as_ptr().cast(), value, len);
        *value.add(len) = 0;
    }
    len as c_uint
}

type Match = (u64, Vec<(String, String)>);

unsafe extern "C" fn record(user: *mut c_void, key: u64, names: *const *const c_char, values: *const *const c_char, count: usize) {
    let matches = unsafe { &mut *user.cast::<Vec<Match>>() };
    let text = |p: *const c_char| unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    let pairs = (0..count).map(|i| unsafe { (text(*names.add(i)), text(*values.add(i))) }).collect();
    matches.push((key, pairs));
}

struct Matcher(*mut mqemu_matcher);

impl Matcher {
    fn new() -> Matcher {
        let matcher = unsafe { mqemu_matcher_new(Some(print_var)) };
        assert!(!matcher.is_null());
        Matcher(matcher)
    }

    fn add(&self, key: u64, pattern: &str) -> i32 {
        let pattern = CString::new(pattern).unwrap();
        unsafe { mqemu_matcher_add(self.0, pattern.as_ptr(), key) }
    }

    fn feed(&self, line: &str) -> Vec<Match> {
        let line = CString::new(line).unwrap();
        let mut matches: Vec<Match> = Vec::new();
        assert!(unsafe { mqemu_matcher_feed(self.0, line.as_ptr(), record, (&raw mut matches).cast()) });
        matches
    }

    fn depth(&self) -> i32 {
        unsafe { mqemu_matcher_depth(self.0) }
    }
}

impl Drop for Matcher {
    fn drop(&mut self) {
        unsafe { mqemu_matcher_free(self.0) }
    }
}

fn values(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(n, v)| ((*n).to_owned(), (*v).to_owned())).collect()
}

#[test]
fn whole_lines_match_with_their_variables() {
    let m = Matcher::new();
    assert_eq!(m.add(1, "#1# tells you, '#2#'"), mqemu_event_status::OK);
    assert_eq!(m.feed("Bob tells you, 'hello there'"), [(1, values(&[("1", "Bob"), ("2", "hello there")]))]);
    // Case doesn't matter, but the whole line does.
    assert_eq!(m.feed("BOB TELLS YOU, 'hi'").len(), 1);
    assert!(m.feed("Bob tells you, 'hi' and more").is_empty());
    assert!(m.feed("Bob says, 'hi'").is_empty());
    assert!(m.feed("").is_empty());
}

#[test]
fn wildcards_names_and_escapes() {
    let m = Matcher::new();
    assert_eq!(m.add(1, "#*#You have slain #mob#!"), mqemu_event_status::OK);
    assert_eq!(m.feed("[12:00] You have slain a bat!"), [(1, values(&[("*", "[12:00] "), ("mob", "a bat")]))]);
    // ## and || are a literal # and |.
    assert_eq!(m.add(2, "Item ##3 || #1#"), mqemu_event_status::OK);
    assert_eq!(m.feed("Item #3 | ready"), [(2, values(&[("1", "ready")]))]);
}

#[test]
fn print_variables_are_expanded_when_matching() {
    let m = Matcher::new();
    assert_eq!(m.add(1, "|Me| hits #1# for #2# points of damage."), mqemu_event_status::OK);
    assert_eq!(m.feed("Tester hits a rat for 12 points of damage."), [(1, values(&[("1", "a rat"), ("2", "12")]))]);
    assert!(m.feed("Other hits a rat for 12 points of damage.").is_empty());
}

#[test]
fn every_matching_event_is_reported_in_the_order_added() {
    let m = Matcher::new();
    for (key, pattern) in [(3, "#*#"), (5, "#1# tells you, #*#"), (7, "nothing like it")] {
        assert_eq!(m.add(key, pattern), mqemu_event_status::OK);
    }
    // Blech itself reports them last added first.
    let keys: Vec<u64> = m.feed("Bob tells you, 'hi'").iter().map(|(key, _)| *key).collect();
    assert_eq!(keys, [3, 5]);
}

#[test]
fn removed_events_stop_matching() {
    let m = Matcher::new();
    assert_eq!(m.add(1, "ping #1#"), mqemu_event_status::OK);
    assert_eq!(m.add(2, "ping #*#"), mqemu_event_status::OK);
    assert!(unsafe { mqemu_matcher_remove(m.0, 1) });
    assert!(!unsafe { mqemu_matcher_remove(m.0, 1) });
    assert_eq!(m.feed("ping you"), [(2, values(&[("*", "you")]))]);
    assert!(unsafe { mqemu_matcher_remove(m.0, 2) });
    assert!(unsafe { mqemu_matcher_is_empty(m.0) });
    assert!(m.feed("ping you").is_empty());
    // A key can be used again once removed.
    assert_eq!(m.add(1, "ping"), mqemu_event_status::OK);
    assert_eq!(m.add(1, "pong"), mqemu_event_status::DUPLICATE_KEY);
}

#[test]
fn a_matcher_needs_a_print_variable_function() {
    assert!(unsafe { mqemu_matcher_new(None) }.is_null());
}

// ----- guards ----------------------------------------------------------------

#[test]
fn patterns_without_text_are_refused() {
    let m = Matcher::new();
    for pattern in ["", "#", "|", "#|", "|#", "####"] {
        // "####" is two literal #s: text, so allowed.
        let expected = if pattern == "####" { mqemu_event_status::OK } else { mqemu_event_status::NO_TEXT };
        assert_eq!(m.add(pattern.len() as u64, pattern), expected, "{pattern:?}");
    }
    assert_eq!(m.add(10, "##"), mqemu_event_status::OK);
    assert_eq!(m.add(11, "#1"), mqemu_event_status::OK, "an unclosed variable is still a part");
}

/// A pattern sitting `parts` deep: text and variables alternating.
fn deep_pattern(first: &str, parts: usize) -> String {
    let mut pattern = first.to_owned();
    for i in 1..parts {
        pattern.push_str(if i % 2 == 1 { "#v#" } else { "-" });
    }
    pattern
}

/// A line `deep_pattern` matches.
fn deep_line(first: &str, parts: usize) -> String {
    let mut line = first.to_owned();
    for i in 1..parts {
        line.push(if i % 2 == 1 { 'v' } else { '-' });
    }
    line
}

#[test]
fn patterns_as_deep_as_the_match_stack_allows_are_matched() {
    let m = Matcher::new();
    assert_eq!(m.add(1, &deep_pattern("ab", MAX_DEPTH as usize)), mqemu_event_status::OK);
    assert_eq!(m.depth(), MAX_DEPTH);
    let matches = m.feed(&deep_line("ab", MAX_DEPTH as usize));
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].1.len(), MAX_DEPTH as usize / 2);
}

#[test]
fn patterns_deeper_than_the_match_stack_are_refused() {
    let m = Matcher::new();
    assert_eq!(m.add(1, &deep_pattern("ab", MAX_DEPTH as usize + 1)), mqemu_event_status::TOO_DEEP);
    assert!(unsafe { mqemu_matcher_is_empty(m.0) });
}

#[test]
fn a_pattern_deepening_another_is_refused_and_leaves_it_as_it_was() {
    let m = Matcher::new();
    assert_eq!(m.add(1, &deep_pattern("ab", MAX_DEPTH as usize)), mqemu_event_status::OK);
    // Shares only "a" with it, so Blech splits its first node in two.
    assert_eq!(m.add(2, "ac"), mqemu_event_status::TOO_DEEP);
    assert_eq!(m.depth(), MAX_DEPTH);
    assert_eq!(m.feed(&deep_line("ab", MAX_DEPTH as usize)).len(), 1);
    assert!(m.feed("ac").is_empty());
    // One that doesn't share the beginning is fine.
    assert_eq!(m.add(3, "zz"), mqemu_event_status::OK);
    assert_eq!(m.feed("zz"), [(3, vec![])]);
}

/// A pattern with `count` print variables expanding to 2047 x's, after a variable.
fn wide_pattern(count: usize) -> String {
    format!("#1#{}", "|wide|".repeat(count))
}

fn wide_line(count: usize) -> String {
    format!("A{}", "x".repeat((PRINT_VAR_CAP - 1) * count))
}

#[test]
fn the_most_print_variables_allowed_still_fit() {
    let m = Matcher::new();
    assert_eq!(m.add(1, &wide_pattern(MAX_PRINT_VARS)), mqemu_event_status::OK);
    assert_eq!(m.feed(&wide_line(MAX_PRINT_VARS)), [(1, values(&[("1", "A")]))]);
    assert_eq!(m.add(2, &wide_pattern(MAX_PRINT_VARS + 1)), mqemu_event_status::TOO_MANY_PRINT_VARS);
}

#[test]
fn patterns_must_be_shorter_than_max_string() {
    let m = Matcher::new();
    let limit = mq_emu_sys::constants::MAX_STRING;
    assert_eq!(m.add(1, &"a".repeat(limit - 1)), mqemu_event_status::OK);
    assert_eq!(m.add(2, &"a".repeat(limit)), mqemu_event_status::TOO_LONG);
}

// ----- what Blech does without the guards ------------------------------------

/// Set in the child process, where the `crash_*` tests run.
const CHILD: &str = "MQ_EMU_EVENTS_CRASH_CHILD";

/// Runs the ignored test `name` in a child process and returns its exit code.
fn run_child(name: &str) -> u32 {
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--ignored", "--test-threads=1"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    output.status.code().expect("the child exited") as u32
}

fn unguarded(patterns: &[String], line: &str) {
    if std::env::var_os(CHILD).is_none() {
        return;
    }
    let patterns: Vec<CString> = patterns.iter().map(|p| CString::new(p.as_str()).unwrap()).collect();
    let pointers: Vec<*const c_char> = patterns.iter().map(|p| p.as_ptr()).collect();
    let line = CString::new(line).unwrap();
    unsafe { mqemu_test_blech_unguarded(pointers.as_ptr(), pointers.len(), line.as_ptr(), print_var) };
}

/// The access violation status.
const ACCESS_VIOLATION: u32 = 0xC000_0005;
/// What a `__fastfail` (the C runtime's invalid parameter handler, /GS)
/// exits with.
const FAST_FAIL: u32 = 0xC000_0409;

#[test]
fn unguarded_blech_runs_a_harmless_pattern() {
    // Positive control: the child setup itself doesn't crash.
    assert_eq!(run_child("crash_child_harmless"), 0);
}

#[test]
#[ignore = "run in a child process by unguarded_blech_runs_a_harmless_pattern"]
fn crash_child_harmless() {
    unguarded(&["#1# tells you".to_owned()], "Bob tells you");
}

#[test]
fn without_the_guard_a_pattern_without_text_crashes() {
    assert_eq!(run_child("crash_child_no_text"), ACCESS_VIOLATION);
}

#[test]
#[ignore = "run in a child process by without_the_guard_a_pattern_without_text_crashes"]
fn crash_child_no_text() {
    unguarded(&["#".to_owned()], "anything");
}

/// Blech's match stack has 100 entries and an 8-bit index. Deeper than
/// [`MAX_DEPTH`], it writes past the array (into the next local, silently, in
/// this build); at 256 the index wraps to 0 and Blech walks garbage.
const CRASHING_DEPTH: usize = 256;

#[test]
fn without_the_guard_a_deep_pattern_crashes_when_matched() {
    assert!(deep_pattern("ab", CRASHING_DEPTH).len() < mq_emu_sys::constants::MAX_STRING);
    assert_eq!(run_child("crash_child_too_deep"), ACCESS_VIOLATION);
}

#[test]
#[ignore = "run in a child process by without_the_guard_a_deep_pattern_crashes_when_matched"]
fn crash_child_too_deep() {
    unguarded(&[deep_pattern("ab", CRASHING_DEPTH)], &deep_line("ab", CRASHING_DEPTH));
}

#[test]
fn without_the_guard_wide_print_variables_crash_when_matched() {
    assert_eq!(run_child("crash_child_print_vars"), FAST_FAIL);
}

#[test]
#[ignore = "run in a child process by without_the_guard_wide_print_variables_crash_when_matched"]
fn crash_child_print_vars() {
    // One more than fits in Blech's buffer.
    let count = MAX_PRINT_VARS + 2;
    unguarded(&[wide_pattern(count)], &wide_line(count));
}

