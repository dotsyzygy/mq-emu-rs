//! A plugin's own EQ UI windows, made from screen pieces in UI XML files it
//! adds, as a C++ plugin's `AddXMLFile` and `CCustomWnd` make them.
//!
//! The window's code (what passes its notifications to the plugin) is in the
//! plugin's DLL, so the runtime deletes it before the UI is cleaned up and
//! before the plugin unloads, and makes it again whenever the UI loads.

use std::ffi::{CStr, c_char, c_void};
use std::fmt;
use std::rc::Rc;

use mq_emu_sys::constants::XWM_LINK;
use mq_emu_sys::layouts::CXWnd;

use super::{Notification, Window};
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::runtime::{Event, Runtime, guard};
use crate::text::to_c;

/// One of this plugin's own windows, from [`Mq::add_window`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PluginWindowId(pub(crate) u64);

/// A notification one of this plugin's windows got, from one of its controls
/// (or from the game or a `/notify` naming no control).
pub struct WindowEvent<'a> {
    window: Window<'a>,
    sender: Option<Window<'a>>,
    message: u32,
    data: i32,
    link: Option<String>,
}

impl<'a> WindowEvent<'a> {
    /// The plugin's window.
    pub fn window(&self) -> Window<'a> {
        self.window
    }

    /// The control that sent it: the window itself or one inside it. `None`
    /// if none was named.
    pub fn sender(&self) -> Option<Window<'a>> {
        self.sender
    }

    /// What it is, if it's one `/notify` names; [`message`](Self::message) has every kind.
    pub fn notification(&self) -> Option<Notification> {
        Notification::from_raw(self.message)
    }

    /// The message (`XWM_*`).
    pub fn message(&self) -> u32 {
        self.message
    }

    /// Its data: for a click on a list, the row; for a slider's new value,
    /// the value. 0 for a link, whose data is [`link`](Self::link).
    pub fn data(&self) -> i32 {
        self.data
    }

    /// For a `link` notification (`XWM_LINK`), the link's code: the link
    /// without its `\x12` markers, its tag digit and its text, as the game
    /// sends it (see [`Window::notify_link`]).
    pub fn link(&self) -> Option<&str> {
        self.link.as_deref()
    }
}

impl fmt::Debug for WindowEvent<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowEvent")
            .field("sender", &self.sender.and_then(|s| s.screen_id()))
            .field("message", &self.message)
            .field("data", &self.data)
            .field("link", &self.link)
            .finish()
    }
}

pub(crate) type WindowHandler<P> = dyn Fn(&mut P, &Mq<P>, &WindowEvent<'_>);

pub(crate) struct PluginWindowSlot<P: Plugin> {
    pub id: u64,
    pub screen: String,
    pub handler: Rc<WindowHandler<P>>,
    /// The window while the UI has it; null before it's made and after
    /// it's deleted.
    pub raw: *mut CXWnd,
}

/// A notification as the window got it, kept until the plugin can take it.
pub(crate) struct WindowNotice {
    pub window: *mut CXWnd,
    pub sender: *mut CXWnd,
    pub message: u32,
    pub data: i32,
    pub link: Option<String>,
}

/// What a plugin's window calls with each notification; `user` is the runtime.
pub(crate) unsafe extern "C" fn notification_trampoline<P: Plugin>(
    user: *mut c_void,
    window: *mut CXWnd,
    sender: *mut CXWnd,
    message: u32,
    data: *mut c_void,
) {
    // SAFETY: the runtime is a static that made the window.
    let runtime = unsafe { &*user.cast::<Runtime<P>>() };
    // A link's data is its text, there only for this call.
    let link = (message == XWM_LINK && !data.is_null()).then(|| unsafe { CStr::from_ptr(data.cast::<c_char>()) }.to_string_lossy().into_owned());
    let data = if link.is_some() { 0 } else { data as usize as i32 };
    guard(|| {
        runtime.enter(Event::Window(WindowNotice { window, sender, message, data, link }));
    });
}

impl<P: Plugin> Runtime<P> {
    /// Delivers a notice to its window's handler, if the window is still
    /// the one the plugin has (the notice may have waited in the queue).
    pub(crate) fn deliver_window_notice(&'static self, plugin: &mut P, mq: &Mq<P>, notice: WindowNotice) {
        let handler = self.plugin_windows.borrow().iter().find(|slot| slot.raw == notice.window).map(|slot| slot.handler.clone());
        let Some(handler) = handler else { return };
        let api = self.host().windows;
        // SAFETY: the window is live: the runtime deletes it, and forgets it,
        // only between callbacks.
        let Some(window) = (unsafe { Window::from_raw(notice.window, api) }) else { return };
        // Only a control still inside it.
        let sender = (!notice.sender.is_null())
            .then(|| std::iter::once(window).chain(window.descendants()).find(|w| std::ptr::eq(w.raw, notice.sender)))
            .flatten();
        handler(plugin, mq, &WindowEvent { window, sender, message: notice.message, data: notice.data, link: notice.link });
    }

    /// Makes the plugin's windows the UI doesn't have yet, if it has their
    /// screen pieces.
    pub(crate) fn create_plugin_windows(&'static self) {
        let api = self.host().windows;
        let user = std::ptr::from_ref(self).cast_mut().cast::<c_void>();
        let mut slots = self.plugin_windows.borrow_mut();
        for slot in slots.iter_mut().filter(|slot| slot.raw.is_null()) {
            let screen = to_c(&slot.screen);
            slot.raw = unsafe { (api.mqemu_plugin_window_create)(screen.as_ptr(), notification_trampoline::<P>, user) };
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
    /// close box hides it).
    pub fn add_window<F>(&self, screen: &str, handler: F) -> PluginWindowId
    where
        F: Fn(&mut P, &Mq<P>, &WindowEvent<'_>) + 'static,
    {
        let runtime = self.runtime();
        let id = runtime.next_window_id.get();
        runtime.next_window_id.set(id + 1);
        let screen = screen.replace('\0', "");
        runtime.plugin_windows.borrow_mut().push(PluginWindowSlot { id, screen, handler: Rc::new(handler), raw: std::ptr::null_mut() });
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
