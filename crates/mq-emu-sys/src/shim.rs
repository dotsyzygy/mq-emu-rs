//! C wrappers around MacroQuest's C++ `MainInterface` (from `shim/*.cpp`,
//! compiled by `build.rs` against MacroQuest's headers and linked statically).
//!
//! Commands and aliases are only reachable through `MainInterface`, and
//! `AddCommand` takes a C++ `std::function`, so these functions build the C++
//! arguments for us. Every function takes the interface explicitly (normally
//! [`mqemu_main_interface`]) plus this plugin's [`MQPluginHandle`] from
//! `InitPluginHandle`. Strings passed as `(ptr, len)` need no NUL terminator;
//! `*const c_char` strings must be NUL-terminated. C++ exceptions never cross
//! into Rust: on one, functions return `false`.

use core::ffi::{c_char, c_int, c_void};
use core::marker::{PhantomData, PhantomPinned};

use crate::plugin::MQPluginHandle;
use crate::types::PlayerClient;

/// `mq::MainInterface`: MacroQuest's C++ interface for plugin services.
#[repr(C)]
pub struct MainInterface {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// A command handler. `user` is the pointer given to [`mqemu_add_command`];
/// `line` is the text after the command name.
pub type mqemu_command_fn = unsafe extern "C" fn(user: *mut c_void, player: *mut PlayerClient, line: *const c_char);

unsafe extern "C" {
    /// MacroQuest's `MainInterface` (`mq::GetMainInterface()`). Only valid
    /// inside a process where MacroQuest is loaded.
    pub fn mqemu_main_interface() -> *mut MainInterface;

    /// Registers `/name` to call `callback(user, ...)`. `user` must stay valid
    /// until the command is removed. Defaults in C++: eq = false, parse = true,
    /// in_game = false.
    pub fn mqemu_add_command(
        main: *mut MainInterface,
        handle: *const MQPluginHandle,
        name: *const c_char,
        name_len: usize,
        callback: mqemu_command_fn,
        user: *mut c_void,
        eq: bool,
        parse: bool,
        in_game: bool,
    ) -> bool;
    pub fn mqemu_remove_command(main: *mut MainInterface, handle: *const MQPluginHandle, name: *const c_char, name_len: usize) -> bool;
    pub fn mqemu_is_command(main: *mut MainInterface, name: *const c_char, name_len: usize) -> bool;
    /// Runs `line` as if typed. `delayed = true` queues it for the next pulse,
    /// which is what plugins normally want.
    pub fn mqemu_do_command(main: *mut MainInterface, handle: *const MQPluginHandle, line: *const c_char, delayed: bool) -> bool;
    pub fn mqemu_timed_command(main: *mut MainInterface, handle: *const MQPluginHandle, line: *const c_char, ms_delay: c_int) -> bool;
    pub fn mqemu_add_alias(
        main: *mut MainInterface,
        handle: *const MQPluginHandle,
        short_command: *const c_char,
        long_command: *const c_char,
        persist: bool,
    ) -> bool;
    pub fn mqemu_remove_alias(main: *mut MainInterface, handle: *const MQPluginHandle, short_command: *const c_char) -> bool;
    pub fn mqemu_is_alias(main: *mut MainInterface, alias: *const c_char) -> bool;
}

/// A fake `MainInterface` that records command API calls, for testing code
/// built on the shim without MacroQuest (`shim/test_fake.cpp`). It is only
/// linked into binaries that use it.
#[doc(hidden)]
pub mod testing {
    use super::*;

    /// What the fake has seen. Mirrors `mqemu_fake_log` in test_fake.cpp.
    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub struct FakeLog {
        pub add_count: c_int,
        pub remove_count: c_int,
        pub do_count: c_int,
        pub timed_count: c_int,
        pub last_eq: bool,
        pub last_parse: bool,
        pub last_in_game: bool,
        pub last_delayed: bool,
        pub last_ms: c_int,
        pub last_handle: u64,
        pub last_text: [c_char; 256],
    }

    impl FakeLog {
        /// `last_text` up to its NUL terminator (the fake always writes one).
        pub fn last_text(&self) -> &core::ffi::CStr {
            let bytes = unsafe { core::slice::from_raw_parts(self.last_text.as_ptr().cast::<u8>(), self.last_text.len()) };
            core::ffi::CStr::from_bytes_until_nul(bytes).unwrap_or_default()
        }
    }

    unsafe extern "C" {
        pub fn mqemu_test_fake_new() -> *mut MainInterface;
        pub fn mqemu_test_fake_free(fake: *mut MainInterface);
        pub fn mqemu_test_fake_log(fake: *mut MainInterface) -> *const FakeLog;
        /// Calls a registered command the way MacroQuest would; false if unknown.
        pub fn mqemu_test_fake_invoke(fake: *mut MainInterface, name: *const c_char, player: *mut PlayerClient, line: *const c_char) -> bool;
    }
}
