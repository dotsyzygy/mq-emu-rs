//! Subclassing the game's windows. As MacroQuest's `WindowOverride` does, a
//! subclassed window gets a copy of its vtable with some entries replaced.
//! Each replaced entry points at a stub of our own, in memory of its own,
//! that loads its subclass into EDX (which `__thiscall` leaves unused) and
//! jumps to a `fastcall` function here, whose second argument EDX is: so the
//! function knows its subclass whatever window it's called for, even through
//! a copy of our table another hook made.
//!
//! Hooks can stack: a window's vtable may be another's copy when we copy it,
//! and another hook (a C++ plugin's `WindowOverride`, another Rust plugin)
//! may copy ours. We put the window's old vtable back only if ours is still
//! the one it has; otherwise the stubs are rewritten to jump straight to the
//! functions they replaced and left in place (with the table), so whatever
//! copied them keeps working after this plugin unloads.
//!
//! Nothing here imports from `MQ2Main.dll` or `eqlib.dll` (only kernel32),
//! so tests can call it with a fake window and vtable.

use core::ffi::c_void;
use core::ptr::{self, null_mut};
use core::sync::atomic::{AtomicPtr, Ordering};

use super::{HOOK_DRAW, HOOK_LBUTTON_DOWN, HOOK_LBUTTON_UP, HOOK_MOUSE_MOVE, HOOK_NOTIFICATION, HOOK_RBUTTON_DOWN, HOOK_RBUTTON_UP, HOOK_WHEEL, mqemu_window_event, mqemu_window_hook_fn};
use crate::layouts::{CXPoint, CXWnd};

const PTR: usize = size_of::<usize>();

// The vtable entries a subclass replaces, counted in pointers. All are below
// 0x164, where the PDB's offsets are right for every window class.
const NOTIFICATION_SLOT: usize = CXWnd::VTABLE_WndNotification / PTR;
const POST_DRAW_SLOT: usize = CXWnd::VTABLE_PostDraw / PTR;
const LBUTTON_DOWN_SLOT: usize = CXWnd::VTABLE_HandleLButtonDown / PTR;
const LBUTTON_UP_SLOT: usize = CXWnd::VTABLE_HandleLButtonUp / PTR;
const RBUTTON_DOWN_SLOT: usize = CXWnd::VTABLE_HandleRButtonDown / PTR;
const RBUTTON_UP_SLOT: usize = CXWnd::VTABLE_HandleRButtonUp / PTR;
const MOUSE_MOVE_SLOT: usize = CXWnd::VTABLE_HandleMouseMove / PTR;
const WHEEL_SLOT: usize = CXWnd::VTABLE_HandleWheelMove / PTR;
/// CXWnd's own entries (`UpdateLayout` is its last): a window's vtable must
/// have at least these to be subclassed.
const CXWND_ENTRIES: usize = CXWnd::VTABLE_UpdateLayout / PTR + 1;

/// Entries copied from the window's vtable: as many as MacroQuest's
/// `WindowOverride` copies, more than any window class has.
const TABLE_ENTRIES: usize = 1024;
/// `mov edx, imm32` (5 bytes); `jmp rel32` (5 bytes).
const STUB_SIZE: usize = 10;

type NotificationFn = unsafe extern "thiscall" fn(*mut CXWnd, *mut CXWnd, u32, *mut c_void) -> i32;
type DrawFn = unsafe extern "thiscall" fn(*mut CXWnd) -> i32;
type MouseFn = unsafe extern "thiscall" fn(*mut CXWnd, *const CXPoint, u32) -> i32;
type WheelFn = unsafe extern "thiscall" fn(*mut CXWnd, *const CXPoint, i32, u32) -> i32;

/// Who gets a window's events: the plugin's function, its data, and a bit
/// per kind it wants.
#[derive(Clone, Copy)]
struct HookTarget {
    hook: Option<mqemu_window_hook_fn>,
    user: *mut c_void,
    mask: u32,
}

impl HookTarget {
    const NONE: HookTarget = HookTarget { hook: None, user: null_mut(), mask: 0 };

    /// True if the plugin handled it, so the window shouldn't.
    unsafe fn send(self, window: *mut CXWnd, event: &mqemu_window_event) -> bool {
        match self.hook {
            Some(hook) if wants(self.mask, event.kind) => unsafe { hook(self.user, window, event) },
            _ => false,
        }
    }
}

fn wants(mask: u32, kind: i32) -> bool {
    (0..32).contains(&kind) && mask & (1 << kind) != 0
}

fn event(kind: i32) -> mqemu_window_event {
    mqemu_window_event { kind, sender: null_mut(), message: 0, data: null_mut(), x: 0, y: 0, flags: 0, scroll: 0 }
}

fn mouse_event(kind: i32, pos: &CXPoint, flags: u32, scroll: i32) -> mqemu_window_event {
    mqemu_window_event { x: pos.x(), y: pos.y(), flags, scroll, ..event(kind) }
}

/// One subclassed window, in memory of its own (read-write), with its copy
/// of the vtable.
#[repr(C)]
struct Subclass {
    /// The next window this plugin subclassed.
    next: *mut Subclass,
    window: *mut CXWnd,
    target: HookTarget,
    /// The window's vtable before.
    original: *const usize,
    /// One stub per replaced entry, executable.
    stubs: *mut u8,
    /// Our copy: the RTTI pointer, then the entries.
    block: [usize; TABLE_ENTRIES + 1],
}

impl Subclass {
    fn table(&mut self) -> *mut usize {
        self.block[1..].as_mut_ptr()
    }

    unsafe fn original<F: Copy>(&self, slot: usize) -> F {
        const { assert!(size_of::<F>() == PTR) };
        unsafe { self.original.add(slot).cast::<F>().read() }
    }
}

unsafe extern "fastcall" fn subclass_notification(this: *mut CXWnd, sub: *mut Subclass, sender: *mut CXWnd, message: u32, data: *mut c_void) -> i32 {
    unsafe {
        // Taken first: the plugin may remove the subclass from its handler.
        let original = (*sub).original::<NotificationFn>(NOTIFICATION_SLOT);
        let event = mqemu_window_event { sender, message, data, ..event(HOOK_NOTIFICATION) };
        if this == (*sub).window && (*sub).target.send(this, &event) {
            return 0;
        }
        original(this, sender, message, data)
    }
}

unsafe extern "fastcall" fn subclass_post_draw(this: *mut CXWnd, sub: *mut Subclass) -> i32 {
    unsafe {
        let target = (*sub).target;
        let mine = this == (*sub).window;
        let result = (*sub).original::<DrawFn>(POST_DRAW_SLOT)(this);
        if mine {
            target.send(this, &event(HOOK_DRAW));
        }
        result
    }
}

unsafe extern "fastcall" fn subclass_mouse<const KIND: i32, const SLOT: usize>(this: *mut CXWnd, sub: *mut Subclass, pos: *const CXPoint, flags: u32) -> i32 {
    unsafe {
        let original = (*sub).original::<MouseFn>(SLOT);
        if this == (*sub).window && (*sub).target.send(this, &mouse_event(KIND, &*pos, flags, 0)) {
            return 0;
        }
        original(this, pos, flags)
    }
}

unsafe extern "fastcall" fn subclass_wheel(this: *mut CXWnd, sub: *mut Subclass, pos: *const CXPoint, scroll: i32, flags: u32) -> i32 {
    unsafe {
        let original = (*sub).original::<WheelFn>(WHEEL_SLOT);
        if this == (*sub).window && (*sub).target.send(this, &mouse_event(HOOK_WHEEL, &*pos, flags, scroll)) {
            return 0;
        }
        original(this, pos, scroll, flags)
    }
}

/// A replaced entry: the hook kind, its slot, and our function for it.
struct Replaced {
    kind: i32,
    slot: usize,
    function: *const (),
}

const REPLACED_COUNT: usize = 8;

fn replaced() -> [Replaced; REPLACED_COUNT] {
    [
        Replaced { kind: HOOK_NOTIFICATION, slot: NOTIFICATION_SLOT, function: subclass_notification as *const () },
        Replaced { kind: HOOK_DRAW, slot: POST_DRAW_SLOT, function: subclass_post_draw as *const () },
        Replaced { kind: HOOK_LBUTTON_DOWN, slot: LBUTTON_DOWN_SLOT, function: subclass_mouse::<HOOK_LBUTTON_DOWN, LBUTTON_DOWN_SLOT> as *const () },
        Replaced { kind: HOOK_LBUTTON_UP, slot: LBUTTON_UP_SLOT, function: subclass_mouse::<HOOK_LBUTTON_UP, LBUTTON_UP_SLOT> as *const () },
        Replaced { kind: HOOK_RBUTTON_DOWN, slot: RBUTTON_DOWN_SLOT, function: subclass_mouse::<HOOK_RBUTTON_DOWN, RBUTTON_DOWN_SLOT> as *const () },
        Replaced { kind: HOOK_RBUTTON_UP, slot: RBUTTON_UP_SLOT, function: subclass_mouse::<HOOK_RBUTTON_UP, RBUTTON_UP_SLOT> as *const () },
        Replaced { kind: HOOK_MOUSE_MOVE, slot: MOUSE_MOVE_SLOT, function: subclass_mouse::<HOOK_MOUSE_MOVE, MOUSE_MOVE_SLOT> as *const () },
        Replaced { kind: HOOK_WHEEL, slot: WHEEL_SLOT, function: subclass_wheel as *const () },
    ]
}

/// The windows this plugin subclassed. The game's UI is single-threaded; the
/// atomic is only for a `static` without `unsafe`.
static SUBCLASSES: AtomicPtr<Subclass> = AtomicPtr::new(null_mut());

fn find_subclass(window: *mut CXWnd) -> *mut Subclass {
    let mut sub = SUBCLASSES.load(Ordering::Relaxed);
    while !sub.is_null() && unsafe { (*sub).window } != window {
        sub = unsafe { (*sub).next };
    }
    sub
}

unsafe fn unlink(sub: *mut Subclass) {
    unsafe {
        let mut link: *mut *mut Subclass = SUBCLASSES.as_ptr();
        while !(*link).is_null() {
            if *link == sub {
                *link = (*sub).next;
                return;
            }
            link = &raw mut (**link).next;
        }
    }
}

/// Writes a jump to `to` at `at`, preceded by `mov edx, edx_value` when
/// given (else the jump comes first).
unsafe fn write_stub(at: *mut u8, edx_value: Option<usize>, to: usize) {
    let (bytes, len) = stub_bytes(at as usize, edx_value, to);
    unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), at, len) };
}

/// The stub's bytes when written at address `at`, and how many there are.
fn stub_bytes(at: usize, edx_value: Option<usize>, to: usize) -> ([u8; STUB_SIZE], usize) {
    let mut bytes = [0u8; STUB_SIZE];
    let mut len = 0;
    if let Some(value) = edx_value {
        bytes[0] = 0xBA;
        bytes[1..5].copy_from_slice(&(value as u32).to_le_bytes());
        len = 5;
    }
    bytes[len] = 0xE9;
    let rel = (to as u32).wrapping_sub((at + len + 5) as u32);
    bytes[len + 1..len + 5].copy_from_slice(&rel.to_le_bytes());
    (bytes, len + 5)
}

#[repr(C)]
struct MemoryBasicInformation {
    base_address: *mut c_void,
    allocation_base: *mut c_void,
    allocation_protect: u32,
    region_size: usize,
    state: u32,
    protect: u32,
    kind: u32,
}

const MEM_COMMIT: u32 = 0x1000;
const MEM_RESERVE: u32 = 0x2000;
const MEM_RELEASE: u32 = 0x8000;
const PAGE_NOACCESS: u32 = 0x01;
const PAGE_READWRITE: u32 = 0x04;
const PAGE_EXECUTE_READ: u32 = 0x20;
const PAGE_GUARD: u32 = 0x100;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn VirtualAlloc(address: *mut c_void, size: usize, allocation_type: u32, protect: u32) -> *mut c_void;
    fn VirtualFree(address: *mut c_void, size: usize, free_type: u32) -> i32;
    fn VirtualProtect(address: *mut c_void, size: usize, protect: u32, old: *mut u32) -> i32;
    fn VirtualQuery(address: *const c_void, info: *mut MemoryBasicInformation, size: usize) -> usize;
    fn FlushInstructionCache(process: *mut c_void, address: *const c_void, size: usize) -> i32;
    fn GetCurrentProcess() -> *mut c_void;
}

unsafe fn set_stubs_writable(sub: *mut Subclass, writable: bool) -> bool {
    let size = STUB_SIZE * REPLACED_COUNT;
    let stubs = unsafe { (*sub).stubs }.cast::<c_void>();
    let mut old = 0;
    if unsafe { VirtualProtect(stubs, size, if writable { PAGE_READWRITE } else { PAGE_EXECUTE_READ }, &mut old) } == 0 {
        return false;
    }
    if !writable {
        unsafe { FlushInstructionCache(GetCurrentProcess(), stubs, size) };
    }
    true
}

/// How many bytes from `at` can be read, up to `want`.
fn readable_bytes(at: usize, want: usize) -> usize {
    let mut total = 0;
    while total < want {
        let mut info = MemoryBasicInformation { base_address: null_mut(), allocation_base: null_mut(), allocation_protect: 0, region_size: 0, state: 0, protect: 0, kind: 0 };
        if unsafe { VirtualQuery(at.wrapping_add(total) as *const c_void, &mut info, size_of::<MemoryBasicInformation>()) } == 0
            || info.state != MEM_COMMIT
            || info.protect & (PAGE_NOACCESS | PAGE_GUARD) != 0
        {
            break;
        }
        total = (info.base_address as usize + info.region_size).wrapping_sub(at);
    }
    total.min(want)
}

unsafe fn free_subclass(sub: *mut Subclass) {
    unsafe {
        if !(*sub).stubs.is_null() {
            VirtualFree((*sub).stubs.cast(), 0, MEM_RELEASE);
        }
        VirtualFree(sub.cast(), 0, MEM_RELEASE);
    }
}

/// The vtable entry (counted in pointers from the first) a subclass
/// ([`mqemu_window_subclass`]) replaces for hook kind `kind`; -1 for none.
/// For tests.
pub fn mqemu_window_hook_slot(kind: i32) -> i32 {
    replaced().iter().find(|entry| entry.kind == kind).map_or(-1, |entry| entry.slot as i32)
}

/// See [`super::Api::mqemu_window_subclass`].
///
/// # Safety
/// On the game thread; `window` is null or a live window with a vtable.
pub unsafe extern "C" fn mqemu_window_subclass(window: *mut CXWnd, hook: mqemu_window_hook_fn, user: *mut c_void, mask: u32) -> bool {
    if window.is_null() || !find_subclass(window).is_null() {
        return false;
    }
    let original = unsafe { *window.cast::<*const usize>() };
    if original.is_null() {
        return false;
    }
    // The whole table, and the RTTI pointer before it, as far as they can be
    // read.
    let entries = readable_bytes((original as usize).wrapping_sub(PTR), (TABLE_ENTRIES + 1) * PTR) / PTR;
    if entries < 1 + CXWND_ENTRIES {
        return false;
    }
    unsafe {
        let sub = VirtualAlloc(null_mut(), size_of::<Subclass>(), MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE).cast::<Subclass>();
        if sub.is_null() {
            return false;
        }
        // VirtualAlloc's memory is zeroed: the rest of the block, and stubs.
        (*sub).window = window;
        (*sub).target = HookTarget { hook: Some(hook), user, mask };
        (*sub).original = original;
        ptr::copy_nonoverlapping(original.sub(1), (*sub).block.as_mut_ptr(), entries);
        (*sub).stubs = VirtualAlloc(null_mut(), STUB_SIZE * REPLACED_COUNT, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE).cast();
        if (*sub).stubs.is_null() {
            free_subclass(sub);
            return false;
        }
        let table = (*sub).table();
        for (i, entry) in replaced().iter().enumerate() {
            let stub = (*sub).stubs.add(i * STUB_SIZE);
            write_stub(stub, Some(sub as usize), entry.function as usize);
            *table.add(entry.slot) = stub as usize;
        }
        if !set_stubs_writable(sub, false) {
            free_subclass(sub);
            return false;
        }
        (*sub).next = SUBCLASSES.load(Ordering::Relaxed);
        SUBCLASSES.store(sub, Ordering::Relaxed);
        *window.cast::<*mut usize>() = table;
    }
    true
}

/// See [`super::Api::mqemu_window_unsubclass`].
///
/// # Safety
/// On the game thread; `window` is live, or `live` is false.
pub unsafe extern "C" fn mqemu_window_unsubclass(window: *mut CXWnd, live: bool) -> bool {
    let sub = find_subclass(window);
    if sub.is_null() {
        return false;
    }
    unsafe {
        unlink(sub);
        (*sub).target = HookTarget::NONE;

        let vtable = window.cast::<*mut usize>();
        let table = (*sub).table();
        if live && *vtable == table {
            *vtable = (*sub).original.cast_mut();
            free_subclass(sub);
            return true;
        }

        // Something copied our table, or put the window's old one back, or
        // the window is gone: make the stubs and our entries the old
        // functions, and keep them.
        let original = (*sub).original;
        for entry in replaced() {
            *table.add(entry.slot) = *original.add(entry.slot);
        }
        if set_stubs_writable(sub, true) {
            for (i, entry) in replaced().iter().enumerate() {
                write_stub((*sub).stubs.add(i * STUB_SIZE), None, *original.add(entry.slot));
            }
            set_stubs_writable(sub, false);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stub_loads_edx_then_jumps() {
        let (bytes, len) = stub_bytes(0x1000, Some(0x1234_5678), 0x2000);
        assert_eq!(len, 10);
        // jmp rel32 is relative to the end of the stub: 0x2000 - 0x100a.
        assert_eq!(bytes, [0xBA, 0x78, 0x56, 0x34, 0x12, 0xE9, 0xF6, 0x0F, 0x00, 0x00]);
    }

    #[test]
    fn a_stub_without_edx_only_jumps_and_can_jump_back() {
        let (bytes, len) = stub_bytes(0x2000, None, 0x1000);
        assert_eq!(len, 5);
        // 0x1000 - 0x2005, two's complement.
        assert_eq!(bytes[..5], [0xE9, 0xFB, 0xEF, 0xFF, 0xFF]);
    }

    #[test]
    fn kinds_past_the_mask_are_never_wanted() {
        assert!(wants(1 << HOOK_WHEEL, HOOK_WHEEL));
        assert!(!wants(1 << HOOK_WHEEL, HOOK_DRAW));
        assert!(!wants(u32::MAX, -1));
        assert!(!wants(u32::MAX, 32));
    }

    #[test]
    fn the_memory_info_is_windows_32_bit_layout() {
        assert_eq!(size_of::<MemoryBasicInformation>(), 28);
    }
}
