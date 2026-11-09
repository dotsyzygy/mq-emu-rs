//! Opaque handles and pointer wrappers shared with MacroQuest.

use core::ffi::c_char;
use core::marker::{PhantomData, PhantomPinned};

macro_rules! opaque {
    ($( $(#[$attr:meta])* $name:ident ),* $(,)?) => {
        $(
            $(#[$attr])*
            #[repr(C)]
            pub struct $name {
                _data: [u8; 0],
                _marker: PhantomData<(*mut u8, PhantomPinned)>,
            }
        )*
    };
}

// eqlib game objects have generated layouts; MacroQuest's own types (below)
// aren't in eqlib.pdb and stay opaque.
pub use crate::layouts::{EQGroundItem, PlayerClient};

opaque! {
    /// `mq::MQTypeVar`, the value type used by TLOs and macro data.
    MQTypeVar,
    /// `mq::MQPlugin`, MacroQuest's bookkeeping record for a loaded plugin.
    MQPlugin,
    /// `mq::PluginInterface`, an optional interface a plugin can expose to others.
    PluginInterface,
    /// eqlib's `PlayerBase`, the first base of [`PlayerClient`], so a
    /// `PlayerClient` pointer is also one to its `PlayerBase`.
    PlayerBase,
}

/// Alias matching MacroQuest's `SPAWNINFO`.
pub type SPAWNINFO = PlayerClient;
/// Alias matching MacroQuest's `GROUNDITEM`.
pub type GROUNDITEM = EQGroundItem;

/// `eqlib::ForeignPointer<T>`: a pointer to a slot inside the game's memory
/// that holds a `T*`. Reading it twice can give different answers, since the
/// game rewrites the slot.
#[repr(C)]
pub struct ForeignPointer<T> {
    pub m_ptr: *mut *mut T,
}

impl<T> ForeignPointer<T> {
    /// Returns the pointer currently stored in the game's slot, or null.
    ///
    /// # Safety
    /// `m_ptr` must be null or point to readable memory, which holds for the
    /// globals eqlib exports once offsets are initialized.
    pub unsafe fn get(&self) -> *mut T {
        if self.m_ptr.is_null() {
            core::ptr::null_mut()
        } else {
            unsafe { *self.m_ptr }
        }
    }
}

/// `eqlib::ComputedPointer<T>`: a global that computes its pointer by calling
/// an eqlib function (an instance getter), rather than reading a slot.
#[repr(C)]
pub struct ComputedPointer<T> {
    pub m_fn: Option<unsafe extern "C" fn() -> *mut T>,
}

impl<T> ComputedPointer<T> {
    /// Calls the getter, or returns null if there is none.
    ///
    /// # Safety
    /// `m_fn` must be null or eqlib's getter, which holds for the globals
    /// eqlib exports once offsets are initialized. Game thread only.
    pub unsafe fn get(&self) -> *mut T {
        match self.m_fn {
            Some(getter) => unsafe { getter() },
            None => core::ptr::null_mut(),
        }
    }
}

/// What an exported `ForeignPointer` global points to, or null, read through
/// a raw pointer to it (`&raw const eqlib::pLocalPC`).
///
/// # Safety
/// As [`ForeignPointer::get`].
pub unsafe fn foreign<T>(global: *const ForeignPointer<T>) -> *mut T {
    unsafe { (*global).get() }
}

/// The object a [`ComputedPointer`] global computes, given a raw pointer to
/// the global (`&raw const eqlib::pChatService`).
///
/// # Safety
/// As [`ComputedPointer::get`].
pub unsafe fn computed<T>(global: *const ComputedPointer<T>) -> *mut T {
    unsafe { (*global).get() }
}

/// The function in `object`'s vtable at byte `offset` (a generated
/// `VTABLE_*` constant of the class that introduced it), as `F`: an
/// `unsafe extern "thiscall" fn` taking the object first.
///
/// # Safety
/// `object` must point to a live object whose vtable pointer is at its start
/// and has a function at `offset`, and `F` must be that function's type.
pub unsafe fn virtual_fn<F: Copy>(object: *const core::ffi::c_void, offset: usize) -> F {
    const { assert!(size_of::<F>() == size_of::<usize>()) };
    unsafe {
        let vtable = *object.cast::<*const u8>();
        vtable.add(offset).cast::<F>().read()
    }
}

/// A `CXStr` this side owns: constructed by eqlib, released on drop.
#[repr(C)]
pub struct OwnedCXStr {
    /// The CXStr itself: one pointer to its shared text (`CStrRep`).
    rep: *mut core::ffi::c_void,
}

impl OwnedCXStr {
    /// Copies `text` into a new CXStr.
    ///
    /// # Safety
    /// `text` must be NUL-terminated; eqlib must be loaded (in game).
    pub unsafe fn new(text: *const c_char) -> OwnedCXStr {
        let mut out = OwnedCXStr { rep: core::ptr::null_mut() };
        unsafe { crate::eqlib::CXStr_from_c_str(out.as_mut_ptr(), text) };
        out
    }

    /// The CXStr a C++ function returning one by value constructs in the
    /// storage `construct` passes it.
    ///
    /// # Safety
    /// `construct` must construct a CXStr at the pointer it's given.
    pub unsafe fn returned(construct: impl FnOnce(*mut crate::layouts::CXStr)) -> OwnedCXStr {
        let mut out = OwnedCXStr { rep: core::ptr::null_mut() };
        construct(out.as_mut_ptr());
        out
    }

    pub fn as_ptr(&self) -> *const crate::layouts::CXStr {
        (self as *const OwnedCXStr).cast()
    }

    /// Its text, NUL-terminated, valid while it lives.
    pub fn c_str(&self) -> *const c_char {
        unsafe { crate::eqlib::CXStr_c_str(self.as_ptr()) }
    }

    fn as_mut_ptr(&mut self) -> *mut crate::layouts::CXStr {
        (self as *mut OwnedCXStr).cast()
    }
}

impl Drop for OwnedCXStr {
    fn drop(&mut self) {
        unsafe { crate::eqlib::CXStr_drop(self.as_mut_ptr()) };
    }
}

/// `std::string_view` as MSVC lays it out (text, then length), which C++
/// functions take by value.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct StringView {
    pub data: *const c_char,
    pub len: usize,
}

impl StringView {
    /// A view of `text`, which must outlive the call it's passed to.
    pub fn new(text: &[u8]) -> StringView {
        StringView { data: text.as_ptr().cast(), len: text.len() }
    }
}

/// `fEQCommand` (`mq/api/CommandAPI.h`): a slash-command handler.
/// `command` is the argument text after the command name.
pub type fEQCommand = Option<unsafe extern "C" fn(player: *mut PlayerClient, command: *const c_char)>;

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, size_of};

    #[test]
    fn foreign_pointer_is_one_pointer() {
        assert_eq!(size_of::<ForeignPointer<PlayerClient>>(), size_of::<*mut ()>());
        assert_eq!(align_of::<ForeignPointer<PlayerClient>>(), align_of::<*mut ()>());
    }

    #[test]
    fn foreign_pointer_get_reads_through_slot() {
        let mut target = 0u32;
        let mut slot: *mut u32 = &mut target;
        let fp = ForeignPointer { m_ptr: &mut slot };
        assert_eq!(unsafe { fp.get() }, &mut target as *mut u32);

        slot = core::ptr::null_mut();
        let fp = ForeignPointer { m_ptr: &mut slot };
        assert!(unsafe { fp.get() }.is_null());

        let unset = ForeignPointer::<u32> { m_ptr: core::ptr::null_mut() };
        assert!(unsafe { unset.get() }.is_null());
    }

    #[test]
    fn computed_pointer_is_one_pointer_and_calls_its_getter() {
        assert_eq!(size_of::<ComputedPointer<u32>>(), size_of::<*mut ()>());
        static mut VALUE: u32 = 7;
        unsafe extern "C" fn getter() -> *mut u32 {
            &raw mut VALUE
        }
        let computed = ComputedPointer { m_fn: Some(getter) };
        assert_eq!(unsafe { computed.get() }, &raw mut VALUE);
        let unset = ComputedPointer::<u32> { m_fn: None };
        assert!(unsafe { unset.get() }.is_null());
    }

    #[test]
    fn mq_types_are_opaque_and_game_types_have_layouts() {
        assert_eq!(size_of::<MQTypeVar>(), 0);
        assert_eq!(size_of::<PluginInterface>(), 0);
        // From eqlib.h: PlayerClient_size = 0x1f08 for the emu client.
        assert_eq!(size_of::<PlayerClient>(), 0x1f08);
    }

    #[test]
    fn optional_callbacks_are_nullable_pointers() {
        // Option<fn> must use the null niche so it matches a C function pointer.
        assert_eq!(size_of::<fEQCommand>(), size_of::<*mut ()>());
    }
}
