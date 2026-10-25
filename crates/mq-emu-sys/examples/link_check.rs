//! Link-time check: a DLL that references every import, so the linker has to
//! resolve each one against MQ2Main.lib / eqlib.lib. `cargo test` builds
//! examples, so a bad declaration fails the test run even though nothing here
//! is ever executed.

use core::ffi::c_void;

use mq_emu_sys::*;

#[unsafe(no_mangle)]
pub extern "C" fn mq_emu_sys_link_check() -> usize {
    let functions: &[*const c_void] = &[
        WriteChatf as *const c_void,
        WriteChatColor as *const c_void,
        WriteChatColorf as *const c_void,
        DebugSpew as *const c_void,
        DebugSpewAlways as *const c_void,
        ParseMacroData as *const c_void,
        GetGameState as *const c_void,
        GetPluginProc as *const c_void,
    ];
    let statics: &[*const c_void] = &[
        (&raw const gGameState).cast(),
        (&raw const gbInZone).cast(),
        (&raw const gszMacroName).cast(),
        (&raw const gPathMQRoot).cast(),
        (&raw const gPathConfig).cast(),
        (&raw const gPathMacros).cast(),
        (&raw const gPathLogs).cast(),
        (&raw const gPathPlugins).cast(),
        (&raw const gPathResources).cast(),
        (&raw const pLocalPlayer).cast(),
        (&raw const pControlledPlayer).cast(),
        (&raw const pTarget).cast(),
    ];
    assert_eq!(
        functions.len() + statics.len(),
        MQ2MAIN_IMPORTS.len() + EQLIB_IMPORTS.len(),
        "link_check is missing an import"
    );
    // The shim's only MQ2Main import is the C++-mangled mq::GetMainInterface.
    let shim: &[*const c_void] = &[
        mq_emu_sys::shim::mqemu_main_interface as *const c_void,
        mq_emu_sys::shim::mqemu_add_command as *const c_void,
        mq_emu_sys::shim::mqemu_do_command as *const c_void,
    ];
    functions.iter().chain(statics).chain(shim).map(|p| *p as usize).fold(0, usize::wrapping_add)
}
