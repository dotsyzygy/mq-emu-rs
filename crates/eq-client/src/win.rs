//! The few kernel32 functions eq-client needs.

use std::ffi::{c_char, c_void};

pub const MEM_COMMIT: u32 = 0x1000;
pub const MEM_RESERVE: u32 = 0x2000;
pub const MEM_RELEASE: u32 = 0x8000;
pub const PAGE_EXECUTE_READWRITE: u32 = 0x40;
pub const GET_MODULE_HANDLE_EX_FLAG_PIN: u32 = 0x1;
pub const GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS: u32 = 0x4;
pub const GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT: u32 = 0x2;

#[link(name = "kernel32")]
unsafe extern "system" {
    pub fn VirtualAlloc(address: *mut c_void, size: usize, allocation: u32, protect: u32) -> *mut c_void;
    pub fn VirtualFree(address: *mut c_void, size: usize, free_type: u32) -> i32;
    pub fn VirtualProtect(address: *mut c_void, size: usize, protect: u32, old: *mut u32) -> i32;
    pub fn FlushInstructionCache(process: *mut c_void, address: *const c_void, size: usize) -> i32;
    pub fn GetCurrentProcess() -> *mut c_void;
    pub fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    pub fn GetModuleHandleExW(flags: u32, name: *const u16, module: *mut *mut c_void) -> i32;
    pub fn GetModuleFileNameW(module: *mut c_void, out: *mut u16, size: u32) -> u32;
    pub fn LoadLibraryW(name: *const u16) -> *mut c_void;
    pub fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
}

/// `text` as a NUL-terminated UTF-16 string.
pub fn wide(text: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    text.encode_wide().chain([0]).collect()
}
