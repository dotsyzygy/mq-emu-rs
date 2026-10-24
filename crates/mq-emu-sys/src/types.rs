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

opaque! {
    /// `eqlib::PlayerClient` (a.k.a. `SPAWNINFO`). Layout depends on the client build.
    PlayerClient,
    /// `eqlib::EQGroundItem` (a.k.a. `GROUNDITEM`).
    EQGroundItem,
    /// `mq::MQTypeVar`, the value type used by TLOs and macro data.
    MQTypeVar,
    /// `mq::MQPlugin`, MacroQuest's bookkeeping record for a loaded plugin.
    MQPlugin,
    /// `mq::PluginInterface`, an optional interface a plugin can expose to others.
    PluginInterface,
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

/// `fEQCommand`: a slash-command handler registered with `AddCommand`.
/// `buffer` is the (mutable) argument text after the command name.
pub type fEQCommand = Option<unsafe extern "C" fn(player: *mut PlayerClient, buffer: *mut c_char)>;

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
    fn opaque_types_are_zero_sized() {
        assert_eq!(size_of::<PlayerClient>(), 0);
        assert_eq!(size_of::<MQTypeVar>(), 0);
    }

    #[test]
    fn optional_callbacks_are_nullable_pointers() {
        // Option<fn> must use the null niche so it matches a C function pointer.
        assert_eq!(size_of::<fEQCommand>(), size_of::<*mut ()>());
    }
}
