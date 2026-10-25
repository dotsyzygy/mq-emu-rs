//! The per-plugin runtime behind [`plugin!`](crate::plugin!): owns the plugin
//! value, dispatches MacroQuest callbacks to it, and keeps it safe from
//! re-entrant calls and panics.
//!
//! Everything here runs on the game thread: MacroQuest calls plugins only from
//! there, and [`Mq`] (the only way to reach the runtime from plugin code) is
//! `!Send`, so it can't leave that thread.

use std::any::Any;
use std::cell::{Cell, RefCell, UnsafeCell};
use std::collections::VecDeque;
use std::ffi::{c_char, c_void};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::time::Duration;

use mq_emu_sys::constants::CONCOLOR_RED;
use mq_emu_sys::plugin::MQPluginHandle;
use mq_emu_sys::shim;
use mq_emu_sys::types::{EQGroundItem, PlayerClient};

use crate::game::{GameState, Spawn};
use crate::host::Host;
use crate::mq::{CommandError, Mq};
use crate::plugin::Plugin;
use crate::stats::{Stats, Warnings};
use crate::text::{from_c, to_c};

/// Most re-entrant events held at once; more are dropped.
pub const MAX_QUEUE: usize = 256;
/// Most queued events delivered after one callback; the rest are dropped. A
/// callback that keeps causing events for itself (e.g. echoing its own chat)
/// would otherwise never return control to the game.
pub const MAX_NESTED_DELIVERIES: usize = 64;

/// A callback from MacroQuest, with the data it needs owned so it can wait in
/// the queue. Spawn and ground item events carry game pointers that may be
/// gone by the time a queued event runs, so those are never queued.
pub(crate) enum Event {
    Pulse,
    IncomingChat(String, u32),
    WriteChatColor(String, u32),
    Zoned,
    CleanUi,
    ReloadUi,
    DrawHud,
    SetGameState(i32),
    AddSpawn(*mut PlayerClient),
    RemoveSpawn(*mut PlayerClient),
    AddGroundItem(*mut EQGroundItem),
    RemoveGroundItem(*mut EQGroundItem),
    BeginZone,
    EndZone,
    UpdateImGui,
    MacroStart(String),
    MacroStop(String),
    LoadPlugin(String),
    UnloadPlugin(String),
    PostUnloadPlugin(String),
    Command(u64, String),
}

impl Event {
    fn kind(&self) -> (u32, &'static str) {
        match self {
            Event::Pulse => (0, "on_pulse"),
            Event::IncomingChat(..) => (1, "on_incoming_chat"),
            Event::WriteChatColor(..) => (2, "on_write_chat_color"),
            Event::Zoned => (3, "on_zoned"),
            Event::CleanUi => (4, "on_clean_ui"),
            Event::ReloadUi => (5, "on_reload_ui"),
            Event::DrawHud => (6, "on_draw_hud"),
            Event::SetGameState(_) => (7, "on_set_game_state"),
            Event::AddSpawn(_) => (8, "on_add_spawn"),
            Event::RemoveSpawn(_) => (9, "on_remove_spawn"),
            Event::AddGroundItem(_) => (10, "on_add_ground_item"),
            Event::RemoveGroundItem(_) => (11, "on_remove_ground_item"),
            Event::BeginZone => (12, "on_begin_zone"),
            Event::EndZone => (13, "on_end_zone"),
            Event::UpdateImGui => (14, "on_update_imgui"),
            Event::MacroStart(_) => (15, "on_macro_start"),
            Event::MacroStop(_) => (16, "on_macro_stop"),
            Event::LoadPlugin(_) => (17, "on_load_plugin"),
            Event::UnloadPlugin(_) => (18, "on_unload_plugin"),
            Event::PostUnloadPlugin(_) => (19, "on_post_unload_plugin"),
            Event::Command(..) => (20, "a command handler"),
        }
    }

    fn queueable(&self) -> bool {
        !matches!(self, Event::AddSpawn(_) | Event::RemoveSpawn(_) | Event::AddGroundItem(_) | Event::RemoveGroundItem(_))
    }
}

/// Kinds used only by the runtime itself, for once-per-kind panic reports.
const KIND_NEW: (u32, &str) = (21, "Plugin::new");
const KIND_SHUTDOWN: (u32, &str) = (22, "shutdown");

pub(crate) type CommandHandler<P> = dyn Fn(&mut P, &Mq<P>, &str);

pub(crate) struct CommandSlot<P: Plugin> {
    id: u64,
    name: String,
    handler: Rc<CommandHandler<P>>,
    runtime: &'static Runtime<P>,
}

/// One plugin's runtime. Created by [`plugin!`](crate::plugin!) as a `static`.
pub struct Runtime<P: Plugin> {
    name: &'static str,
    host: Host,
    plugin: UnsafeCell<Option<P>>,
    /// True while a plugin callback runs; `plugin` is then exclusively borrowed.
    busy: Cell<bool>,
    handle: Cell<MQPluginHandle>,
    queue: RefCell<VecDeque<Event>>,
    commands: RefCell<Vec<Rc<CommandSlot<P>>>>,
    /// Removed command slots, freed once no callback is running.
    retired: RefCell<Vec<Rc<CommandSlot<P>>>>,
    next_command_id: Cell<u64>,
    stats: Cell<Stats>,
    warnings: RefCell<Warnings>,
    /// Bit per event kind whose panic message has already been shown.
    reported_panics: Cell<u32>,
    /// True while the runtime writes its own diagnostics, which must not come
    /// back to the plugin as chat events (they could feed a loop they report).
    writing_diagnostic: Cell<bool>,
}

// SAFETY: MacroQuest calls plugins only from the game thread, and plugin code
// can only reach the runtime through `Mq`, which is `!Send`. The runtime is
// never touched from two threads.
unsafe impl<P: Plugin> Sync for Runtime<P> {}

impl<P: Plugin> Runtime<P> {
    /// A runtime that talks to MacroQuest. `name` labels its chat messages.
    pub const fn new(name: &'static str) -> Runtime<P> {
        Runtime::with_host(name, crate::host::REAL)
    }

    pub(crate) const fn with_host(name: &'static str, host: Host) -> Runtime<P> {
        Runtime {
            name,
            host,
            plugin: UnsafeCell::new(None),
            busy: Cell::new(false),
            handle: Cell::new(MQPluginHandle { pluginID: 0 }),
            queue: RefCell::new(VecDeque::new()),
            commands: RefCell::new(Vec::new()),
            retired: RefCell::new(Vec::new()),
            next_command_id: Cell::new(1),
            stats: Cell::new(Stats::new()),
            warnings: RefCell::new(Warnings::new()),
            reported_panics: Cell::new(0),
            writing_diagnostic: Cell::new(false),
        }
    }

    pub(crate) fn name(&self) -> &'static str {
        self.name
    }

    pub(crate) fn host(&self) -> &Host {
        &self.host
    }

    // ---------------------------------------------------------------------
    // Entry points, called by the exports `plugin!` generates. None of them
    // may unwind: a panic escaping an `extern "C"` function aborts the game.
    // ---------------------------------------------------------------------

    pub fn init_handle(&'static self, handle: MQPluginHandle) {
        self.handle.set(handle);
    }

    pub fn initialize(&'static self) {
        guard(|| {
            if self.busy.get() || self.has_plugin() {
                return;
            }
            self.busy.set(true);
            let mq = Mq::new(self);
            match catch_unwind(AssertUnwindSafe(|| P::new(&mq))) {
                // SAFETY: busy, so nothing else is using `plugin`.
                Ok(plugin) => unsafe { *self.plugin.get() = Some(plugin) },
                Err(payload) => self.on_panic(KIND_NEW, payload.as_ref()),
            }
            self.drain();
            self.busy.set(false);
            self.free_retired();
        })
    }

    pub fn shutdown(&'static self) {
        guard(|| {
            let was_busy = self.busy.replace(true);
            // SAFETY: busy, so nothing else is using `plugin`.
            if let Some(plugin) = unsafe { (*self.plugin.get()).as_mut() } {
                let mq = Mq::new(self);
                if let Err(payload) = catch_unwind(AssertUnwindSafe(|| plugin.shutdown(&mq))) {
                    self.on_panic(KIND_SHUTDOWN, payload.as_ref());
                }
            }
            // Nothing is delivered after shutdown.
            self.queue.borrow_mut().clear();
            self.remove_all_commands();
            let plugin = unsafe { (*self.plugin.get()).take() };
            if catch_unwind(AssertUnwindSafe(move || drop(plugin))).is_err() {
                self.diagnostic("panicked while dropping the plugin");
            }
            self.busy.set(was_busy);
            if !was_busy {
                self.retired.borrow_mut().clear();
            }
        })
    }

    pub fn pulse(&'static self) {
        guard(|| {
            if self.busy.get() {
                self.enqueue(Event::Pulse);
                return;
            }
            let start = (self.host.now)();
            self.enter(Event::Pulse);
            let took = (self.host.now)().saturating_duration_since(start);
            self.record_pulse(took);
            let warning = self.warnings.borrow_mut().poll((self.host.now)());
            if let Some(warning) = warning {
                self.diagnostic(&format!("warning: {warning}"));
            }
        })
    }

    /// # Safety
    /// `line` must be null or a valid NUL-terminated string.
    pub unsafe fn incoming_chat(&'static self, line: *const c_char, color: u32) -> bool {
        let line = unsafe { from_c(line) }.into_owned();
        guard_or(false, || self.enter(Event::IncomingChat(line, color)))
    }

    /// # Safety
    /// `line` must be null or a valid NUL-terminated string.
    pub unsafe fn write_chat_color(&'static self, line: *const c_char, color: u32) -> i32 {
        if self.writing_diagnostic.get() {
            return 0;
        }
        let line = unsafe { from_c(line) }.into_owned();
        guard(|| {
            self.enter(Event::WriteChatColor(line, color));
        });
        0
    }

    pub fn zoned(&'static self) {
        self.event(Event::Zoned)
    }
    pub fn clean_ui(&'static self) {
        self.event(Event::CleanUi)
    }
    pub fn reload_ui(&'static self) {
        self.event(Event::ReloadUi)
    }
    pub fn draw_hud(&'static self) {
        self.event(Event::DrawHud)
    }
    pub fn set_game_state(&'static self, state: i32) {
        self.event(Event::SetGameState(state))
    }
    pub fn add_spawn(&'static self, spawn: *mut PlayerClient) {
        self.event(Event::AddSpawn(spawn))
    }
    pub fn remove_spawn(&'static self, spawn: *mut PlayerClient) {
        self.event(Event::RemoveSpawn(spawn))
    }
    pub fn add_ground_item(&'static self, item: *mut EQGroundItem) {
        self.event(Event::AddGroundItem(item))
    }
    pub fn remove_ground_item(&'static self, item: *mut EQGroundItem) {
        self.event(Event::RemoveGroundItem(item))
    }
    pub fn begin_zone(&'static self) {
        self.event(Event::BeginZone)
    }
    pub fn end_zone(&'static self) {
        self.event(Event::EndZone)
    }
    pub fn update_imgui(&'static self) {
        self.event(Event::UpdateImGui)
    }

    /// The macro and plugin name callbacks.
    ///
    /// # Safety
    /// `name` must be null or a valid NUL-terminated string.
    pub unsafe fn named(&'static self, which: NamedEvent, name: *const c_char) {
        let name = unsafe { from_c(name) }.into_owned();
        self.event(match which {
            NamedEvent::MacroStart => Event::MacroStart(name),
            NamedEvent::MacroStop => Event::MacroStop(name),
            NamedEvent::LoadPlugin => Event::LoadPlugin(name),
            NamedEvent::UnloadPlugin => Event::UnloadPlugin(name),
            NamedEvent::PostUnloadPlugin => Event::PostUnloadPlugin(name),
        })
    }

    fn event(&'static self, event: Event) {
        guard(|| {
            self.enter(event);
        })
    }

    // ---------------------------------------------------------------------
    // Dispatch
    // ---------------------------------------------------------------------

    fn has_plugin(&self) -> bool {
        // SAFETY: only reads the discriminant; callers aren't inside a callback.
        unsafe { (*self.plugin.get()).is_some() }
    }

    /// Delivers `event`, or queues it if a callback is already running, then
    /// delivers whatever the callback queued. Returns the callback's result
    /// (only `on_incoming_chat` has one; a queued event returns false).
    fn enter(&'static self, event: Event) -> bool {
        if self.busy.get() {
            self.enqueue(event);
            return false;
        }
        self.busy.set(true);
        let result = self.run_one(event);
        self.drain();
        self.busy.set(false);
        self.free_retired();
        result
    }

    fn enqueue(&self, event: Event) {
        let mut stats = self.stats.get();
        if event.queueable() && self.queue.borrow().len() < MAX_QUEUE {
            self.queue.borrow_mut().push_back(event);
            stats.queued += 1;
        } else {
            stats.dropped += 1;
            self.warnings.borrow_mut().record_dropped(1);
        }
        self.stats.set(stats);
    }

    fn drain(&'static self) {
        let mut delivered = 0;
        loop {
            let next = self.queue.borrow_mut().pop_front();
            let Some(event) = next else { break };
            if delivered == MAX_NESTED_DELIVERIES {
                let rest = 1 + self.queue.borrow().len() as u64;
                self.queue.borrow_mut().clear();
                let mut stats = self.stats.get();
                stats.dropped += rest;
                stats.cap_hits += 1;
                self.stats.set(stats);
                self.warnings.borrow_mut().record_dropped(rest);
                break;
            }
            self.run_one(event);
            delivered += 1;
        }
    }

    /// Runs one event's callback. Must only be called while `busy`.
    fn run_one(&'static self, event: Event) -> bool {
        // SAFETY: the caller set `busy`, so this is the only borrow of `plugin`.
        let Some(plugin) = (unsafe { &mut *self.plugin.get() }).as_mut() else {
            return false;
        };
        let mq = Mq::new(self);
        let kind = event.kind();
        match catch_unwind(AssertUnwindSafe(|| self.deliver(plugin, &mq, event))) {
            Ok(result) => result,
            Err(payload) => {
                self.on_panic(kind, payload.as_ref());
                false
            }
        }
    }

    fn deliver(&'static self, plugin: &mut P, mq: &Mq<P>, event: Event) -> bool {
        match event {
            Event::Pulse => plugin.on_pulse(mq),
            Event::IncomingChat(line, color) => return plugin.on_incoming_chat(mq, &line, color),
            Event::WriteChatColor(line, color) => plugin.on_write_chat_color(mq, &line, color),
            Event::Zoned => plugin.on_zoned(mq),
            Event::CleanUi => plugin.on_clean_ui(mq),
            Event::ReloadUi => plugin.on_reload_ui(mq),
            Event::DrawHud => plugin.on_draw_hud(mq),
            Event::SetGameState(state) => plugin.on_set_game_state(mq, GameState::from(state)),
            Event::AddSpawn(raw) => {
                // SAFETY: MacroQuest passes a live spawn for this call.
                if let Some(spawn) = unsafe { Spawn::from_raw(raw) } {
                    plugin.on_add_spawn(mq, &spawn);
                }
            }
            Event::RemoveSpawn(raw) => {
                if let Some(spawn) = unsafe { Spawn::from_raw(raw) } {
                    plugin.on_remove_spawn(mq, &spawn);
                }
            }
            Event::AddGroundItem(raw) => {
                if let Some(item) = unsafe { raw.as_ref() } {
                    plugin.on_add_ground_item(mq, item);
                }
            }
            Event::RemoveGroundItem(raw) => {
                if let Some(item) = unsafe { raw.as_ref() } {
                    plugin.on_remove_ground_item(mq, item);
                }
            }
            Event::BeginZone => plugin.on_begin_zone(mq),
            Event::EndZone => plugin.on_end_zone(mq),
            Event::UpdateImGui => plugin.on_update_imgui(mq),
            Event::MacroStart(name) => plugin.on_macro_start(mq, &name),
            Event::MacroStop(name) => plugin.on_macro_stop(mq, &name),
            Event::LoadPlugin(name) => plugin.on_load_plugin(mq, &name),
            Event::UnloadPlugin(name) => plugin.on_unload_plugin(mq, &name),
            Event::PostUnloadPlugin(name) => plugin.on_post_unload_plugin(mq, &name),
            Event::Command(id, line) => {
                // Clone the handler out so it may add or remove commands itself.
                let handler = self.commands.borrow().iter().find(|slot| slot.id == id).map(|slot| slot.handler.clone());
                if let Some(handler) = handler {
                    handler(plugin, mq, &line);
                }
            }
        }
        false
    }

    fn on_panic(&self, (bit, name): (u32, &str), payload: &(dyn Any + Send)) {
        let mut stats = self.stats.get();
        stats.panics += 1;
        self.stats.set(stats);

        // Show the message once per callback kind; after that, only count it
        // in the rate-limited warnings, so a callback panicking every frame
        // doesn't flood chat. (A panic shown in full isn't counted again there.)
        let reported = self.reported_panics.get();
        if reported & (1 << bit) != 0 {
            self.warnings.borrow_mut().record_panic();
        } else {
            self.reported_panics.set(reported | (1 << bit));
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("(non-string panic payload)");
            self.diagnostic(&format!("panicked in {name}: {message}"));
        }
    }

    /// Writes one of the runtime's own messages to chat, in red, without it
    /// coming back to this plugin's `on_write_chat_color`.
    fn diagnostic(&self, text: &str) {
        self.writing_diagnostic.set(true);
        (self.host.write_chat)(&format!("[{}] {text}", self.name), CONCOLOR_RED);
        self.writing_diagnostic.set(false);
    }

    fn record_pulse(&self, took: Duration) {
        let mut stats = self.stats.get();
        stats.pulses += 1;
        stats.pulse_time += took;
        stats.slowest_pulse = stats.slowest_pulse.max(took);
        self.stats.set(stats);
        self.warnings.borrow_mut().record_pulse(took);
    }

    fn free_retired(&self) {
        if !self.busy.get() {
            self.retired.borrow_mut().clear();
        }
    }

    // ---------------------------------------------------------------------
    // Used by `Mq`
    // ---------------------------------------------------------------------

    pub(crate) fn stats(&self) -> Stats {
        self.stats.get()
    }

    /// Runs `f` on the plugin between callbacks (tests only).
    #[cfg(test)]
    pub(crate) fn with_plugin<R>(&self, f: impl FnOnce(&mut P) -> R) -> Option<R> {
        assert!(!self.busy.get(), "with_plugin called during a callback");
        // SAFETY: not busy, so nothing else borrows `plugin`.
        unsafe { (*self.plugin.get()).as_mut() }.map(f)
    }

    pub(crate) fn reset_stats(&self) {
        self.stats.set(Stats::new());
        self.reported_panics.set(0);
    }

    pub(crate) fn set_slow_pulse_warning(&self, threshold: Option<Duration>) {
        self.warnings.borrow_mut().slow_threshold = threshold;
    }

    pub(crate) fn add_command(&'static self, name: &str, handler: Rc<CommandHandler<P>>) -> Result<(), CommandError> {
        if !name.starts_with('/') || name.len() < 2 || name.contains(char::is_whitespace) {
            return Err(CommandError::InvalidName);
        }
        if self.commands.borrow().iter().any(|slot| slot.name.eq_ignore_ascii_case(name)) {
            return Err(CommandError::AlreadyRegistered);
        }
        let main = (self.host.main_interface)();
        if main.is_null() {
            return Err(CommandError::Unavailable);
        }

        let id = self.next_command_id.get();
        self.next_command_id.set(id + 1);
        let slot = Rc::new(CommandSlot { id, name: name.to_owned(), handler, runtime: self });
        let user = Rc::as_ptr(&slot).cast_mut().cast::<c_void>();
        // Registered before MacroQuest learns of it, in case it calls back at once.
        self.commands.borrow_mut().push(slot);

        let handle = self.handle.get();
        let added = unsafe {
            shim::mqemu_add_command(main, &handle, name.as_ptr().cast(), name.len(), command_trampoline::<P>, user, false, true, false)
        };
        if added {
            Ok(())
        } else {
            let mut commands = self.commands.borrow_mut();
            let slot = commands.pop().expect("just pushed");
            self.retired.borrow_mut().push(slot);
            Err(CommandError::Rejected)
        }
    }

    pub(crate) fn remove_command(&self, name: &str) -> bool {
        let position = self.commands.borrow().iter().position(|slot| slot.name.eq_ignore_ascii_case(name));
        let Some(position) = position else { return false };
        let slot = self.commands.borrow_mut().remove(position);
        self.unregister(&slot);
        // A handler may be removing its own command while it runs.
        self.retired.borrow_mut().push(slot);
        true
    }

    fn remove_all_commands(&self) {
        let slots: Vec<_> = self.commands.borrow_mut().drain(..).collect();
        for slot in &slots {
            self.unregister(slot);
        }
        self.retired.borrow_mut().extend(slots);
    }

    fn unregister(&self, slot: &CommandSlot<P>) {
        let main = (self.host.main_interface)();
        if !main.is_null() {
            let handle = self.handle.get();
            unsafe { shim::mqemu_remove_command(main, &handle, slot.name.as_ptr().cast(), slot.name.len()) };
        }
    }

    pub(crate) fn do_command(&self, line: &str, delayed: bool) -> bool {
        let main = (self.host.main_interface)();
        if main.is_null() {
            return false;
        }
        let line = to_c(line);
        let handle = self.handle.get();
        unsafe { shim::mqemu_do_command(main, &handle, line.as_ptr(), delayed) }
    }

    pub(crate) fn timed_command(&self, line: &str, delay: Duration) -> bool {
        let main = (self.host.main_interface)();
        if main.is_null() {
            return false;
        }
        let line = to_c(line);
        let handle = self.handle.get();
        let ms = i32::try_from(delay.as_millis()).unwrap_or(i32::MAX);
        unsafe { shim::mqemu_timed_command(main, &handle, line.as_ptr(), ms) }
    }

    pub(crate) fn is_command(&self, name: &str) -> bool {
        let main = (self.host.main_interface)();
        !main.is_null() && unsafe { shim::mqemu_is_command(main, name.as_ptr().cast(), name.len()) }
    }
}

/// Which of the name-carrying callbacks [`Runtime::named`] is handling.
#[derive(Debug, Clone, Copy)]
pub enum NamedEvent {
    MacroStart,
    MacroStop,
    LoadPlugin,
    UnloadPlugin,
    PostUnloadPlugin,
}

/// MacroQuest calls this for every registered command; `user` is the slot.
unsafe extern "C" fn command_trampoline<P: Plugin>(user: *mut c_void, _player: *mut PlayerClient, line: *const c_char) {
    // Copy what we need out of the slot first: the handler may remove the
    // command (retiring the slot) while it runs.
    let (runtime, id) = {
        // SAFETY: `user` is a slot kept alive in `commands` or `retired`.
        let slot = unsafe { &*user.cast::<CommandSlot<P>>() };
        (slot.runtime, slot.id)
    };
    let line = unsafe { from_c(line) }.into_owned();
    guard(|| {
        runtime.enter(Event::Command(id, line));
    });
}

/// Runs `f`, swallowing any panic: runtime code must never unwind into C++.
fn guard(f: impl FnOnce()) {
    let _ = catch_unwind(AssertUnwindSafe(f));
}

fn guard_or<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

#[cfg(test)]
mod tests;
