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
use mq_emu_sys::types::{EQGroundItem, MQPlugin, PlayerClient};

use crate::actors::{MailboxSlot, Message};
use crate::game::{GameState, Spawn};
use crate::host::Host;
use crate::mq::{CommandError, Mq};
use crate::plugin::Plugin;
use crate::stats::{Stats, Warnings};
use crate::tasks::Executor;
use crate::text::{from_c, to_c};

/// Most re-entrant events held at once; more are dropped.
pub const MAX_QUEUE: usize = 256;
/// Deepest chain of queued events: events a callback causes are delivered
/// after it, events *those* cause after them, and so on, up to this many
/// generations; deeper ones are dropped. A callback that keeps causing events
/// for itself (e.g. echoing its own chat) would otherwise never return
/// control to the game. A callback that causes many events at once (a long
/// report in chat) is one generation, limited only by [`MAX_QUEUE`].
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
    /// A message for the mailbox with this id.
    Message(u64, Message),
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
            Event::Message(..) => (24, "a mailbox handler"),
        }
    }

    fn queueable(&self) -> bool {
        !matches!(self, Event::AddSpawn(_) | Event::RemoveSpawn(_) | Event::AddGroundItem(_) | Event::RemoveGroundItem(_))
    }
}

/// Kinds used only by the runtime itself, for once-per-kind panic reports.
const KIND_NEW: (u32, &str) = (21, "Plugin::new");
const KIND_SHUTDOWN: (u32, &str) = (22, "shutdown");
const KIND_TASK: (u32, &str) = (23, "a task");
const KIND_TLO: (u32, &str) = (24, "a TLO");

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
    /// MacroQuest's record for this plugin (null in tests).
    mq_plugin: Cell<*mut MQPlugin>,
    /// Queued events, each with its generation (see `MAX_NESTED_DELIVERIES`).
    queue: RefCell<VecDeque<(Event, usize)>>,
    /// The generation of the event being delivered; 0 for MacroQuest's own call.
    generation: Cell<usize>,
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
    /// True while something holds `&mut P` (a callback or `Task::with_plugin`).
    plugin_in_use: Cell<bool>,
    executor: Executor,
    mailboxes: RefCell<Vec<Rc<MailboxSlot<P>>>>,
    next_mailbox_id: Cell<u64>,
    /// Names of the TLOs this plugin added.
    tlos: RefCell<Vec<String>>,
}

// SAFETY: MacroQuest calls plugins only from the game thread, and plugin code
// can only reach the runtime through `Mq`, which is `!Send`. The runtime is
// never touched from two threads.
unsafe impl<P: Plugin> Sync for Runtime<P> {}

impl<P: Plugin> Runtime<P> {
    /// A runtime that talks to MacroQuest. `name` labels its chat messages.
    pub const fn new(name: &'static str) -> Runtime<P> {
        Runtime::with_host(name, crate::host::real::<P>())
    }

    pub(crate) const fn with_host(name: &'static str, host: Host) -> Runtime<P> {
        Runtime {
            name,
            host,
            plugin: UnsafeCell::new(None),
            busy: Cell::new(false),
            handle: Cell::new(MQPluginHandle { pluginID: 0 }),
            mq_plugin: Cell::new(std::ptr::null_mut()),
            queue: RefCell::new(VecDeque::new()),
            generation: Cell::new(0),
            commands: RefCell::new(Vec::new()),
            retired: RefCell::new(Vec::new()),
            next_command_id: Cell::new(1),
            stats: Cell::new(Stats::new()),
            warnings: RefCell::new(Warnings::new()),
            reported_panics: Cell::new(0),
            writing_diagnostic: Cell::new(false),
            plugin_in_use: Cell::new(false),
            executor: Executor::new(),
            mailboxes: RefCell::new(Vec::new()),
            next_mailbox_id: Cell::new(1),
            tlos: RefCell::new(Vec::new()),
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

    /// Stores what MacroQuest's loader passes to `InitPluginHandle`.
    pub fn init_handle(&'static self, plugin: *mut MQPlugin, handle: MQPluginHandle) {
        self.handle.set(handle);
        self.mq_plugin.set(plugin);
    }

    pub fn initialize(&'static self) {
        guard(|| {
            if self.busy.get() || self.has_plugin() {
                return;
            }
            self.busy.set(true);
            // The default mailbox ("<plugin>") feeds Plugin::on_message and
            // receives replies to requests. Without MacroQuest's messaging
            // (or in tests without it) the plugin simply has no mailbox.
            let _ = self.register_mailbox("", None);
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
            // Nothing is delivered after shutdown, and tasks go before the plugin.
            self.queue.borrow_mut().clear();
            self.executor.clear();
            self.remove_all_commands();
            self.unregister_all_mailboxes();
            self.remove_all_tlos();
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
            self.busy.set(true);
            self.run_one(Event::Pulse);
            // Tasks run in the same busy section, after on_pulse; anything
            // they cause for the plugin is delivered right after them.
            self.executor.poll_all(
                |failure| self.diagnostic(&format!("task failed: {failure}")),
                |payload| self.on_panic(KIND_TASK, payload),
            );
            self.drain();
            self.busy.set(false);
            self.free_retired();
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
    pub(crate) fn enter(&'static self, event: Event) -> bool {
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
        let generation = self.generation.get() + 1;
        let mut stats = self.stats.get();
        if generation > MAX_NESTED_DELIVERIES {
            // Too deep: almost certainly a feedback loop.
            stats.dropped += 1;
            stats.cap_hits += 1;
            self.warnings.borrow_mut().record_dropped(1);
        } else if event.queueable() && self.queue.borrow().len() < MAX_QUEUE {
            self.queue.borrow_mut().push_back((event, generation));
            stats.queued += 1;
        } else {
            stats.dropped += 1;
            self.warnings.borrow_mut().record_dropped(1);
        }
        self.stats.set(stats);
    }

    fn drain(&'static self) {
        loop {
            let next = self.queue.borrow_mut().pop_front();
            let Some((event, generation)) = next else { break };
            self.generation.set(generation);
            self.run_one(event);
        }
        self.generation.set(0);
    }

    /// Runs one event's callback. Must only be called while `busy`.
    fn run_one(&'static self, event: Event) -> bool {
        // SAFETY: the caller set `busy`, so this is the only borrow of `plugin`.
        let Some(plugin) = (unsafe { &mut *self.plugin.get() }).as_mut() else {
            return false;
        };
        let mq = Mq::new(self);
        let kind = event.kind();
        self.plugin_in_use.set(true);
        let outcome = catch_unwind(AssertUnwindSafe(|| self.deliver(plugin, &mq, event)));
        self.plugin_in_use.set(false);
        match outcome {
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
            Event::Message(mailbox, message) => self.dispatch_message(plugin, mq, mailbox, message),
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

    pub(crate) fn executor(&self) -> &Executor {
        &self.executor
    }

    pub(crate) fn handle(&self) -> MQPluginHandle {
        self.handle.get()
    }

    /// The plugin's name as MacroQuest knows it (the DLL name without "MQ2"
    /// and ".dll"), which MacroQuest uses to name and address its mailboxes.
    /// Falls back to the crate name when MacroQuest hasn't said (tests).
    pub(crate) fn mq_name(&self) -> String {
        let name = unsafe { mq_emu_sys::shim::mqemu_plugin_name(self.mq_plugin.get()) };
        if name.is_null() {
            self.name.to_owned()
        } else {
            unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned()
        }
    }

    pub(crate) fn tlos(&self) -> &RefCell<Vec<String>> {
        &self.tlos
    }

    /// Asks a TLO handler for its value, synchronously: MacroQuest needs the
    /// answer now, so unlike events this can't be queued. None (NULL) if a
    /// callback is already running (the plugin is borrowed) or it panics.
    pub(crate) fn call_tlo(&'static self, handler: &crate::data::TloHandler<P>, index: &str) -> Option<crate::data::Value> {
        if self.busy.get() || !self.has_plugin() {
            let mut stats = self.stats.get();
            stats.dropped += 1;
            self.stats.set(stats);
            self.warnings.borrow_mut().record_dropped(1);
            return None;
        }
        self.busy.set(true);
        // SAFETY: busy, so this is the only borrow of `plugin`.
        let plugin = unsafe { (*self.plugin.get()).as_mut() }.expect("checked above");
        let mq = Mq::new(self);
        self.plugin_in_use.set(true);
        let outcome = catch_unwind(AssertUnwindSafe(|| handler(plugin, &mq, index)));
        self.plugin_in_use.set(false);
        let value = match outcome {
            Ok(value) => value,
            Err(payload) => {
                self.on_panic(KIND_TLO, payload.as_ref());
                None
            }
        };
        // Deliver whatever the handler caused (chat it wrote, say).
        self.drain();
        self.busy.set(false);
        self.free_retired();
        value
    }

    pub(crate) fn mailboxes(&self) -> &RefCell<Vec<Rc<MailboxSlot<P>>>> {
        &self.mailboxes
    }

    pub(crate) fn next_mailbox_id(&self) -> u64 {
        let id = self.next_mailbox_id.get();
        self.next_mailbox_id.set(id + 1);
        id
    }

    /// The default mailbox's dropbox, which requests are sent from.
    pub(crate) fn default_dropbox(&self) -> Option<*mut mq_emu_sys::shim::Dropbox> {
        let mailboxes = self.mailboxes.borrow();
        let slot = mailboxes.iter().find(|slot| slot.name.is_empty())?;
        let dropbox = slot.dropbox();
        (!dropbox.is_null()).then_some(dropbox)
    }

    /// Lends the plugin to a task.
    pub(crate) fn with_plugin<R>(&'static self, f: impl FnOnce(&mut P, &Mq<P>) -> R) -> R {
        assert!(
            !self.plugin_in_use.get(),
            "with_plugin called while the plugin is already borrowed; call it from a task, not from a callback"
        );
        // SAFETY: plugin_in_use guards the only other way to reach `plugin`
        // mutably, and everything runs on the game thread.
        let plugin = unsafe { (*self.plugin.get()).as_mut() }.expect("the plugin exists while its tasks run");

        struct Release<'a>(&'a Cell<bool>);
        impl Drop for Release<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        self.plugin_in_use.set(true);
        let _release = Release(&self.plugin_in_use);
        f(plugin, &Mq::new(self))
    }

    /// Runs `f` on the plugin between callbacks (tests only).
    #[cfg(test)]
    pub(crate) fn peek_plugin<R>(&self, f: impl FnOnce(&mut P) -> R) -> Option<R> {
        assert!(!self.busy.get(), "with_plugin called during a callback");
        // SAFETY: not busy, so nothing else borrows `plugin`.
        unsafe { (*self.plugin.get()).as_mut() }.map(f)
    }

    pub(crate) fn reset_stats(&self) {
        self.stats.set(Stats::new());
        self.reported_panics.set(0);
    }

    pub(crate) fn dismiss_pending_warnings(&self) {
        self.warnings.borrow_mut().dismiss();
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
pub(crate) fn guard(f: impl FnOnce()) {
    let _ = catch_unwind(AssertUnwindSafe(f));
}

fn guard_or<T>(fallback: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(fallback)
}

#[cfg(test)]
mod tests;
