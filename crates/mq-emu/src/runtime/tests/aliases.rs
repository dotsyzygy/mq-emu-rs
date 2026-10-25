//! Aliases against the fake MainInterface, which follows MacroQuest's rules:
//! names are case-insensitive, an alias can't shadow a (non-EQ) command, and
//! removing one also deletes it from the ini.

use std::ffi::{CStr, c_char};

use super::*;

impl FakeMain {
    /// An alias's replacement and whether it's saved in the ini, if it exists.
    fn alias(&self, name: &str) -> Option<(String, bool)> {
        let name = CString::new(name).unwrap();
        let (mut out, mut persisted) = ([0 as c_char; 256], false);
        let found = unsafe { mqemu_test_fake_alias(self.0, name.as_ptr(), out.as_mut_ptr(), out.len(), &mut persisted) };
        found.then(|| (unsafe { CStr::from_ptr(out.as_ptr()) }.to_string_lossy().into_owned(), persisted))
    }
}

#[test]
fn temporary_aliases_are_added_and_removed_at_shutdown() {
    let main = FakeMain::install();
    let rt = runtime();
    rt.init_handle(ptr::null_mut(), MQPluginHandle { pluginID: 0x42 });
    set_hook(&ON_NEW, |plugin, mq| {
        let added = mq.add_alias("/rta", "/rusttest selftest");
        plugin.log.push(format!("{added:?} {}", mq.is_alias("/RTA")));
    });
    rt.initialize();
    assert_eq!(log_of(rt), ["new", "Ok(()) true"]);
    assert_eq!(main.alias("/rta"), Some(("/rusttest selftest".into(), false)));
    assert_eq!(main.log().last_handle, 0x42);

    rt.shutdown();
    assert_eq!(main.alias("/rta"), None, "MacroQuest would keep it after unload");
}

#[test]
fn persistent_aliases_are_saved_and_survive_shutdown() {
    let main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| mq.add_persistent_alias("/rtp", "/rusttest stats").unwrap());
    rt.initialize();
    rt.shutdown();
    assert_eq!(main.alias("/rtp"), Some(("/rusttest stats".into(), true)));
}

#[test]
fn removing_an_alias_forgets_it_for_shutdown() {
    let main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| {
        mq.add_alias("/rta", "/one").unwrap();
        plugin.log.push(format!("{} {}", mq.remove_alias("/rta"), mq.remove_alias("/rta")));
    });
    rt.initialize();
    assert_eq!(log_of(rt), ["new", "true false"]);

    // Someone else's alias by the same name now: shutdown must leave it.
    let other = MQPluginHandle { pluginID: 7 };
    assert!(unsafe { shim::mqemu_add_alias(main.0, &other, c"/rta".as_ptr(), c"/two".as_ptr(), true) });
    rt.shutdown();
    assert_eq!(main.alias("/rta"), Some(("/two".into(), true)));
}

#[test]
fn remove_alias_removes_any_alias_like_the_alias_command() {
    let main = FakeMain::install();
    let other = MQPluginHandle { pluginID: 7 };
    assert!(unsafe { shim::mqemu_add_alias(main.0, &other, c"/theirs".as_ptr(), c"/who".as_ptr(), true) });
    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| plugin.log.push(format!("{}", mq.remove_alias("/THEIRS"))));
    rt.initialize();
    assert_eq!(log_of(rt), ["new", "true"]);
    assert_eq!(main.alias("/theirs"), None);
}

// Negative tests.

#[test]
fn bad_aliases_are_rejected() {
    let main = FakeMain::install();
    unsafe extern "C" fn noop(_: *mut std::ffi::c_void, _: *mut PlayerClient, _: *const c_char) {}
    let other = MQPluginHandle { pluginID: 7 };
    // A plugin's command, and an EverQuest command (which aliases may shadow).
    assert!(unsafe { shim::mqemu_add_command(main.0, &other, "/taken".as_ptr().cast(), 6, noop, ptr::null_mut(), false, true, false) });
    assert!(unsafe { shim::mqemu_add_command(main.0, &other, "/eqcmd".as_ptr().cast(), 6, noop, ptr::null_mut(), true, true, false) });

    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| {
        let long = format!("/echo {}", "x".repeat(MAX_STRING));
        let cases = [
            ("rt", "/who"),
            ("/", "/who"),
            ("/a b", "/who"),
            ("/a=b", "/who"),
            ("/a\0b", "/who"),
            ("/empty", ""),
            ("/blank", "   "),
            ("/lines", "/who\n/quit"),
            ("/nul", "/who\0"),
            ("/long", long.as_str()),
            ("/ok", "/who"),
            ("/OK", "/who"),
            ("/taken", "/who"),
            ("/eqcmd", "/who"),
        ];
        for (name, replacement) in cases {
            let label = if replacement.len() > 20 { "<long>" } else { replacement };
            plugin.log.push(format!("{name:?}->{label:?}={:?}", mq.add_alias(name, replacement)));
        }
    });
    rt.initialize();
    assert_eq!(
        log_of(rt),
        [
            "new",
            "\"rt\"->\"/who\"=Err(InvalidName)",
            "\"/\"->\"/who\"=Err(InvalidName)",
            "\"/a b\"->\"/who\"=Err(InvalidName)",
            "\"/a=b\"->\"/who\"=Err(InvalidName)",
            "\"/a\\0b\"->\"/who\"=Err(InvalidName)",
            "\"/empty\"->\"\"=Err(InvalidReplacement)",
            "\"/blank\"->\"   \"=Err(InvalidReplacement)",
            "\"/lines\"->\"/who\\n/quit\"=Err(InvalidReplacement)",
            "\"/nul\"->\"/who\\0\"=Err(InvalidReplacement)",
            "\"/long\"->\"<long>\"=Err(InvalidReplacement)",
            "\"/ok\"->\"/who\"=Ok(())",
            "\"/OK\"->\"/who\"=Err(Rejected)",
            "\"/taken\"->\"/who\"=Err(Rejected)",
            "\"/eqcmd\"->\"/who\"=Ok(())",
        ]
    );
    // A rejected duplicate isn't tracked, so shutdown removes /ok only once
    // and leaves the failed names alone.
    rt.shutdown();
    assert_eq!((main.alias("/ok"), main.alias("/eqcmd")), (None, None));
}

#[test]
fn aliases_are_unavailable_without_macroquest() {
    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| {
        plugin.log.push(format!("{:?} {} {}", mq.add_alias("/rt", "/who"), mq.is_alias("/rt"), mq.remove_alias("/rt")));
    });
    rt.initialize();
    assert_eq!(log_of(rt), ["new", "Err(Unavailable) false false"]);
}
