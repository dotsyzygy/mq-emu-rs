//! What a plugin hooks in a window, its own ([`Mq::add_window_with`]) or one
//! of the game's it subclassed ([`Mq::subclass_window`]): notifications from
//! its controls, the mouse over it, and drawing on it after it draws.
//!
//! The window calls into the plugin while the game runs its UI, outside the
//! plugin's other callbacks, so the plugin's answer (handled or not) reaches
//! the window. What the window gets while one of the plugin's callbacks runs
//! (a notification the plugin's own click caused, say) is delivered after
//! that callback, and the window has handled it by then.

use std::ffi::{CStr, c_char, c_void};
use std::fmt;
use std::ops::ControlFlow;
use std::rc::Rc;

use mq_emu_sys::constants::XWM_LINK;
use mq_emu_sys::layouts::CXWnd;
use mq_emu_sys::windows::{
    HOOK_DRAW, HOOK_LBUTTON_DOWN, HOOK_LBUTTON_UP, HOOK_MOUSE_MOVE, HOOK_NOTIFICATION, HOOK_RBUTTON_DOWN, HOOK_RBUTTON_UP, HOOK_WHEEL,
    mqemu_window_event,
};

use super::{Notification, Painter, Window};
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::runtime::{Event, Runtime, guard_or};

/// A notification a window got, from one of its controls (or from the game
/// or a `/notify` naming no control).
pub struct WindowEvent<'a> {
    pub(super) window: Window<'a>,
    pub(super) sender: Option<Window<'a>>,
    pub(super) message: u32,
    pub(super) data: i32,
    pub(super) link: Option<String>,
}

impl<'a> WindowEvent<'a> {
    /// The window: the plugin's own, or the one it subclassed.
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

/// What the mouse did over a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseAction {
    LeftDown,
    LeftUp,
    RightDown,
    RightUp,
    /// It moved.
    Move,
    /// The wheel turned this far (up is positive; one notch is usually 120).
    Wheel(i32),
}

/// The mouse over a window, as the game tells the window: over the window
/// itself, not one of its controls (which get it instead).
#[derive(Debug, Clone, Copy)]
pub struct MouseEvent<'a> {
    window: Window<'a>,
    action: MouseAction,
    x: i32,
    y: i32,
    flags: u32,
}

impl<'a> MouseEvent<'a> {
    pub fn window(&self) -> Window<'a> {
        self.window
    }

    pub fn action(&self) -> MouseAction {
        self.action
    }

    /// Where the mouse is, on screen (as [`Window::screen_rect`] places
    /// windows).
    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// The flags the game passed with it (which keys were held).
    pub fn flags(&self) -> u32 {
        self.flags
    }
}

pub(crate) type NotificationHandler<P> = dyn Fn(&mut P, &Mq<P>, &WindowEvent<'_>) -> ControlFlow<()>;
pub(crate) type DrawHandler<P> = dyn Fn(&mut P, &Mq<P>, &Painter<'_>);
pub(crate) type MouseHandler<P> = dyn Fn(&mut P, &Mq<P>, &MouseEvent<'_>) -> ControlFlow<()>;

/// What a plugin does with what a window gets, for
/// [`Mq::add_window_with`] and [`Mq::subclass_window`]. A handler returning
/// [`ControlFlow::Break`] has handled it: the window doesn't. With
/// [`ControlFlow::Continue`] the window goes on to handle it as it would
/// have.
///
/// ```no_run
/// # use std::ops::ControlFlow;
/// # use mq_emu::{Mq, Plugin, WindowHooks, imgui::EqTextFlags};
/// # struct Counter { clicks: u32 }
/// # impl Plugin for Counter { fn new(_: &Mq<Self>) -> Self { Counter { clicks: 0 } } }
/// # fn f(mq: &Mq<Counter>) {
/// let hooks = WindowHooks::new()
///     .on_notification(|plugin: &mut Counter, _mq, event| {
///         plugin.clicks += 1;
///         ControlFlow::Continue(())
///     })
///     .on_draw(|plugin, _mq, painter| {
///         let text = format!("{} clicks", plugin.clicks);
///         painter.text(2, &text, painter.clip_rect(), 0xffffff00, EqTextFlags::NONE);
///     });
/// mq.subclass_window("InventoryWindow", hooks);
/// # }
/// ```
pub struct WindowHooks<P: Plugin> {
    pub(crate) notification: Option<Rc<NotificationHandler<P>>>,
    pub(crate) draw: Option<Rc<DrawHandler<P>>>,
    pub(crate) mouse: Option<Rc<MouseHandler<P>>>,
}

impl<P: Plugin> WindowHooks<P> {
    /// Hooks nothing yet.
    pub fn new() -> WindowHooks<P> {
        WindowHooks { notification: None, draw: None, mouse: None }
    }

    /// Each notification the window gets, from its controls (a button
    /// clicked, a list row picked, Enter in an edit box...) or from
    /// `/notify`, before the window handles it.
    pub fn on_notification<F>(mut self, handler: F) -> WindowHooks<P>
    where
        F: Fn(&mut P, &Mq<P>, &WindowEvent<'_>) -> ControlFlow<()> + 'static,
    {
        self.notification = Some(Rc::new(handler));
        self
    }

    /// Each time the window has drawn (every frame it's shown), to draw on
    /// it with the game's own drawing (see [`Painter`]). MacroQuest's map
    /// draws its labels here.
    pub fn on_draw<F>(mut self, handler: F) -> WindowHooks<P>
    where
        F: Fn(&mut P, &Mq<P>, &Painter<'_>) + 'static,
    {
        self.draw = Some(Rc::new(handler));
        self
    }

    /// The mouse over the window (buttons, moves and the wheel), before the
    /// window handles it.
    pub fn on_mouse<F>(mut self, handler: F) -> WindowHooks<P>
    where
        F: Fn(&mut P, &Mq<P>, &MouseEvent<'_>) -> ControlFlow<()> + 'static,
    {
        self.mouse = Some(Rc::new(handler));
        self
    }

    /// A bit per `HOOK_*` kind the window should pass on.
    pub(crate) fn mask(&self) -> u32 {
        let bit = |kind: i32| 1u32 << kind;
        let mut mask = 0;
        if self.notification.is_some() {
            mask |= bit(HOOK_NOTIFICATION);
        }
        if self.draw.is_some() {
            mask |= bit(HOOK_DRAW);
        }
        if self.mouse.is_some() {
            for kind in [HOOK_LBUTTON_DOWN, HOOK_LBUTTON_UP, HOOK_RBUTTON_DOWN, HOOK_RBUTTON_UP, HOOK_MOUSE_MOVE, HOOK_WHEEL] {
                mask |= bit(kind);
            }
        }
        mask
    }
}

impl<P: Plugin> Default for WindowHooks<P> {
    fn default() -> Self {
        WindowHooks::new()
    }
}

impl<P: Plugin> fmt::Debug for WindowHooks<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowHooks")
            .field("notification", &self.notification.is_some())
            .field("draw", &self.draw.is_some())
            .field("mouse", &self.mouse.is_some())
            .finish()
    }
}

/// What a window passed on, kept until the plugin can take it.
pub(crate) struct WindowNotice {
    pub window: *mut CXWnd,
    pub kind: NoticeKind,
}

pub(crate) enum NoticeKind {
    Notification { sender: *mut CXWnd, message: u32, data: i32, link: Option<String> },
    Draw,
    Mouse { action: MouseAction, x: i32, y: i32, flags: u32 },
}

impl WindowNotice {
    /// A drawing can't wait: by then the frame is drawn.
    pub(crate) fn queueable(&self) -> bool {
        !matches!(self.kind, NoticeKind::Draw)
    }
}

/// What a hooked window calls with each event; `user` is the runtime.
pub(crate) unsafe extern "C" fn hook_trampoline<P: Plugin>(user: *mut c_void, window: *mut CXWnd, event: *const mqemu_window_event) -> bool {
    // SAFETY: the runtime is a static that hooked the window, and the event
    // is the window's, for this call.
    let runtime = unsafe { &*user.cast::<Runtime<P>>() };
    let event = unsafe { &*event };
    let mouse = |action| NoticeKind::Mouse { action, x: event.x, y: event.y, flags: event.flags };
    let kind = match event.kind {
        HOOK_NOTIFICATION => {
            // A link's data is its text, there only for this call.
            let link = (event.message == XWM_LINK && !event.data.is_null())
                .then(|| unsafe { CStr::from_ptr(event.data.cast::<c_char>()) }.to_string_lossy().into_owned());
            let data = if link.is_some() { 0 } else { event.data as usize as i32 };
            NoticeKind::Notification { sender: event.sender, message: event.message, data, link }
        }
        HOOK_DRAW => NoticeKind::Draw,
        HOOK_LBUTTON_DOWN => mouse(MouseAction::LeftDown),
        HOOK_LBUTTON_UP => mouse(MouseAction::LeftUp),
        HOOK_RBUTTON_DOWN => mouse(MouseAction::RightDown),
        HOOK_RBUTTON_UP => mouse(MouseAction::RightUp),
        HOOK_MOUSE_MOVE => mouse(MouseAction::Move),
        HOOK_WHEEL => mouse(MouseAction::Wheel(event.scroll)),
        _ => return false,
    };
    guard_or(false, || runtime.enter(Event::Window(WindowNotice { window, kind })))
}

/// A window's hooks, found by the window.
struct Hooked<P: Plugin> {
    notification: Option<Rc<NotificationHandler<P>>>,
    draw: Option<Rc<DrawHandler<P>>>,
    mouse: Option<Rc<MouseHandler<P>>>,
}

impl<P: Plugin> Runtime<P> {
    /// The hooks of one of the plugin's windows or of a window it subclassed,
    /// cloned out so a handler may remove them.
    fn hooks_of(&self, window: *mut CXWnd) -> Option<Hooked<P>> {
        let clone = |hooks: &WindowHooks<P>| Hooked { notification: hooks.notification.clone(), draw: hooks.draw.clone(), mouse: hooks.mouse.clone() };
        if let Some(slot) = self.plugin_windows.borrow().iter().find(|slot| slot.raw == window) {
            return Some(clone(&slot.hooks));
        }
        self.subclasses.borrow().iter().find(|slot| slot.raw == window).map(|slot| clone(&slot.hooks))
    }

    /// Delivers a notice to its window's hooks, if the window is still one
    /// the plugin has (the notice may have waited in the queue). True if the
    /// plugin handled it.
    pub(crate) fn deliver_window_notice(&'static self, plugin: &mut P, mq: &Mq<P>, notice: WindowNotice) -> bool {
        let Some(hooks) = self.hooks_of(notice.window) else { return false };
        let api = self.host().windows;
        // SAFETY: the window is live: the runtime forgets its windows, and
        // takes its subclasses off, only between callbacks.
        let Some(window) = (unsafe { Window::from_raw(notice.window, api) }) else { return false };
        match notice.kind {
            NoticeKind::Notification { sender, message, data, link } => {
                let Some(handler) = hooks.notification else { return false };
                // Only a control still inside it.
                let sender = (!sender.is_null())
                    .then(|| std::iter::once(window).chain(window.descendants()).find(|w| std::ptr::eq(w.raw, sender)))
                    .flatten();
                handler(plugin, mq, &WindowEvent { window, sender, message, data, link }).is_break()
            }
            NoticeKind::Draw => {
                if let Some(handler) = hooks.draw {
                    handler(plugin, mq, &Painter::new(window));
                }
                false
            }
            NoticeKind::Mouse { action, x, y, flags } => {
                let Some(handler) = hooks.mouse else { return false };
                handler(plugin, mq, &MouseEvent { window, action, x, y, flags }).is_break()
            }
        }
    }
}
