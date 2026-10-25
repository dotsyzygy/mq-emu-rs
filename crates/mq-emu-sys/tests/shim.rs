//! Drives the C++ shim against a fake `MainInterface` (shim/test_fake.cpp), so
//! argument marshalling and callback routing are checked without MacroQuest.

use core::ffi::{c_char, c_void};
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

#[test]
fn aliases_round_trip() {
    let fake = Fake::new();
    let (short, long) = (CString::new("/rt").unwrap(), CString::new("/rusttest").unwrap());
    assert!(unsafe { mqemu_add_alias(fake.0, &HANDLE, short.as_ptr(), long.as_ptr(), false) });
    assert!(unsafe { mqemu_is_alias(fake.0, short.as_ptr()) });
    assert!(unsafe { mqemu_remove_alias(fake.0, &HANDLE, short.as_ptr()) });
    assert!(!unsafe { mqemu_is_alias(fake.0, short.as_ptr()) });
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
