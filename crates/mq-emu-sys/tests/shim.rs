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

// ----- items ----------------------------------------------------------------

mod item_definition {
    use mq_emu_sys::layouts::{ItemClient, ItemDefinition};
    use mq_emu_sys::shim::mqemu_item_definition;

    /// The definition our fake game implementation hands out.
    static mut DEFINITION: u8 = 0;

    unsafe extern "thiscall" fn get_unknown(_this: *const ItemClient) -> u32 {
        8
    }
    unsafe extern "thiscall" fn destructor(_this: *const ItemClient, _flags: u32) {}
    unsafe extern "thiscall" fn get_item_definition(_this: *const ItemClient) -> *mut ItemDefinition {
        (&raw mut DEFINITION).cast()
    }
    unsafe extern "thiscall" fn wrong_slot(_this: *const ItemClient) -> *mut ItemDefinition {
        core::ptr::null_mut()
    }

    /// An ItemClient whose vtable is ours, like one the game created.
    fn item_with(vtable: Vec<usize>) -> Vec<u64> {
        let vtable = vtable.leak();
        let mut storage = vec![0u64; size_of::<ItemClient>().div_ceil(8)];
        storage[0] = vtable.as_ptr() as u64; // __vfptr at 0 (32-bit: the low half)
        storage
    }

    #[test]
    fn the_definition_comes_from_the_vtable_not_the_fields() {
        // VeBaseReferenceCount's virtuals (getUnknown, the destructor), then
        // ItemBase::GetItemDefinition. Item1 and SharedItemDef stay zero.
        let item = item_with(vec![get_unknown as *const () as usize, destructor as *const () as usize, get_item_definition as *const () as usize, wrong_slot as *const () as usize]);
        let found = unsafe { mqemu_item_definition(item.as_ptr().cast()) };
        assert_eq!(found, (&raw mut DEFINITION).cast());
        assert!(unsafe { mqemu_item_definition(core::ptr::null()) }.is_null());
    }

    #[test]
    fn a_vtable_answering_null_gives_null() {
        let item = item_with(vec![get_unknown as *const () as usize, destructor as *const () as usize, wrong_slot as *const () as usize]);
        assert!(unsafe { mqemu_item_definition(item.as_ptr().cast()) }.is_null());
    }
}

// ----- subclassing windows ------------------------------------------------------

mod subclass {
    use core::ffi::c_void;
    use std::cell::{Cell, RefCell};
    use std::sync::Mutex;

    use mq_emu_sys::layouts::{CXPoint, CXWnd};
    use mq_emu_sys::windows::*;

    /// The shim keeps one list of subclasses; tests take turns with it.
    static SHIM: Mutex<()> = Mutex::new(());

    thread_local! {
        /// What the window's own functions and the hook saw, in order.
        static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
        /// Whether the hook says it handled what it gets.
        static HANDLE: Cell<bool> = const { Cell::new(false) };
        /// A window the hook takes its own subclass off of, when it's called.
        static REMOVE_IN_HOOK: Cell<Option<usize>> = const { Cell::new(None) };
    }

    fn log(line: String) {
        LOG.with(|log| log.borrow_mut().push(line));
    }

    fn take_log() -> Vec<String> {
        LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
    }

    // The window's own functions, as the game's class has them.
    unsafe extern "thiscall" fn own_notification(this: *mut CXWnd, sender: *mut CXWnd, message: u32, data: *mut c_void) -> i32 {
        log(format!("own notification {:#x} {:#x} {message} {}", this as usize, sender as usize, data as usize));
        7
    }
    unsafe extern "thiscall" fn own_post_draw(this: *mut CXWnd) -> i32 {
        log(format!("own post_draw {:#x}", this as usize));
        9
    }
    unsafe extern "thiscall" fn own_mouse(this: *mut CXWnd, pos: *const CXPoint, flags: u32) -> i32 {
        let pos = unsafe { &*pos };
        log(format!("own mouse {:#x} {} {} {flags}", this as usize, pos.x(), pos.y()));
        11
    }
    unsafe extern "thiscall" fn own_wheel(this: *mut CXWnd, pos: *const CXPoint, scroll: i32, flags: u32) -> i32 {
        let pos = unsafe { &*pos };
        log(format!("own wheel {:#x} {} {} {scroll} {flags}", this as usize, pos.x(), pos.y()));
        13
    }

    unsafe extern "C" fn hook(user: *mut c_void, window: *mut CXWnd, event: *const mqemu_window_event) -> bool {
        let e = unsafe { &*event };
        log(format!(
            "hook {} {:#x} kind {} sender {:#x} message {} data {} at {},{} flags {} scroll {}",
            user as usize, window as usize, e.kind, e.sender as usize, e.message, e.data as usize, e.x, e.y, e.flags, e.scroll
        ));
        if let Some(remove) = REMOVE_IN_HOOK.take() {
            log(format!("removed {}", unsafe { mqemu_window_unsubclass(remove as *mut CXWnd, true) }));
        }
        HANDLE.get()
    }

    const ENTRIES: usize = 1100;
    const RTTI: usize = 0x5151_5151;
    const ALL: u32 = (1 << HOOK_KINDS) - 1;

    fn slot(kind: i32) -> usize {
        let slot = unsafe { mqemu_window_hook_slot(kind) };
        usize::try_from(slot).unwrap_or_else(|_| panic!("no slot for kind {kind}"))
    }

    /// A vtable laid out as a game class's: the RTTI pointer, then numbered
    /// entries, with the window's own functions where the shim hooks.
    fn vtable() -> &'static [usize] {
        let mut table: Vec<usize> = (0..=ENTRIES).map(|i| 0x1000_0000 + i).collect();
        table[0] = RTTI;
        let entries = &mut table[1..];
        entries[slot(HOOK_NOTIFICATION)] = own_notification as *const () as usize;
        entries[slot(HOOK_DRAW)] = own_post_draw as *const () as usize;
        for kind in [HOOK_LBUTTON_DOWN, HOOK_LBUTTON_UP, HOOK_RBUTTON_DOWN, HOOK_RBUTTON_UP, HOOK_MOUSE_MOVE] {
            entries[slot(kind)] = own_mouse as *const () as usize;
        }
        entries[slot(HOOK_WHEEL)] = own_wheel as *const () as usize;
        &table.leak()[1..]
    }

    /// A window: its vtable pointer, then room for the rest.
    struct Window(Box<[usize; 64]>);

    impl Window {
        fn new(vtable: *const usize) -> Window {
            let mut storage = Box::new([0usize; 64]);
            storage[0] = vtable as usize;
            Window(storage)
        }

        fn ptr(&self) -> *mut CXWnd {
            self.0.as_ptr().cast_mut().cast()
        }

        fn vtable(&self) -> *const usize {
            self.0[0] as *const usize
        }

        fn set_vtable(&mut self, vtable: *const usize) {
            self.0[0] = vtable as usize;
        }

        fn entry(&self, kind: i32) -> usize {
            unsafe { *self.vtable().add(slot(kind)) }
        }

        // Calls through the window's vtable, as the game does.
        fn notify(&self, sender: usize, message: u32, data: usize) -> i32 {
            type F = unsafe extern "thiscall" fn(*mut CXWnd, *mut CXWnd, u32, *mut c_void) -> i32;
            let f: F = unsafe { std::mem::transmute(self.entry(HOOK_NOTIFICATION)) };
            unsafe { f(self.ptr(), sender as *mut CXWnd, message, data as *mut c_void) }
        }

        fn post_draw(&self) -> i32 {
            type F = unsafe extern "thiscall" fn(*mut CXWnd) -> i32;
            let f: F = unsafe { std::mem::transmute(self.entry(HOOK_DRAW)) };
            unsafe { f(self.ptr()) }
        }

        fn mouse(&self, kind: i32, x: i32, y: i32, flags: u32) -> i32 {
            type F = unsafe extern "thiscall" fn(*mut CXWnd, *const CXPoint, u32) -> i32;
            let f: F = unsafe { std::mem::transmute(self.entry(kind)) };
            let point = [x, y];
            unsafe { f(self.ptr(), point.as_ptr().cast(), flags) }
        }

        fn wheel(&self, x: i32, y: i32, scroll: i32, flags: u32) -> i32 {
            type F = unsafe extern "thiscall" fn(*mut CXWnd, *const CXPoint, i32, u32) -> i32;
            let f: F = unsafe { std::mem::transmute(self.entry(HOOK_WHEEL)) };
            let point = [x, y];
            unsafe { f(self.ptr(), point.as_ptr().cast(), scroll, flags) }
        }

        fn subclass(&self, mask: u32) -> bool {
            unsafe { mqemu_window_subclass(self.ptr(), hook, 42 as *mut c_void, mask) }
        }

        fn unsubclass(&self) -> bool {
            unsafe { mqemu_window_unsubclass(self.ptr(), true) }
        }
    }

    fn setup(handle: bool) -> std::sync::MutexGuard<'static, ()> {
        let guard = SHIM.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        take_log();
        HANDLE.set(handle);
        guard
    }

    #[test]
    fn the_slots_are_eqlibs() {
        // CXWnd::VirtualFunctionTable's offset comments, read from the header.
        let header = std::fs::read_to_string(std::path::Path::new(env!("MQ_ROOT")).join("src/eqlib/include/eqlib/game/CXWnd.h")).unwrap();
        let offset = |name: &str| -> usize {
            let entry = format!("void* {name};");
            let line = header.lines().find(|line| line.trim_end().ends_with(&entry)).unwrap_or_else(|| panic!("{name}"));
            let hex = line.trim().strip_prefix("/*0x").and_then(|rest| rest.split("*/").next()).unwrap();
            usize::from_str_radix(hex, 16).unwrap()
        };
        let expected = [
            (HOOK_NOTIFICATION, "WndNotification"),
            (HOOK_DRAW, "PostDraw"),
            (HOOK_LBUTTON_DOWN, "HandleLButtonDown"),
            (HOOK_LBUTTON_UP, "HandleLButtonUp"),
            (HOOK_RBUTTON_DOWN, "HandleRButtonDown"),
            (HOOK_RBUTTON_UP, "HandleRButtonUp"),
            (HOOK_MOUSE_MOVE, "HandleMouseMove"),
            (HOOK_WHEEL, "HandleWheelMove"),
        ];
        for (kind, name) in expected {
            assert_eq!(slot(kind) * 4, offset(name), "{name}");
        }
        assert_eq!(unsafe { mqemu_window_hook_slot(HOOK_KINDS) }, -1);
    }

    #[test]
    fn hooked_entries_reach_the_hook_and_then_the_window() {
        let _shim = setup(false);
        let window = Window::new(vtable().as_ptr());
        let w = window.ptr() as usize;
        assert!(window.subclass(ALL));
        assert_eq!(window.notify(0x77, 30, 5), 7, "the window's own result");
        assert_eq!(window.post_draw(), 9);
        assert_eq!(window.mouse(HOOK_LBUTTON_DOWN, 100, -20, 4), 11);
        assert_eq!(window.mouse(HOOK_RBUTTON_UP, 1, 2, 0), 11);
        assert_eq!(window.wheel(3, 4, -120, 8), 13);
        assert_eq!(
            take_log(),
            [
                format!("hook 42 {w:#x} kind 0 sender 0x77 message 30 data 5 at 0,0 flags 0 scroll 0"),
                format!("own notification {w:#x} 0x77 30 5"),
                format!("own post_draw {w:#x}"),
                format!("hook 42 {w:#x} kind 1 sender 0x0 message 0 data 0 at 0,0 flags 0 scroll 0"),
                format!("hook 42 {w:#x} kind 2 sender 0x0 message 0 data 0 at 100,-20 flags 4 scroll 0"),
                format!("own mouse {w:#x} 100 -20 4"),
                format!("hook 42 {w:#x} kind 5 sender 0x0 message 0 data 0 at 1,2 flags 0 scroll 0"),
                format!("own mouse {w:#x} 1 2 0"),
                format!("hook 42 {w:#x} kind 7 sender 0x0 message 0 data 0 at 3,4 flags 8 scroll -120"),
                format!("own wheel {w:#x} 3 4 -120 8"),
            ],
            "drawing comes after the window drew"
        );
        assert!(window.unsubclass());
    }

    #[test]
    fn a_handled_event_skips_the_window() {
        let _shim = setup(true);
        let window = Window::new(vtable().as_ptr());
        assert!(window.subclass(ALL));
        assert_eq!(window.notify(0, 1, 0), 0);
        assert_eq!(window.mouse(HOOK_MOUSE_MOVE, 5, 6, 0), 0);
        assert_eq!(window.post_draw(), 9, "drawing can't be handled: the window drew first");
        let own: Vec<String> = take_log().into_iter().filter(|line| line.starts_with("own")).collect();
        assert_eq!(own, [format!("own post_draw {:#x}", window.ptr() as usize)]);
        assert!(window.unsubclass());
    }

    #[test]
    fn only_the_kinds_asked_for_reach_the_hook() {
        let _shim = setup(true);
        let window = Window::new(vtable().as_ptr());
        assert!(window.subclass(1 << HOOK_DRAW));
        assert_eq!(window.notify(0, 1, 0), 7, "not asked for: the window has it");
        assert_eq!(window.mouse(HOOK_LBUTTON_UP, 0, 0, 0), 11);
        window.post_draw();
        let hooked: Vec<String> = take_log().into_iter().filter(|line| line.starts_with("hook")).collect();
        assert_eq!(hooked.len(), 1);
        assert!(hooked[0].contains("kind 1 "), "{hooked:?}");
        assert!(window.unsubclass());
    }

    #[test]
    fn the_rest_of_the_vtable_and_its_rtti_are_kept() {
        let _shim = setup(false);
        let original = vtable();
        let window = Window::new(original.as_ptr());
        assert!(window.subclass(ALL));
        assert_ne!(window.vtable(), original.as_ptr(), "a copy");
        let copy = unsafe { std::slice::from_raw_parts(window.vtable().sub(1), 1025) };
        assert_eq!(copy[0], RTTI);
        let hooked: Vec<usize> = (0..HOOK_KINDS).map(slot).collect();
        for (i, entry) in copy[1..].iter().enumerate() {
            if hooked.contains(&i) {
                assert_ne!(*entry, original[i], "{i} is replaced");
            } else {
                assert_eq!(*entry, original[i], "{i} is kept");
            }
        }
        assert!(window.unsubclass());
        assert_eq!(window.vtable(), original.as_ptr(), "put back");
        assert!(!window.unsubclass(), "only once");
    }

    #[test]
    fn a_window_is_subclassed_once() {
        let _shim = setup(false);
        let window = Window::new(vtable().as_ptr());
        assert!(window.subclass(ALL));
        let first = window.vtable();
        assert!(!window.subclass(ALL));
        assert_eq!(window.vtable(), first);
        assert!(!unsafe { mqemu_window_subclass(std::ptr::null_mut(), hook, std::ptr::null_mut(), ALL) });
        assert!(window.unsubclass());
        assert!(window.subclass(ALL), "again once taken off");
        assert!(window.unsubclass());
    }

    #[test]
    fn a_window_without_a_readable_vtable_is_refused() {
        let _shim = setup(false);
        let mut window = Window::new(std::ptr::null());
        assert!(!window.subclass(ALL), "no vtable");
        // Unmapped: the first 64 KB of the address space never are.
        window.set_vtable(0x100 as *const usize);
        assert!(!window.subclass(ALL), "a vtable that can't be read");
        assert_eq!(window.vtable(), 0x100 as *const usize, "left alone");
        assert!(!window.unsubclass());
    }

    #[test]
    fn another_window_sharing_the_table_only_reaches_its_own_functions() {
        // MacroQuest's WindowOverride gives a second window of a class the
        // patched table it made for the first; that table can be ours.
        let _shim = setup(true);
        let window = Window::new(vtable().as_ptr());
        assert!(window.subclass(ALL));
        let other = Window::new(window.vtable());
        assert_eq!(other.notify(1, 2, 3), 7);
        assert_eq!(take_log(), [format!("own notification {:#x} 0x1 2 3", other.ptr() as usize)]);
        assert!(window.unsubclass());
    }

    #[test]
    fn a_hook_on_top_keeps_working_after_ours_comes_off() {
        // Another hook copies our table (as WindowOverride copies whatever
        // the window has) and puts its copy on the window.
        let _shim = setup(false);
        let mut window = Window::new(vtable().as_ptr());
        assert!(window.subclass(ALL));
        let theirs: &'static [usize] = unsafe { std::slice::from_raw_parts(window.vtable().sub(1), 1025) }.to_vec().leak();
        window.set_vtable(theirs[1..].as_ptr());
        window.notify(0, 1, 0);
        assert_eq!(take_log().len(), 2, "through their copy to our hook, then the window");

        assert!(!window.unsubclass(), "not ours to put back");
        assert_eq!(window.vtable(), theirs[1..].as_ptr(), "theirs stays");
        assert_eq!(window.notify(0, 1, 0), 7);
        assert_eq!(window.post_draw(), 9);
        assert_eq!(window.wheel(1, 1, 1, 1), 13);
        let log = take_log();
        assert!(log.iter().all(|line| line.starts_with("own")), "the stubs they copied go straight to the window's functions: {log:?}");
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn a_deleted_window_is_left_alone() {
        // The game deleted the window; its memory may hold anything now.
        let _shim = setup(false);
        let mut window = Window::new(vtable().as_ptr());
        assert!(window.subclass(ALL));
        let ours = window.vtable();
        window.set_vtable(0x100 as *const usize);
        assert!(!unsafe { mqemu_window_unsubclass(window.ptr(), false) });
        assert_eq!(window.vtable(), 0x100 as *const usize, "not touched");
        // Anything still holding our table goes straight to the old functions.
        window.set_vtable(ours);
        assert_eq!(window.notify(0, 1, 0), 7);
        assert_eq!(take_log().len(), 1, "no hook");
        assert!(window.subclass(ALL), "forgotten: a new window there can be subclassed");
        assert!(window.unsubclass());
    }

    #[test]
    fn the_hook_can_take_its_own_subclass_off() {
        let _shim = setup(false);
        let original = vtable();
        let window = Window::new(original.as_ptr());
        assert!(window.subclass(ALL));
        REMOVE_IN_HOOK.set(Some(window.ptr() as usize));
        assert_eq!(window.notify(0, 1, 0), 7, "the window still gets it");
        let log = take_log();
        assert_eq!(log.len(), 3, "{log:?}");
        assert_eq!(log[1], "removed true");
        assert_eq!(window.vtable(), original.as_ptr());
        window.post_draw();
        assert_eq!(take_log().len(), 1, "no hook anymore");
    }
}
