//! The DLL's logic in-process: the hook records watched IDs and passes every
//! call through; outside eqgame.exe nothing is patched.

use super::*;
use crate::fake_code;

fn poll(cursor: &mut u64, capacity: usize) -> (Vec<u32>, u64) {
    let mut out = vec![0; capacity];
    let mut missed = 0;
    let n = unsafe { (API.poll_string_ids)(cursor, out.as_mut_ptr(), capacity, &mut missed) };
    out.truncate(n);
    (out, missed)
}

/// The one test that hooks: the hook's trampoline is process-wide.
#[test]
fn the_hook_records_watched_ids_and_returns_what_the_original_does() {
    // The patch tests patch the shared fakes; this one is ours alone.
    let target = host_fake();
    assert_eq!(unsafe { hook_at(target, fake_code::PROLOGUE) }, Status::HOOKED);
    RING.watch(173);
    RING.watch(425);
    let mut cursor = unsafe { (API.string_ids_head)() };

    assert_eq!(fake_code::call(target, 1000, 173), 1173, "the client gets the original's answer");
    assert_eq!(fake_code::call(target, 1000, 9), 1009);
    assert_eq!(fake_code::call(target, 1000, 425), 1425);
    assert_eq!(poll(&mut cursor, 1), (vec![173], 0), "at most the capacity");
    assert_eq!(poll(&mut cursor, 8), (vec![425], 0), "only watched IDs, in order");
    assert_eq!(poll(&mut cursor, 8), (vec![], 0));
}

#[test]
fn outside_the_client_nothing_is_hooked() {
    assert_eq!(unsafe { (API.watch_string_ids)([173].as_ptr(), 1) }, Status::NOT_THE_CLIENT);
    assert_eq!(unsafe { (API.watch_string_ids)(std::ptr::null(), 0) }, Status::NOT_THE_CLIENT, "and it's only tried once");
}

#[test]
fn a_hook_site_outside_the_image_is_refused_without_reading_it() {
    // This test binary is far smaller than eqgame.exe, so getString's address is past its end.
    let base = unsafe { GetModuleHandleW(std::ptr::null()) } as usize;
    assert!(unsafe { image_size(base) } < rof2::GET_STRING.address - rof2::PREFERRED_BASE);
    assert_eq!(unsafe { hook(&rof2::GET_STRING, base) }, Status::UNKNOWN_CLIENT);
}

#[test]
fn polling_with_nowhere_to_write_reads_nothing() {
    let mut cursor = 0;
    assert_eq!(unsafe { (API.poll_string_ids)(&mut cursor, std::ptr::null_mut(), 8, std::ptr::null_mut()) }, 0);
    assert_eq!(unsafe { (API.poll_string_ids)(std::ptr::null_mut(), [0u32; 1].as_mut_ptr(), 1, std::ptr::null_mut()) }, 0);
}

/// A fake only this module patches.
fn host_fake() -> *mut u8 {
    unsafe extern "C" {
        fn eqc_fake_host();
    }
    eqc_fake_host as *mut u8
}

std::arch::global_asm!(
    ".text",
    ".globl _eqc_fake_host",
    "_eqc_fake_host:",
    // getString's bytes exactly (the assembler's own `mov ebp, esp` is 89 E5).
    ".byte 0x55, 0x8B, 0xEC, 0x8B, 0x41, 0x08",
    "add eax, [ebp+8]",
    "pop ebp",
    "ret 8",
);
