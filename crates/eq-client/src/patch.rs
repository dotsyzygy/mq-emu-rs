//! Inline detours on 32-bit x86: the first bytes of a function become a jump
//! to the detour, and a trampoline runs those bytes and jumps back.
//!
//! Nothing is guessed. A detour is installed only over the exact bytes the
//! caller expects, which must be whole instructions with nothing relative to
//! where they are (no jumps or calls), so copying them to the trampoline is
//! safe. Any other bytes, a different client build or another hook, are
//! refused.

use std::ffi::c_void;
use std::fmt;
use std::ptr;

use crate::win::*;

/// `jmp rel32`.
const JMP: u8 = 0xE9;
const JMP_LEN: usize = 5;
/// `nop`, padding the patch to whole instructions.
const NOP: u8 = 0x90;
/// The longest prologue a detour may replace.
pub const MAX_PROLOGUE: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchError {
    /// The prologue is shorter than a jump, or longer than [`MAX_PROLOGUE`].
    BadPrologue,
    /// The function doesn't start with the expected bytes; these are what it has.
    UnexpectedBytes(Vec<u8>),
    /// No memory for the trampoline, or the code couldn't be made writable.
    Memory,
    /// Removing: the function no longer starts with this detour's jump
    /// (another hook was put over it), so it's left alone.
    Overwritten,
}

impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatchError::BadPrologue => write!(f, "a detour's prologue must be {JMP_LEN} to {MAX_PROLOGUE} bytes"),
            PatchError::UnexpectedBytes(found) => write!(f, "the function doesn't start as expected (it has {found:02X?}): a different client build, or hooked already"),
            PatchError::Memory => f.write_str("couldn't allocate or unprotect code memory"),
            PatchError::Overwritten => f.write_str("another hook was put over this detour, so it can't be removed"),
        }
    }
}

impl std::error::Error for PatchError {}

/// An installed detour. It stays installed until [`Detour::remove`]; dropping
/// it leaves it in place, since code may be running in the trampoline.
#[derive(Debug)]
pub struct Detour {
    target: *mut u8,
    detour: usize,
    original: Vec<u8>,
    trampoline: *mut u8,
}

impl Detour {
    /// Points `target` at `detour`. `expected` is the prologue `target` must
    /// start with (see the module docs).
    ///
    /// # Safety
    /// `target` must be a function's code, with `expected` its first whole,
    /// position-independent instructions, and `detour` a function with the
    /// same signature and calling convention. No other thread may be running
    /// `target`'s first bytes while it's patched.
    pub unsafe fn install(target: *mut u8, expected: &[u8], detour: *const c_void) -> Result<Detour, PatchError> {
        let len = expected.len();
        if !(JMP_LEN..=MAX_PROLOGUE).contains(&len) {
            return Err(PatchError::BadPrologue);
        }
        let found = unsafe { std::slice::from_raw_parts(target, len) };
        if found != expected {
            return Err(PatchError::UnexpectedBytes(found.to_vec()));
        }

        let trampoline = unsafe { VirtualAlloc(ptr::null_mut(), len + JMP_LEN, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE) }.cast::<u8>();
        if trampoline.is_null() {
            return Err(PatchError::Memory);
        }
        unsafe {
            ptr::copy_nonoverlapping(target, trampoline, len);
            write_jump(trampoline.add(len), target.add(len));
        }

        let mut patch = [NOP; MAX_PROLOGUE];
        write_jump_bytes(&mut patch[..JMP_LEN], target as usize, detour as usize);
        if let Err(error) = unsafe { write_code(target, &patch[..len]) } {
            unsafe { VirtualFree(trampoline.cast(), 0, MEM_RELEASE) };
            return Err(error);
        }
        Ok(Detour { target, detour: detour as usize, original: expected.to_vec(), trampoline })
    }

    /// Calls the original function: its replaced prologue, then the rest.
    /// Transmute to the function's type.
    pub fn trampoline(&self) -> *const c_void {
        self.trampoline.cast()
    }

    /// Puts the original bytes back. The trampoline is kept, as a call may
    /// still be returning through it.
    ///
    /// # Safety
    /// As for [`Detour::install`]: no thread may be running the patched bytes.
    pub unsafe fn remove(self) -> Result<(), (Detour, PatchError)> {
        let len = self.original.len();
        let current = unsafe { std::slice::from_raw_parts(self.target, len) };
        let ours = jump_target(self.target as usize, current) == Some(self.detour) && current[JMP_LEN..].iter().all(|byte| *byte == NOP);
        if !ours {
            return Err((self, PatchError::Overwritten));
        }
        match unsafe { write_code(self.target, &self.original) } {
            Ok(()) => Ok(()),
            Err(error) => Err((self, error)),
        }
    }
}

/// Where a `jmp rel32` at `from` lands.
pub fn jump_target(from: usize, bytes: &[u8]) -> Option<usize> {
    (bytes.len() >= JMP_LEN && bytes[0] == JMP).then(|| {
        let rel = i32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]);
        from.wrapping_add(JMP_LEN).wrapping_add(rel as usize)
    })
}

fn write_jump_bytes(out: &mut [u8], from: usize, to: usize) {
    let rel = to.wrapping_sub(from.wrapping_add(JMP_LEN)) as i32;
    out[0] = JMP;
    out[1..JMP_LEN].copy_from_slice(&rel.to_le_bytes());
}

unsafe fn write_jump(at: *mut u8, to: *const u8) {
    let mut bytes = [0; JMP_LEN];
    write_jump_bytes(&mut bytes, at as usize, to as usize);
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), at, JMP_LEN) };
}

/// Held while code is writable: two writes to one page at once would have
/// the first to finish protect it again under the second.
static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Writes `bytes` over code, unprotecting it for the write.
unsafe fn write_code(at: *mut u8, bytes: &[u8]) -> Result<(), PatchError> {
    let _writing = WRITING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut old = 0;
    if unsafe { VirtualProtect(at.cast(), bytes.len(), PAGE_EXECUTE_READWRITE, &mut old) } == 0 {
        return Err(PatchError::Memory);
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), at, bytes.len());
        VirtualProtect(at.cast(), bytes.len(), old, &mut old);
        FlushInstructionCache(GetCurrentProcess(), at.cast(), bytes.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
