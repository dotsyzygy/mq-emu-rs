//! Subclassing the game's own windows, as a C++ plugin's `WindowOverride`
//! does: its notifications, the mouse over it and drawing on it pass
//! through the plugin's [hooks](super::WindowHooks) first.
//!
//! The hooks' code is in the plugin's DLL, so the runtime takes a subclass
//! off before the UI is cleaned up (when the game deletes its windows) and
//! before the plugin unloads, and puts it back on when the window is there
//! again.

use std::ffi::c_void;

use mq_emu_sys::layouts::CXWnd;

use super::hooks::hook_trampoline;
use super::{Window, WindowHooks};
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::runtime::Runtime;

/// One of the game's windows this plugin subclassed, from [`Mq::subclass_window`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubclassId(pub(crate) u64);

pub(crate) struct SubclassSlot<P: Plugin> {
    pub id: u64,
    /// The window, as [`Mq::window`] finds it.
    pub name: String,
    pub hooks: WindowHooks<P>,
    /// The window while it's subclassed; null otherwise.
    pub raw: *mut CXWnd,
}

impl<P: Plugin> Runtime<P> {
    /// Subclasses the windows the plugin asked for that aren't yet, if the
    /// game has them.
    pub(crate) fn attach_subclasses(&'static self) {
        if self.subclasses.borrow().iter().all(|slot| !slot.raw.is_null()) {
            return;
        }
        let api = self.host().windows;
        let user = std::ptr::from_ref(self).cast_mut().cast::<c_void>();
        let mq = Mq::new(self);
        let mut slots = self.subclasses.borrow_mut();
        for i in 0..slots.len() {
            if !slots[i].raw.is_null() {
                continue;
            }
            let Some(window) = mq.window(&slots[i].name) else { continue };
            let raw = window.ptr();
            // Two subclasses of one window: the first one has it.
            if slots.iter().any(|other| other.raw == raw) {
                continue;
            }
            if unsafe { (api.mqemu_window_subclass)(raw, hook_trampoline::<P>, user, slots[i].hooks.mask()) } {
                slots[i].raw = raw;
            }
        }
    }

    /// Takes the plugin's subclasses off, keeping them to put back on when
    /// the windows are there again.
    pub(crate) fn detach_subclasses(&'static self) {
        let windows: Vec<*mut CXWnd> =
            self.subclasses.borrow_mut().iter_mut().map(|slot| std::mem::replace(&mut slot.raw, std::ptr::null_mut())).collect();
        for raw in windows.into_iter().filter(|raw| !raw.is_null()) {
            self.unsubclass(raw);
        }
    }

    /// Takes one subclass off, touching the window only if the game still has it.
    fn unsubclass(&'static self, raw: *mut CXWnd) {
        let live = Mq::new(self).has_window(raw);
        unsafe { (self.host().windows.mqemu_window_unsubclass)(raw, live) };
    }

    /// At unload: takes the subclasses off and forgets them.
    pub(crate) fn remove_all_subclasses(&'static self) {
        self.detach_subclasses();
        self.subclasses.borrow_mut().clear();
    }
}

impl<P: Plugin> Mq<P> {
    /// Subclasses one of the game's windows, found as [`window`](Self::window)
    /// finds it (a top-level window by name, or a path to a control in one),
    /// as a C++ plugin's `WindowOverride` does: `hooks` get what it gets (its
    /// notifications, the mouse over it) before it does, and may handle it
    /// instead, and draw on it after it draws.
    ///
    /// It's subclassed now if the game has the window, or as soon as it does
    /// (checked each pulse, and when the UI loads); taken off when the UI is
    /// cleaned up (after `on_clean_ui`) and put back when it loads; and taken
    /// off for good with [`remove_subclass`](Self::remove_subclass) or when
    /// the plugin unloads. Only the window first found by the name is
    /// subclassed (item display windows share theirs). Other hooks on the
    /// same window (another plugin's) stack with it.
    pub fn subclass_window(&self, name: &str, hooks: WindowHooks<P>) -> SubclassId {
        let runtime = self.runtime();
        let id = runtime.next_window_id.get();
        runtime.next_window_id.set(id + 1);
        runtime.subclasses.borrow_mut().push(SubclassSlot { id, name: name.to_owned(), hooks, raw: std::ptr::null_mut() });
        runtime.attach_subclasses();
        SubclassId(id)
    }

    /// The window a subclass is on, while it's on one.
    pub fn subclassed_window(&self, id: SubclassId) -> Option<Window<'_>> {
        let raw = self.runtime().subclasses.borrow().iter().find(|slot| slot.id == id.0)?.raw;
        // SAFETY: the runtime takes subclasses off before the game deletes
        // the windows, between callbacks.
        unsafe { Window::from_raw(raw, self.window_api()) }
    }

    /// Takes a subclass off for good, now (even from inside its own hook).
    /// False if the plugin had none by that ID.
    pub fn remove_subclass(&self, id: SubclassId) -> bool {
        let runtime = self.runtime();
        let slot = {
            let mut slots = runtime.subclasses.borrow_mut();
            let Some(position) = slots.iter().position(|slot| slot.id == id.0) else { return false };
            slots.remove(position)
        };
        if !slot.raw.is_null() {
            runtime.unsubclass(slot.raw);
        }
        true
    }

    /// Whether the game still has `raw`: a top-level window or one inside one.
    pub(crate) fn has_window(&self, raw: *mut CXWnd) -> bool {
        self.windows().any(|top| std::ptr::eq(top.raw, raw) || top.descendants().any(|w| std::ptr::eq(w.raw, raw)))
    }
}
