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

// ----- actors (mq/api/ActorAPI.h) -----

/// `mq::postoffice::Dropbox`: a registered mailbox.
#[repr(C)]
pub struct Dropbox {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// A received message, kept alive (and repliable) until [`mqemu_message_free`].
#[repr(C)]
pub struct mqemu_message {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// `mq::postoffice::Address` as C: strings are NUL-terminated, null when absent.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct mqemu_address {
    pub uuid: *const c_char,
    pub peer_ip: *const c_char,
    pub name: *const c_char,
    pub mailbox: *const c_char,
    pub account: *const c_char,
    pub server: *const c_char,
    pub character: *const c_char,
    pub pid: u32,
    pub peer_port: u16,
    pub has_pid: bool,
    pub has_peer_port: bool,
    /// Use `mailbox` as given rather than prefixed with the sending plugin's name.
    pub absolute_mailbox: bool,
}

impl Default for mqemu_address {
    fn default() -> Self {
        let null = core::ptr::null();
        mqemu_address {
            uuid: null,
            peer_ip: null,
            name: null,
            mailbox: null,
            account: null,
            server: null,
            character: null,
            pid: 0,
            peer_port: 0,
            has_pid: false,
            has_peer_port: false,
            absolute_mailbox: false,
        }
    }
}

/// Frees the Rust state behind a callback's `user` pointer.
pub type mqemu_drop_fn = unsafe extern "C" fn(user: *mut c_void);
/// A mailbox received `message`, which the callee now owns (free it with [`mqemu_message_free`]).
pub type mqemu_receive_fn = unsafe extern "C" fn(user: *mut c_void, message: *mut mqemu_message);
/// A request's outcome: `status` >= 0 is the replier's status, < 0 a routing
/// failure (`ResponseStatus`). `message` (may be null) is owned by the callee.
pub type mqemu_response_fn = unsafe extern "C" fn(user: *mut c_void, status: c_int, message: *mut mqemu_message);

unsafe extern "C" {
    /// Registers a mailbox at `local_address`. Ownership of `user` passes to
    /// C++: `drop(user)` runs exactly once, when MacroQuest destroys the
    /// handler, or before returning if registration fails (null result).
    pub fn mqemu_add_actor(
        main: *mut MainInterface,
        handle: *const MQPluginHandle,
        local_address: *const c_char,
        receive: mqemu_receive_fn,
        user: *mut c_void,
        drop: Option<mqemu_drop_fn>,
    ) -> *mut Dropbox;
    pub fn mqemu_remove_actor(main: *mut MainInterface, handle: *const MQPluginHandle, dropbox: *mut Dropbox) -> bool;
    /// Sends `data` to `address`. With `respond`, a request (needs a
    /// `dropbox`). Ownership of `user` passes to C++ as for `mqemu_add_actor`:
    /// `drop` runs exactly once, after the last possible `respond` (at once if
    /// there's no `respond` or sending fails).
    pub fn mqemu_send(
        main: *mut MainInterface,
        handle: *const MQPluginHandle,
        dropbox: *mut Dropbox,
        address: *const mqemu_address,
        data: *const c_char,
        data_len: usize,
        respond: Option<mqemu_response_fn>,
        user: *mut c_void,
        drop: Option<mqemu_drop_fn>,
    ) -> bool;
    /// Replies to `message` through `dropbox`, the mailbox that received it.
    pub fn mqemu_reply(
        main: *mut MainInterface,
        handle: *const MQPluginHandle,
        dropbox: *mut Dropbox,
        message: *const mqemu_message,
        data: *const c_char,
        data_len: usize,
        status: u8,
    ) -> bool;
    pub fn mqemu_message_free(message: *mut mqemu_message);
    /// The payload, if any; valid while the message is alive.
    pub fn mqemu_message_payload(message: *const mqemu_message, data: *mut *const c_char, len: *mut usize) -> bool;
    /// The sender, if known; strings valid while the message is alive.
    pub fn mqemu_message_sender(message: *const mqemu_message, out: *mut mqemu_address) -> bool;
    /// `sizeof(mqemu_address)` in C++.
    pub fn mqemu_address_size() -> usize;
    /// The plugin's name as MacroQuest knows it: the DLL name without the
    /// "MQ2"/"MQ" prefix and ".dll" (MQ2Example.dll is "Example"). MacroQuest
    /// names and addresses the plugin's mailboxes with it. Null for null.
    pub fn mqemu_plugin_name(plugin: *const crate::types::MQPlugin) -> *const c_char;
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

        // The fake post office.
        /// Delivers a message to the mailbox at `local_address`; false if none.
        /// `data` may be null (no payload).
        pub fn mqemu_test_fake_deliver(
            fake: *mut MainInterface,
            local_address: *const c_char,
            sender_character: *const c_char,
            sender_mailbox: *const c_char,
            data: *const c_char,
            len: usize,
        ) -> bool;
        /// Answers the oldest pending request; false if none.
        pub fn mqemu_test_fake_respond(fake: *mut MainInterface, status: c_int, data: *const c_char, len: usize) -> bool;
        /// Forgets pending requests without answering them.
        pub fn mqemu_test_fake_drop_pending(fake: *mut MainInterface);
        pub fn mqemu_test_fake_actor_count(fake: *mut MainInterface) -> usize;
        pub fn mqemu_test_fake_has_actor(fake: *mut MainInterface, local_address: *const c_char) -> bool;
        pub fn mqemu_test_fake_sent_count(fake: *mut MainInterface) -> usize;
        pub fn mqemu_test_fake_pending_count(fake: *mut MainInterface) -> usize;
        pub fn mqemu_test_fake_reply_count(fake: *mut MainInterface) -> usize;
        /// The last send as "dropbox=.. callback=.. character=.. mailbox=.. absolute=.. data=..".
        pub fn mqemu_test_fake_last_sent(fake: *mut MainInterface, out: *mut c_char, out_len: usize) -> usize;
        pub fn mqemu_test_fake_last_reply(fake: *mut MainInterface, out: *mut c_char, out_len: usize, status: *mut u8) -> bool;

        /// Calls `init` (a plugin's `InitPluginHandle`) exactly as MacroQuest's
        /// loader does, with a handle holding `plugin_id`.
        pub fn mqemu_test_call_init_plugin_handle(init: *const c_void, plugin: *mut crate::types::MQPlugin, plugin_id: u64);
        /// An MQPlugin record named `name`, as the loader makes.
        pub fn mqemu_test_new_plugin(name: *const c_char) -> *mut crate::types::MQPlugin;
        pub fn mqemu_test_free_plugin(plugin: *mut crate::types::MQPlugin);
    }
}
