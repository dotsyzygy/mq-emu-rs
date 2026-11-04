//! Functions to patch in tests, in assembly so their bytes are exact. Each
//! starts as the client's `getString` does and is thiscall like it, returning
//! a number from `this` and the ID. Each test patches its own, as tests run
//! at once.

use std::arch::global_asm;

// Each starts with getString's bytes exactly, as `.byte`s: the assembler
// encodes `mov ebp, esp` as 89 E5 where the client has 8B EC.
global_asm!(
    ".text",
    // this->value(+8) + id
    ".globl _eqc_fake_add",
    "_eqc_fake_add:",
    ".byte 0x55, 0x8B, 0xEC, 0x8B, 0x41, 0x08",
    "add eax, [ebp+8]",
    "pop ebp",
    "ret 8",
    // this->value(+8) - id
    ".globl _eqc_fake_sub",
    "_eqc_fake_sub:",
    ".byte 0x55, 0x8B, 0xEC, 0x8B, 0x41, 0x08",
    "sub eax, [ebp+8]",
    "pop ebp",
    "ret 8",
    // this->value(+8) * 2
    ".globl _eqc_fake_double",
    "_eqc_fake_double:",
    ".byte 0x55, 0x8B, 0xEC, 0x8B, 0x41, 0x08",
    "add eax, eax",
    "pop ebp",
    "ret 8",
    // this->value(+8) ^ id
    ".globl _eqc_fake_xor",
    "_eqc_fake_xor:",
    ".byte 0x55, 0x8B, 0xEC, 0x8B, 0x41, 0x08",
    "xor eax, [ebp+8]",
    "pop ebp",
    "ret 8",
    // this->value(+8) | id
    ".globl _eqc_fake_or",
    "_eqc_fake_or:",
    ".byte 0x55, 0x8B, 0xEC, 0x8B, 0x41, 0x08",
    "or eax, [ebp+8]",
    "pop ebp",
    "ret 8",
);

unsafe extern "C" {
    fn eqc_fake_add();
    fn eqc_fake_sub();
    fn eqc_fake_double();
    fn eqc_fake_xor();
    fn eqc_fake_or();
}

/// Held by every test that patches, reads or calls the shared fakes: a fake
/// mustn't run while another test rewrites it.
pub static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `getString`'s first instructions, which every fake starts with.
pub const PROLOGUE: &[u8] = crate::rof2::GET_STRING.prologue;

/// What the fakes read: `value` where `getString`'s table has its pointer.
#[repr(C)]
pub struct Object {
    _vtable: usize,
    _other: usize,
    pub value: u32,
}

impl Object {
    pub fn new(value: u32) -> Object {
        Object { _vtable: 0, _other: 0, value }
    }
}

pub type Fake = unsafe extern "thiscall" fn(this: *const Object, id: u32, found: *mut bool) -> u32;

pub fn add() -> *mut u8 {
    eqc_fake_add as *mut u8
}
pub fn sub() -> *mut u8 {
    eqc_fake_sub as *mut u8
}
pub fn double() -> *mut u8 {
    eqc_fake_double as *mut u8
}
pub fn xor() -> *mut u8 {
    eqc_fake_xor as *mut u8
}
pub fn or() -> *mut u8 {
    eqc_fake_or as *mut u8
}

/// Calls a fake (or what it was patched into) as the client calls `getString`.
pub fn call(code: *const u8, value: u32, id: u32) -> u32 {
    let object = Object::new(value);
    let function: Fake = unsafe { std::mem::transmute(code) };
    unsafe { function(&object, id, std::ptr::null_mut()) }
}

#[test]
fn the_fakes_start_as_get_string_does() {
    let _fakes = lock();
    for code in [add(), sub(), double(), xor(), or()] {
        assert_eq!(unsafe { std::slice::from_raw_parts(code, PROLOGUE.len()) }, PROLOGUE);
    }
    assert_eq!(call(add(), 1000, 7), 1007);
    assert_eq!(call(sub(), 1000, 7), 993);
}
