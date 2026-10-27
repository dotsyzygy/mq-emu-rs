//! A plugin's own EQ UI windows, made from screen pieces in UI XML files it
//! adds, as a C++ plugin's `AddXMLFile` and `CCustomWnd` make them, with
//! [hooks](super::WindowHooks) for what they get.
//!
//! The window's code (what passes its notifications to the plugin) is in the
//! plugin's DLL, so the runtime deletes it before the UI is cleaned up and
//! before the plugin unloads, and makes it again whenever the UI loads.

use std::ffi::c_void;
use std::ops::ControlFlow;

use mq_emu_sys::layouts::CXWnd;

use super::hooks::hook_trampoline;
use super::{Window, WindowEvent, WindowHooks};
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::runtime::Runtime;
use crate::text::to_c;

/// One of this plugin's own windows, from [`Mq::add_window`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PluginWindowId(pub(crate) u64);

pub(crate) struct PluginWindowSlot<P: Plugin> {
    pub id: u64,
    pub screen: String,
    pub hooks: WindowHooks<P>,
    /// The window while the UI has it; null before it's made and after
    /// it's deleted.
    pub raw: *mut CXWnd,
}

impl<P: Plugin> Runtime<P> {
    /// Makes the plugin's windows the UI doesn't have yet, if it has their
    /// screen pieces.
    pub(crate) fn create_plugin_windows(&'static self) {
        let api = self.host().windows;
        let user = std::ptr::from_ref(self).cast_mut().cast::<c_void>();
        let mut slots = self.plugin_windows.borrow_mut();
        for slot in slots.iter_mut().filter(|slot| slot.raw.is_null()) {
            let screen = to_c(&slot.screen);
            slot.raw = unsafe { (api.mqemu_plugin_window_create)(screen.as_ptr(), hook_trampoline::<P>, user, slot.hooks.mask()) };
        }
    }

    /// Deletes the plugin's windows, keeping them to make again when the UI
    /// next loads, and the removed ones still waiting to be deleted.
    pub(crate) fn destroy_plugin_windows(&self) {
        // Taken out first: deleting one can't come back here.
        let mut windows: Vec<*mut CXWnd> =
            self.plugin_windows.borrow_mut().iter_mut().map(|slot| std::mem::replace(&mut slot.raw, std::ptr::null_mut())).collect();
        windows.append(&mut self.removed_windows.borrow_mut());
        self.delete_windows(windows);
    }

    /// Deletes the windows [`Mq::remove_window`] removed. Called where no
    /// window's code is running: removed from inside its own notification,
    /// a window can't be deleted until the game is done with it.
    pub(crate) fn delete_removed_windows(&self) {
        let windows = std::mem::take(&mut *self.removed_windows.borrow_mut());
        self.delete_windows(windows);
    }

    fn delete_windows(&self, windows: Vec<*mut CXWnd>) {
        let api = self.host().windows;
        for window in windows.into_iter().filter(|w| !w.is_null()) {
            unsafe { (api.mqemu_plugin_window_destroy)(window) };
        }
    }

    /// At unload: deletes the windows and forgets them, and takes the
    /// plugin's XML files back out of the UI.
    pub(crate) fn remove_all_plugin_windows(&self) {
        self.destroy_plugin_windows();
        self.plugin_windows.borrow_mut().clear();
        let api = self.host().windows;
        for file in self.xml_files.borrow_mut().drain(..) {
            unsafe { (api.mqemu_remove_xml_file)(to_c(&file).as_ptr()) };
        }
    }
}

impl<P: Plugin> Mq<P> {
    /// Adds a file of UI XML to what MacroQuest includes when the UI loads,
    /// as a C++ plugin's `AddXMLFile` does: its screen pieces are what
    /// [`add_window`](Self::add_window) makes windows from. The file is
    /// looked for in `uifiles\<skin>` and then `uifiles\default`, in
    /// MacroQuest's `resources` folder or EverQuest's; MacroQuest says in
    /// chat if it isn't there.
    ///
    /// The UI only reads it when it next loads: add it in `Plugin::new`, and
    /// a plugin loaded before entering the world has its screen pieces in
    /// the in-game UI. Added while in game, it needs a UI reload
    /// (`/reloadui`, which freezes the game while it runs) before its
    /// screen pieces are [loaded](Self::is_screen_piece_loaded). It's taken back out
    /// (`RemoveXMLFile`) when the plugin unloads. False for an empty name.
    pub fn add_xml_file(&self, filename: &str) -> bool {
        if filename.is_empty() || filename.contains('\0') {
            return false;
        }
        let runtime = self.runtime();
        if !unsafe { (self.window_api().mqemu_add_xml_file)(to_c(filename).as_ptr()) } {
            return false;
        }
        let mut files = runtime.xml_files.borrow_mut();
        if !files.iter().any(|file| file.eq_ignore_ascii_case(filename)) {
            files.push(filename.to_owned());
        }
        true
    }

    /// Takes a file [`add_xml_file`](Self::add_xml_file) added back out,
    /// from the next UI load on. False if this plugin hadn't added it.
    pub fn remove_xml_file(&self, filename: &str) -> bool {
        let mut files = self.runtime().xml_files.borrow_mut();
        let Some(position) = files.iter().position(|file| file.eq_ignore_ascii_case(filename)) else { return false };
        let file = files.remove(position);
        unsafe { (self.window_api().mqemu_remove_xml_file)(to_c(&file).as_ptr()) }
    }

    /// Whether the loaded UI has a screen piece (a window's template in UI
    /// XML) by this name (`IsScreenPieceLoaded`).
    pub fn is_screen_piece_loaded(&self, screen: &str) -> bool {
        !screen.contains('\0') && unsafe { (self.window_api().mqemu_screen_piece_loaded)(to_c(screen).as_ptr()) }
    }

    /// Adds a window of the plugin's own, made from the screen piece `screen`
    /// (with its controls) as a C++ plugin's `CCustomWnd` makes one: shown,
    /// its place saved in the character's UI ini. It's made now if the UI
    /// has the screen piece, and again each time the UI loads; it's deleted
    /// when the UI is cleaned up (before `on_clean_ui` returns to the game)
    /// and when the plugin unloads. [`plugin_window`](Self::plugin_window)
    /// finds it while it exists.
    ///
    /// `handler` gets each notification the window gets, from its controls
    /// (a button clicked, a list row picked, Enter in an edit box...) or from
    /// `/notify`, before the window handles it as any screen does (the
    /// close box hides it). [`add_window_with`](Self::add_window_with) also
    /// draws on the window and takes the mouse over it.
    pub fn add_window<F>(&self, screen: &str, handler: F) -> PluginWindowId
    where
        F: Fn(&mut P, &Mq<P>, &WindowEvent<'_>) + 'static,
    {
        self.add_window_with(
            screen,
            WindowHooks::new().on_notification(move |plugin, mq, event| {
                handler(plugin, mq, event);
                ControlFlow::Continue(())
            }),
        )
    }

    /// Adds a window of the plugin's own as [`add_window`](Self::add_window)
    /// does, with `hooks` for what it gets: its notifications (which the
    /// plugin may handle instead of the window), drawing on it after it
    /// draws, and the mouse over it. A window that draws its own content
    /// can be a screen piece with no controls.
    pub fn add_window_with(&self, screen: &str, hooks: WindowHooks<P>) -> PluginWindowId {
        let runtime = self.runtime();
        let id = runtime.next_window_id.get();
        runtime.next_window_id.set(id + 1);
        let screen = screen.replace('\0', "");
        runtime.plugin_windows.borrow_mut().push(PluginWindowSlot { id, screen, hooks, raw: std::ptr::null_mut() });
        runtime.create_plugin_windows();
        PluginWindowId(id)
    }

    /// One of this plugin's windows, while the UI has it.
    pub fn plugin_window(&self, id: PluginWindowId) -> Option<Window<'_>> {
        let raw = self.runtime().plugin_windows.borrow().iter().find(|slot| slot.id == id.0)?.raw;
        // SAFETY: the runtime deletes its windows only between callbacks.
        unsafe { Window::from_raw(raw, self.window_api()) }
    }

    /// Removes one of this plugin's windows for good: it's hidden now, gets
    /// no more notifications, and is deleted before the next pulse (it may
    /// be the window whose notification is running). False if the plugin
    /// had none by that ID.
    pub fn remove_window(&self, id: PluginWindowId) -> bool {
        let runtime = self.runtime();
        let slot = {
            let mut slots = runtime.plugin_windows.borrow_mut();
            let Some(position) = slots.iter().position(|slot| slot.id == id.0) else { return false };
            slots.remove(position)
        };
        if !slot.raw.is_null() {
            unsafe { (self.window_api().mqemu_window_show)(slot.raw, false) };
            runtime.removed_windows.borrow_mut().push(slot.raw);
        }
        true
    }
}
