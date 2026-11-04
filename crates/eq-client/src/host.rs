//! What eqclient.dll does, kept here so it's tested in-process: one ring of
//! string IDs, and the `getString` hook that fills it.

use std::ffi::{c_char, c_void};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::abi::{API_VERSION, Api, Status};
use crate::patch::{Detour, PatchError};
use crate::ring::Ring;
use crate::rof2::{self, HookSite};
use crate::win::*;

static RING: Ring = Ring::new();
/// The trampoline to the real `getString`, once hooked.
static GET_STRING: AtomicUsize = AtomicUsize::new(0);
/// How hooking `getString` went, once tried: it's tried only once.
static HOOKED: Mutex<Option<Status>> = Mutex::new(None);

type GetString = unsafe extern "thiscall" fn(this: *mut c_void, id: u32, found: *mut bool) -> *const c_char;

unsafe extern "thiscall" fn get_string(this: *mut c_void, id: u32, found: *mut bool) -> *const c_char {
    RING.record(id);
    let original: GetString = unsafe { std::mem::transmute(GET_STRING.load(Ordering::Acquire)) };
    unsafe { original(this, id, found) }
}

/// The table eqclient.dll exports.
pub static API: Api = Api { version: API_VERSION, watch_string_ids, string_ids_head, poll_string_ids };

unsafe extern "C" fn watch_string_ids(ids: *const u32, count: usize) -> Status {
    let status = hook_once(|| match client_base() {
        Some(base) => unsafe { hook(&rof2::GET_STRING, base) },
        None => Status::NOT_THE_CLIENT,
    });
    if !status.is_hooked() {
        return status;
    }
    let ids = if ids.is_null() { &[][..] } else { unsafe { std::slice::from_raw_parts(ids, count) } };
    // Every ID is watched, even after one too large.
    let refused = ids.iter().filter(|id| !RING.watch(**id)).count();
    if refused == 0 { Status::HOOKED } else { Status::ID_TOO_LARGE }
}

unsafe extern "C" fn string_ids_head() -> u64 {
    RING.head()
}

unsafe extern "C" fn poll_string_ids(cursor: *mut u64, out: *mut u32, capacity: usize, missed: *mut u64) -> usize {
    let Some(cursor) = (unsafe { cursor.as_mut() }) else { return 0 };
    if out.is_null() {
        return 0;
    }
    let polled = RING.poll_at_most(cursor, capacity);
    unsafe {
        std::ptr::copy_nonoverlapping(polled.ids.as_ptr(), out, polled.ids.len());
        if let Some(missed) = missed.as_mut() {
            *missed += polled.missed;
        }
    }
    polled.ids.len()
}

fn hook_once(hook: impl FnOnce() -> Status) -> Status {
    let mut hooked = HOOKED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *hooked.get_or_insert_with(hook)
}

/// eqgame.exe's base, if this process is it.
fn client_base() -> Option<usize> {
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    let mut name = [0u16; 260];
    let len = unsafe { GetModuleFileNameW(module, name.as_mut_ptr(), name.len() as u32) } as usize;
    let path = String::from_utf16_lossy(&name[..len]);
    let file = path.rsplit(['\\', '/']).next().unwrap_or_default();
    file.eq_ignore_ascii_case("eqgame.exe").then_some(module as usize)
}

/// Detours `site` in a client loaded at `base`, after checking the address is
/// inside the image.
///
/// # Safety
/// `base` must be a loaded module's base.
unsafe fn hook(site: &HookSite, base: usize) -> Status {
    let target = site.at(base);
    if !(base..base + unsafe { image_size(base) }).contains(&(target + site.prologue.len())) {
        return Status::UNKNOWN_CLIENT;
    }
    unsafe { hook_at(target as *mut u8, site.prologue) }
}

/// Detours the function at `target` with the `getString` hook.
///
/// # Safety
/// `target` must be a thiscall `(u32, *mut bool) -> *const c_char` function
/// starting with `prologue` (see [`Detour::install`]).
pub(crate) unsafe fn hook_at(target: *mut u8, prologue: &[u8]) -> Status {
    match unsafe { Detour::install(target, prologue, get_string as *const c_void) } {
        Ok(detour) => {
            GET_STRING.store(detour.trampoline() as usize, Ordering::Release);
            // Stays in for the life of the process, as the DLL does.
            std::mem::forget(detour);
            Status::HOOKED
        }
        Err(PatchError::Memory) => Status::MEMORY,
        Err(_) => Status::UNKNOWN_CLIENT,
    }
}

/// `SizeOfImage` from a loaded module's PE header.
unsafe fn image_size(base: usize) -> usize {
    let read = |at: usize| unsafe { std::ptr::read_unaligned((base + at) as *const u32) } as usize;
    read(read(0x3C) + 0x50)
}

/// Keeps the module holding this code loaded until the process ends, so the
/// hook and the ring it fills never go away under the client.
pub fn pin_self() -> bool {
    let mut module = std::ptr::null_mut();
    unsafe { GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_PIN | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, (&raw const API).cast(), &mut module) != 0 }
}

#[cfg(test)]
pub(crate) mod tests;
