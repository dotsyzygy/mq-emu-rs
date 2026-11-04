//! Detours on the assembly fakes, which start as the client's `getString`.

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::fake_code::{self, Fake, Object, PROLOGUE};

fn bytes(code: *const u8, len: usize) -> Vec<u8> {
    unsafe { std::slice::from_raw_parts(code, len) }.to_vec()
}

static ADD_ORIGINAL: AtomicUsize = AtomicUsize::new(0);

/// Calls the original and adds a million, so a call through it shows both ran.
unsafe extern "thiscall" fn add_a_million(this: *const Object, id: u32, found: *mut bool) -> u32 {
    let original: Fake = unsafe { std::mem::transmute(ADD_ORIGINAL.load(Ordering::Acquire)) };
    (unsafe { original(this, id, found) }) + 1_000_000
}

#[test]
fn a_detour_runs_instead_and_its_trampoline_runs_the_original() {
    let _fakes = fake_code::lock();
    let target = fake_code::add();
    assert_eq!(fake_code::call(target, 1000, 7), 1007);

    let detour = unsafe { Detour::install(target, PROLOGUE, add_a_million as *const c_void) }.unwrap();
    ADD_ORIGINAL.store(detour.trampoline() as usize, Ordering::Release);
    assert_eq!(jump_target(target as usize, &bytes(target, JMP_LEN)), Some(add_a_million as *const () as usize), "the function starts with a jump to the detour");
    assert_eq!(bytes(target, PROLOGUE.len())[JMP_LEN..], [NOP], "padded to whole instructions");
    assert_eq!(fake_code::call(target, 1000, 7), 1_001_007, "the detour ran, and through it the original, arguments intact");
    assert_eq!(fake_code::call(detour.trampoline().cast(), 1000, 7), 1007, "the trampoline alone is the original");

    unsafe { detour.remove() }.unwrap();
    assert_eq!(bytes(target, PROLOGUE.len()), PROLOGUE, "removing puts the bytes back");
    assert_eq!(fake_code::call(target, 1000, 7), 1007);
}

#[test]
fn bytes_other_than_expected_are_refused_and_left_alone() {
    let _fakes = fake_code::lock();
    let target = fake_code::sub();
    let other_build = [0x55, 0x8B, 0xEC, 0x8B, 0x41, 0x0C]; // mov eax,[ecx+0Ch]
    let error = unsafe { Detour::install(target, &other_build, add_a_million as *const c_void) }.unwrap_err();
    assert_eq!(error, PatchError::UnexpectedBytes(PROLOGUE.to_vec()));
    assert_eq!(bytes(target, PROLOGUE.len()), PROLOGUE, "nothing was written");
    assert_eq!(fake_code::call(target, 1000, 7), 993);
}

#[test]
fn a_function_hooked_already_is_refused() {
    let _fakes = fake_code::lock();
    let target = fake_code::double();
    let first = unsafe { Detour::install(target, PROLOGUE, add_a_million as *const c_void) }.unwrap();
    let second = unsafe { Detour::install(target, PROLOGUE, add_a_million as *const c_void) }.unwrap_err();
    assert!(matches!(second, PatchError::UnexpectedBytes(found) if found[0] == JMP), "the jump isn't the expected prologue");
    unsafe { first.remove() }.unwrap();
}

#[test]
fn a_detour_hooked_over_is_not_removed() {
    let _fakes = fake_code::lock();
    let target = fake_code::xor();
    let detour = unsafe { Detour::install(target, PROLOGUE, add_a_million as *const c_void) }.unwrap();
    // Another hook rewrites the jump to go somewhere else.
    let mut theirs = [NOP; 6];
    write_jump_bytes(&mut theirs[..JMP_LEN], target as usize, fake_code::or() as usize);
    unsafe { write_code(target, &theirs) }.unwrap();

    let (detour, error) = unsafe { detour.remove() }.unwrap_err();
    assert_eq!(error, PatchError::Overwritten);
    assert_eq!(bytes(target, 6), theirs, "their hook is left in place");
    // Undo it for the test's sake, then ours comes off.
    let mut ours = [NOP; 6];
    write_jump_bytes(&mut ours[..JMP_LEN], target as usize, add_a_million as *const () as usize);
    unsafe { write_code(target, &ours) }.unwrap();
    unsafe { detour.remove() }.unwrap();
    assert_eq!(bytes(target, PROLOGUE.len()), PROLOGUE);
}

#[test]
fn prologues_too_short_or_long_are_refused() {
    let _fakes = fake_code::lock();
    let target = fake_code::or();
    for prologue in [&PROLOGUE[..4], &[0x90; MAX_PROLOGUE + 1][..]] {
        assert_eq!(unsafe { Detour::install(target, prologue, add_a_million as *const c_void) }.unwrap_err(), PatchError::BadPrologue);
    }
    assert_eq!(bytes(target, PROLOGUE.len()), PROLOGUE);
}
