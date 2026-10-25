//! Drives the C++ shim against a fake `MainInterface` (shim/test_fake.cpp), so
//! argument marshalling and callback routing are checked without MacroQuest.

use core::ffi::{c_char, c_int, c_void};
use std::ffi::{CStr, CString};

use mq_emu_sys::plugin::MQPluginHandle;
use mq_emu_sys::shim::testing::*;
use mq_emu_sys::shim::*;
use mq_emu_sys::types::PlayerClient;

/// Owns a fake MainInterface for the duration of a test.
struct Fake(*mut MainInterface);

impl Fake {
    fn new() -> Fake {
        Fake(unsafe { mqemu_test_fake_new() })
    }

    fn log(&self) -> FakeLog {
        unsafe { *mqemu_test_fake_log(self.0) }
    }

    fn invoke(&self, name: &str, player: *mut PlayerClient, line: &str) -> bool {
        let name = CString::new(name).unwrap();
        let line = CString::new(line).unwrap();
        unsafe { mqemu_test_fake_invoke(self.0, name.as_ptr(), player, line.as_ptr()) }
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        unsafe { mqemu_test_fake_free(self.0) }
    }
}

const HANDLE: MQPluginHandle = MQPluginHandle { pluginID: 0x1234_5678_9abc_def0 };

fn add(fake: &Fake, name: &str, callback: mqemu_command_fn, user: *mut c_void) -> bool {
    unsafe { mqemu_add_command(fake.0, &HANDLE, name.as_ptr().cast(), name.len(), callback, user, true, false, true) }
}

#[derive(Default)]
struct Calls {
    lines: Vec<String>,
    players: Vec<usize>,
}

unsafe extern "C" fn record(user: *mut c_void, player: *mut PlayerClient, line: *const c_char) {
    let calls = unsafe { &mut *user.cast::<Calls>() };
    calls.lines.push(unsafe { CStr::from_ptr(line) }.to_string_lossy().into_owned());
    calls.players.push(player as usize);
}

#[test]
fn added_command_passes_name_flags_and_handle() {
    let fake = Fake::new();
    let mut calls = Calls::default();
    assert!(add(&fake, "/rusttest", record, (&raw mut calls).cast()));

    let log = fake.log();
    assert_eq!(log.add_count, 1);
    assert_eq!(log.last_text(), c"/rusttest");
    assert_eq!((log.last_eq, log.last_parse, log.last_in_game), (true, false, true));
    assert_eq!(log.last_handle, HANDLE.pluginID);
}

#[test]
fn invoking_a_command_reaches_the_callback_with_its_user_pointer() {
    let fake = Fake::new();
    let mut calls = Calls::default();
    add(&fake, "/rusttest", record, (&raw mut calls).cast());

    let player = 0x4000usize as *mut PlayerClient;
    assert!(fake.invoke("/rusttest", player, "stats now"));
    assert!(fake.invoke("/rusttest", core::ptr::null_mut(), ""));
    assert_eq!(calls.lines, ["stats now", ""]);
    assert_eq!(calls.players, [0x4000, 0]);
}

#[test]
fn name_length_is_honoured_without_a_nul_terminator() {
    let fake = Fake::new();
    let mut calls = Calls::default();
    // Only the first 5 bytes of this buffer are the name.
    let buffer = "/ping and trailing garbage";
    assert!(unsafe { mqemu_add_command(fake.0, &HANDLE, buffer.as_ptr().cast(), 5, record, (&raw mut calls).cast(), false, true, false) });
    assert_eq!(fake.log().last_text(), c"/ping");
    assert!(unsafe { mqemu_is_command(fake.0, "/ping".as_ptr().cast(), 5) });
}

#[test]
fn removed_commands_are_gone() {
    let fake = Fake::new();
    let mut calls = Calls::default();
    add(&fake, "/gone", record, (&raw mut calls).cast());
    let is = |name: &str| unsafe { mqemu_is_command(fake.0, name.as_ptr().cast(), name.len()) };

    assert!(is("/gone"));
    assert!(unsafe { mqemu_remove_command(fake.0, &HANDLE, "/gone".as_ptr().cast(), 5) });
    assert!(!is("/gone"));
    assert!(!fake.invoke("/gone", core::ptr::null_mut(), "x"));
    assert!(calls.lines.is_empty());
}

#[test]
fn do_and_timed_commands_pass_their_arguments() {
    let fake = Fake::new();
    let line = CString::new("/echo hello").unwrap();

    assert!(unsafe { mqemu_do_command(fake.0, &HANDLE, line.as_ptr(), true) });
    let log = fake.log();
    assert_eq!((log.do_count, log.last_delayed, log.last_text()), (1, true, c"/echo hello"));
    assert_eq!(log.last_handle, HANDLE.pluginID);

    assert!(unsafe { mqemu_timed_command(fake.0, &HANDLE, line.as_ptr(), 1500) });
    let log = fake.log();
    assert_eq!((log.timed_count, log.last_ms), (1, 1500));
}

impl Fake {
    /// An alias's replacement and whether it was persisted, if it exists.
    fn alias(&self, name: &CStr) -> Option<(String, bool)> {
        let (mut out, mut persisted) = ([0 as c_char; 256], false);
        let found = unsafe { mqemu_test_fake_alias(self.0, name.as_ptr(), out.as_mut_ptr(), out.len(), &mut persisted) };
        found.then(|| (unsafe { CStr::from_ptr(out.as_ptr()) }.to_string_lossy().into_owned(), persisted))
    }
}

#[test]
fn aliases_round_trip() {
    let fake = Fake::new();
    let (short, long) = (CString::new("/rt").unwrap(), CString::new("/rusttest").unwrap());
    assert!(unsafe { mqemu_add_alias(fake.0, &HANDLE, short.as_ptr(), long.as_ptr(), false) });
    assert!(unsafe { mqemu_is_alias(fake.0, short.as_ptr()) });
    assert_eq!(fake.alias(&short), Some(("/rusttest".into(), false)));
    assert_eq!(fake.log().last_handle, HANDLE.pluginID);
    assert!(unsafe { mqemu_remove_alias(fake.0, &HANDLE, short.as_ptr()) });
    assert!(!unsafe { mqemu_is_alias(fake.0, short.as_ptr()) });
}

#[test]
fn persisting_an_alias_reaches_macroquest() {
    let fake = Fake::new();
    let (short, long) = (c"/rtp", c"/rusttest stats");
    assert!(unsafe { mqemu_add_alias(fake.0, &HANDLE, short.as_ptr(), long.as_ptr(), true) });
    assert_eq!(fake.alias(short), Some(("/rusttest stats".into(), true)));
}

// Negative tests.

#[test]
fn duplicate_command_is_rejected() {
    let fake = Fake::new();
    let mut calls = Calls::default();
    assert!(add(&fake, "/dup", record, (&raw mut calls).cast()));
    assert!(!add(&fake, "/dup", record, (&raw mut calls).cast()));
}

#[test]
fn duplicate_or_unknown_aliases_fail() {
    let fake = Fake::new();
    assert!(unsafe { mqemu_add_alias(fake.0, &HANDLE, c"/dupe".as_ptr(), c"/one".as_ptr(), false) });
    // MacroQuest matches alias names case-insensitively.
    assert!(!unsafe { mqemu_add_alias(fake.0, &HANDLE, c"/DUPE".as_ptr(), c"/two".as_ptr(), false) });
    assert_eq!(fake.alias(c"/dupe"), Some(("/one".into(), false)));
    assert!(!unsafe { mqemu_remove_alias(fake.0, &HANDLE, c"/nope".as_ptr()) });
    assert!(!unsafe { mqemu_is_alias(fake.0, c"/nope".as_ptr()) });
}

#[test]
fn removing_or_invoking_unknown_commands_fails() {
    let fake = Fake::new();
    assert!(!unsafe { mqemu_remove_command(fake.0, &HANDLE, "/nope".as_ptr().cast(), 5) });
    assert!(!unsafe { mqemu_is_command(fake.0, "/nope".as_ptr().cast(), 5) });
    assert!(!fake.invoke("/nope", core::ptr::null_mut(), ""));
}

#[test]
fn tests_do_not_depend_on_mq2main_dll() {
    // Only shim/main_interface.cpp imports from MQ2Main.dll; it must stay out
    // of binaries that don't call mqemu_main_interface, or they can't start
    // outside the game.
    use object::read::pe::PeFile32;
    let exe = std::fs::read(std::env::current_exe().unwrap()).unwrap();
    let file = PeFile32::parse(&*exe).unwrap();
    let imports = file.import_table().unwrap().unwrap();
    let mut descriptors = imports.descriptors().unwrap();
    while let Some(descriptor) = descriptors.next().unwrap() {
        let name = imports.name(descriptor.name.get(object::LittleEndian)).unwrap();
        assert!(!name.eq_ignore_ascii_case(b"MQ2Main.dll"), "test binary imports MQ2Main.dll");
    }
}

// ----- actors -------------------------------------------------------------------

use std::cell::RefCell;

/// What the actor callbacks saw, through their `user` pointer.
#[derive(Default)]
struct ActorLog {
    received: Vec<(Option<Vec<u8>>, Option<String>)>,
    responses: Vec<(i32, Option<Vec<u8>>)>,
    drops: u32,
}

thread_local! {
    static ACTOR_LOG: RefCell<ActorLog> = RefCell::new(ActorLog::default());
}

fn take_payload(message: *mut mqemu_message) -> Option<Vec<u8>> {
    let (mut data, mut len) = (std::ptr::null(), 0usize);
    let has = unsafe { mqemu_message_payload(message, &mut data, &mut len) };
    has.then(|| unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len) }.to_vec())
}

fn sender_character(message: *mut mqemu_message) -> Option<String> {
    let mut sender = mqemu_address::default();
    if !unsafe { mqemu_message_sender(message, &mut sender) } || sender.character.is_null() {
        return None;
    }
    Some(unsafe { CStr::from_ptr(sender.character) }.to_string_lossy().into_owned())
}

unsafe extern "C" fn on_receive(_user: *mut c_void, message: *mut mqemu_message) {
    let entry = (take_payload(message), sender_character(message));
    unsafe { mqemu_message_free(message) };
    ACTOR_LOG.with(|log| log.borrow_mut().received.push(entry));
}

unsafe extern "C" fn on_response(_user: *mut c_void, status: c_int, message: *mut mqemu_message) {
    let payload = if message.is_null() { None } else { take_payload(message) };
    if !message.is_null() {
        unsafe { mqemu_message_free(message) };
    }
    ACTOR_LOG.with(|log| log.borrow_mut().responses.push((status, payload)));
}

unsafe extern "C" fn on_drop(_user: *mut c_void) {
    ACTOR_LOG.with(|log| log.borrow_mut().drops += 1);
}

fn drops() -> u32 {
    ACTOR_LOG.with(|log| log.borrow().drops)
}

fn last_sent(fake: &Fake) -> String {
    let mut buffer = [0 as c_char; 512];
    unsafe { mqemu_test_fake_last_sent(fake.0, buffer.as_mut_ptr(), buffer.len()) };
    unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().into_owned()
}

fn add_actor(fake: &Fake, name: &str) -> *mut Dropbox {
    let name = CString::new(name).unwrap();
    unsafe { mqemu_add_actor(fake.0, &HANDLE, name.as_ptr(), on_receive, std::ptr::null_mut(), Some(on_drop)) }
}

#[test]
fn address_layout_matches_cpp() {
    assert_eq!(unsafe { mqemu_address_size() }, size_of::<mqemu_address>());
}

#[test]
fn mailboxes_receive_messages_with_payload_and_sender() {
    let fake = Fake::new();
    let dropbox = add_actor(&fake, "MQ2Test:follow");
    assert!(!dropbox.is_null());

    let (name, from, data) = (CString::new("MQ2Test:follow").unwrap(), CString::new("Bob").unwrap(), b"come here");
    assert!(unsafe { mqemu_test_fake_deliver(fake.0, name.as_ptr(), from.as_ptr(), std::ptr::null(), data.as_ptr().cast(), data.len()) });
    // A message without a payload.
    assert!(unsafe { mqemu_test_fake_deliver(fake.0, name.as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0) });

    let received = ACTOR_LOG.with(|log| log.borrow().received.clone());
    assert_eq!(received, [(Some(b"come here".to_vec()), Some("Bob".to_owned())), (None, None)]);
}

#[test]
fn removing_a_mailbox_drops_its_handler_exactly_once() {
    let fake = Fake::new();
    let dropbox = add_actor(&fake, "MQ2Test:gone");
    assert_eq!(drops(), 0);
    assert!(unsafe { mqemu_remove_actor(fake.0, &HANDLE, dropbox) });
    assert_eq!(drops(), 1);
    let name = CString::new("MQ2Test:gone").unwrap();
    assert!(!unsafe { mqemu_test_fake_deliver(fake.0, name.as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0) });
}

#[test]
fn a_plain_send_drops_its_user_at_once() {
    let fake = Fake::new();
    let (character, mailbox) = (CString::new("Bob").unwrap(), CString::new("follow").unwrap());
    let address = mqemu_address { character: character.as_ptr(), mailbox: mailbox.as_ptr(), ..Default::default() };
    let data = b"hi";
    assert!(unsafe {
        mqemu_send(fake.0, &HANDLE, std::ptr::null_mut(), &address, data.as_ptr().cast(), data.len(), None, std::ptr::null_mut(), Some(on_drop))
    });
    assert_eq!(drops(), 1);
    assert_eq!(last_sent(&fake), "dropbox=0 callback=0 character=Bob mailbox=follow absolute=0 data=hi");
}

#[test]
fn a_request_gets_its_response_and_then_drops() {
    let fake = Fake::new();
    let dropbox = add_actor(&fake, "MQ2Test");
    let character = CString::new("Bob").unwrap();
    let address = mqemu_address { character: character.as_ptr(), absolute_mailbox: true, ..Default::default() };
    let data = b"ping";
    assert!(unsafe {
        mqemu_send(fake.0, &HANDLE, dropbox, &address, data.as_ptr().cast(), data.len(), Some(on_response), std::ptr::null_mut(), Some(on_drop))
    });
    assert_eq!(last_sent(&fake), "dropbox=1 callback=1 character=Bob mailbox=- absolute=1 data=ping");
    assert_eq!(drops(), 0, "still waiting for the response");

    let pong = b"pong";
    assert!(unsafe { mqemu_test_fake_respond(fake.0, 0, pong.as_ptr().cast(), pong.len()) });
    assert_eq!(ACTOR_LOG.with(|log| log.borrow().responses.clone()), [(0, Some(b"pong".to_vec()))]);
    assert_eq!(drops(), 1, "the request's user is freed after its one response");
    assert!(!unsafe { mqemu_test_fake_respond(fake.0, 0, std::ptr::null(), 0) }, "nothing left pending");
}

#[test]
fn an_abandoned_request_still_drops_its_user() {
    let fake = Fake::new();
    let dropbox = add_actor(&fake, "MQ2Test");
    let address = mqemu_address::default();
    assert!(unsafe { mqemu_send(fake.0, &HANDLE, dropbox, &address, std::ptr::null(), 0, Some(on_response), std::ptr::null_mut(), Some(on_drop)) });
    unsafe { mqemu_test_fake_drop_pending(fake.0) };
    assert_eq!(drops(), 1);
    assert!(ACTOR_LOG.with(|log| log.borrow().responses.is_empty()));
}

/// Keeps a received message, for replying after the handler returns.
static mut KEPT: *mut mqemu_message = std::ptr::null_mut();

unsafe extern "C" fn keep_message(_user: *mut c_void, message: *mut mqemu_message) {
    unsafe { KEPT = message };
}

#[test]
fn a_kept_message_can_be_replied_to_later() {
    let fake = Fake::new();
    let name = CString::new("MQ2Test:rpc").unwrap();
    let dropbox = unsafe { mqemu_add_actor(fake.0, &HANDLE, name.as_ptr(), keep_message, std::ptr::null_mut(), None) };
    assert!(unsafe { mqemu_test_fake_deliver(fake.0, name.as_ptr(), std::ptr::null(), std::ptr::null(), std::ptr::null(), 0) });

    let kept = unsafe { KEPT };
    assert!(!kept.is_null());
    let answer = b"done";
    assert!(unsafe { mqemu_reply(fake.0, &HANDLE, dropbox, kept, answer.as_ptr().cast(), answer.len(), 7) });
    unsafe { mqemu_message_free(kept) };

    let mut buffer = [0 as c_char; 64];
    let mut status = 0u8;
    assert!(unsafe { mqemu_test_fake_last_reply(fake.0, buffer.as_mut_ptr(), buffer.len(), &mut status) });
    assert_eq!(unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_bytes(), b"done");
    assert_eq!(status, 7);
}

// ----- the InitPluginHandle ABI ----------------------------------------------------

thread_local! {
    static RECEIVED: std::cell::Cell<Option<(usize, u64)>> = const { std::cell::Cell::new(None) };
}

/// Declared exactly like the `InitPluginHandle` that mq-emu's `plugin!` exports.
extern "C" fn init_plugin_handle(plugin: *mut mq_emu_sys::types::MQPlugin, handle: MQPluginHandle) {
    RECEIVED.with(|r| r.set(Some((plugin as usize, handle.pluginID))));
}

#[test]
fn init_plugin_handle_receives_the_handle_macroquest_passes() {
    // MacroQuest passes MQPluginHandle by value; it's a C++ class with its
    // own copy/move constructors, so MSVC may pass it differently from a
    // plain struct. Call our callee the way the loader does and compare.
    let plugin = 0x5000_1000usize as *mut mq_emu_sys::types::MQPlugin;
    let id = 0x1122_3344_5566_7788u64;
    unsafe { mqemu_test_call_init_plugin_handle(init_plugin_handle as *const c_void, plugin, id) };
    let received = RECEIVED.with(|r| r.get()).expect("InitPluginHandle was called");
    assert_eq!(received, (0x5000_1000, id), "received plugin {:#x}, handle {:#x}", received.0, received.1);
}

#[test]
fn plugin_name_comes_from_the_loaders_record() {
    let name = CString::new("RustPluginTest").unwrap();
    let plugin = unsafe { mqemu_test_new_plugin(name.as_ptr()) };
    let read = unsafe { CStr::from_ptr(mqemu_plugin_name(plugin)) };
    assert_eq!(read.to_bytes(), b"RustPluginTest");
    unsafe { mqemu_test_free_plugin(plugin) };
    assert!(unsafe { mqemu_plugin_name(std::ptr::null()) }.is_null());
}
