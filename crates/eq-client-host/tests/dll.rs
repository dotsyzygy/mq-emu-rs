//! The built eqclient.dll, loaded as a plugin loads it.

use std::path::PathBuf;

use eq_client::abi::{API_VERSION, DLL_NAME, Status};
use eq_client::loader;

/// The DLL cargo built for this test, next to it in `deps`.
fn built_dll() -> PathBuf {
    std::env::current_exe().unwrap().parent().unwrap().join(DLL_NAME)
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut std::ffi::c_void;
    fn FreeLibrary(module: *mut std::ffi::c_void) -> i32;
}

fn module_handle() -> *mut std::ffi::c_void {
    let name: Vec<u16> = DLL_NAME.encode_utf16().chain([0]).collect();
    unsafe { GetModuleHandleW(name.as_ptr()) }
}

#[test]
fn the_dll_loads_speaks_its_version_and_stays_loaded() {
    let api = loader::load(&built_dll()).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(api.version, API_VERSION);
    assert_eq!(unsafe { (api.watch_string_ids)([173].as_ptr(), 1) }, Status::NOT_THE_CLIENT, "a test process isn't eqgame.exe, so nothing is hooked");
    assert_eq!(unsafe { (api.string_ids_head)() }, 0);

    // Plugins unload; the DLL mustn't go with them while the client may call its hook.
    let module = module_handle();
    assert!(!module.is_null());
    for _ in 0..4 {
        unsafe { FreeLibrary(module) };
    }
    assert_eq!(module_handle(), module, "pinned on first use");
    assert!(std::ptr::eq(loader::load(&built_dll()).unwrap(), api), "loading again gets the same one");
}
