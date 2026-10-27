//! Runtime behaviour against a fake host: dispatch, re-entrancy, the feedback
//! cap, panics, commands and warnings. Each test runs on its own thread, so the
//! thread-local fakes isolate tests from each other.

use std::cell::{Cell, RefCell};
use std::ffi::CString;
use std::ptr;
use std::rc::Rc;
use std::time::{Duration, Instant};

use mq_emu_sys::layouts::{EverQuestinfo, PlayerManagerClient};
use mq_emu_sys::shim::MainInterface;
use mq_emu_sys::shim::testing::*;

use super::*;
use crate::game::{Heading, Position, SpawnId};
use crate::host::Host;
use crate::movement::{MoveError, MoveKey};

mod aliases;
mod data;
mod datatypes;
mod events;
mod game_data;
mod imgui;
mod key_binds;
mod plugin_windows;
mod subclass;
mod windows;

// ----- fake host -----------------------------------------------------------

type Hook = Rc<dyn Fn(&mut Recorder, &Mq<Recorder>)>;
/// Stands in for MacroQuest calling OnWriteChatColor back synchronously.
type Echo = Rc<dyn Fn(&str, i32)>;

thread_local! {
    static CHAT: RefCell<Vec<(String, i32)>> = const { RefCell::new(Vec::new()) };
    /// Simulates MacroQuest calling every plugin's OnWriteChatColor synchronously.
    static ECHO: RefCell<Option<Echo>> = const { RefCell::new(None) };
    static MAIN: Cell<*mut MainInterface> = const { Cell::new(ptr::null_mut()) };
    static CLOCK_BASE: Instant = Instant::now();
    static CLOCK: Cell<Duration> = const { Cell::new(Duration::ZERO) };
    static ON_NEW: RefCell<Option<Hook>> = const { RefCell::new(None) };
    static ON_PULSE: RefCell<Option<Hook>> = const { RefCell::new(None) };
    static ON_CHAT: RefCell<Option<Hook>> = const { RefCell::new(None) };
    static ON_IMGUI: RefCell<Option<Hook>> = const { RefCell::new(None) };
    /// Runs in on_clean_ui, on_reload_ui and on_set_game_state.
    static ON_UI: RefCell<Option<Hook>> = const { RefCell::new(None) };
    static GAME_STATE: Cell<i32> = const { Cell::new(mq_emu_sys::constants::GAMESTATE_INGAME) };
    /// The local and controlled player (the same spawn unless "riding").
    static PLAYER: FakeObject<PlayerClient> = FakeObject::new();
    static HAS_PLAYER: Cell<bool> = const { Cell::new(true) };
    static TARGET: FakeObject<PlayerClient> = FakeObject::new();
    static HAS_TARGET: Cell<bool> = const { Cell::new(false) };
    static INFO: FakeObject<EverQuestinfo> = FakeObject::new();
    /// A command the fake game reports as disabled.
    static DISABLED: Cell<Option<u32>> = const { Cell::new(None) };
    /// Every EQExecuteCmd call, in order.
    static EXECUTED: RefCell<Vec<(u32, bool)>> = const { RefCell::new(Vec::new()) };
    static FACE_ANGLE: Cell<f64> = const { Cell::new(mq_emu_sys::constants::FACE_ANGLE_INACTIVE) };
    static MANAGER: FakeObject<PlayerManagerClient> = FakeObject::new();
    static HAS_MANAGER: Cell<bool> = const { Cell::new(true) };
    /// The name MQ2Lua was loaded as, if it is.
    static LUA_LOADED_AS: Cell<Option<&'static str>> = const { Cell::new(None) };
    /// What the fake ImGui shim reports as caught after the next frame.
    static IMGUI_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
    /// What the game-data hosts return; null unless a test points them at its fakes.
    static LOCAL_PC: Cell<*mut mq_emu_sys::layouts::PcClient> = const { Cell::new(ptr::null_mut()) };
    static WINDOWS: Cell<Windows> = const { Cell::new(Windows::NONE) };
    static SPELL_MANAGER: Cell<*mut mq_emu_sys::layouts::ClientSpellManager> = const { Cell::new(ptr::null_mut()) };
    static WORLD_DATA: Cell<*mut mq_emu_sys::layouts::EQWorldData> = const { Cell::new(ptr::null_mut()) };
    static ZONE_INFO: Cell<*mut mq_emu_sys::layouts::zoneHeader> = const { Cell::new(ptr::null_mut()) };
    static GROUND_ITEMS: Cell<*mut mq_emu_sys::layouts::EQGroundItemListManager> = const { Cell::new(ptr::null_mut()) };
    /// The zone's other spawns; kept alive here so the list's pointers stay valid.
    static ZONE: RefCell<Vec<FakeObject<PlayerClient>>> = const { RefCell::new(Vec::new()) };
}

/// Zeroed, aligned storage standing in for a game object.
struct FakeObject<T> {
    storage: RefCell<Vec<u64>>,
    _type: std::marker::PhantomData<T>,
}

impl<T> FakeObject<T> {
    fn new() -> FakeObject<T> {
        FakeObject { storage: RefCell::new(vec![0; size_of::<T>().div_ceil(8)]), _type: std::marker::PhantomData }
    }

    fn ptr(&self) -> *mut T {
        self.storage.borrow_mut().as_mut_ptr().cast()
    }
}

fn fake_execute_cmd(command: u32, down: bool) -> bool {
    EXECUTED.with(|log| log.borrow_mut().push((command, down)));
    // Like the game: the command's state follows its key.
    let info = unsafe { &*INFO.with(FakeObject::ptr) };
    info.set_keyDown_at(command as usize, i32::from(down));
    true
}

fn player() -> &'static PlayerClient {
    unsafe { &*PLAYER.with(FakeObject::ptr) }
}

fn executed() -> Vec<(u32, bool)> {
    EXECUTED.with(|log| log.borrow().clone())
}

fn fake_write_chat(text: &str, color: i32) {
    CHAT.with(|chat| chat.borrow_mut().push((text.to_owned(), color)));
    let echo = ECHO.with(|echo| echo.borrow().clone());
    if let Some(echo) = echo {
        echo(text, color);
    }
}

fn fake_now() -> Instant {
    CLOCK_BASE.with(|base| *base) + CLOCK.with(Cell::get)
}

fn advance_clock(by: Duration) {
    CLOCK.with(|clock| clock.set(clock.get() + by));
}

/// The fake game's windows and managers; all missing unless a test sets them.
#[derive(Clone, Copy)]
struct Windows {
    target_buffs_populated: bool,
    target_window: *mut mq_emu_sys::layouts::CTargetWnd,
    pet_window: *mut mq_emu_sys::layouts::CPetInfoWnd,
    raid: *mut mq_emu_sys::layouts::CRaid,
    mercenary_manager: *mut mq_emu_sys::layouts::CMercenaryManager,
    aggro: *mut mq_emu_sys::layouts::AggroMeterManagerClient,
    merchant_window: *mut mq_emu_sys::layouts::CMerchantWnd,
    loot_window: *mut mq_emu_sys::layouts::CLootWnd,
    active_merchant: *mut PlayerClient,
    active_corpse: *mut PlayerClient,
    window_manager: *mut mq_emu_sys::layouts::CXWndManager,
    context_menu_manager: *mut mq_emu_sys::layouts::CContextMenuManager,
    inv_slot_manager: *mut mq_emu_sys::layouts::CInvSlotMgr,
    trade_window: *mut mq_emu_sys::layouts::CTradeWnd,
    item_display_manager: *mut mq_emu_sys::layouts::CItemDisplayManager,
}

impl Windows {
    const NONE: Windows = Windows {
        target_buffs_populated: false,
        target_window: ptr::null_mut(),
        pet_window: ptr::null_mut(),
        raid: ptr::null_mut(),
        mercenary_manager: ptr::null_mut(),
        aggro: ptr::null_mut(),
        merchant_window: ptr::null_mut(),
        loot_window: ptr::null_mut(),
        active_merchant: ptr::null_mut(),
        active_corpse: ptr::null_mut(),
        window_manager: ptr::null_mut(),
        context_menu_manager: ptr::null_mut(),
        inv_slot_manager: ptr::null_mut(),
        trade_window: ptr::null_mut(),
        item_display_manager: ptr::null_mut(),
    };
}

/// Changes the fake game's windows and managers for this thread.
fn set_windows(f: impl FnOnce(&mut Windows)) {
    WINDOWS.with(|windows| {
        let mut value = windows.get();
        f(&mut value);
        windows.set(value);
    });
}

const FAKE: Host = Host {
    write_chat: fake_write_chat,
    main_interface: || MAIN.with(Cell::get),
    game_state: || GAME_STATE.with(Cell::get),
    now: fake_now,
    local_player: || if HAS_PLAYER.with(Cell::get) { PLAYER.with(FakeObject::ptr) } else { ptr::null_mut() },
    controlled_player: || if HAS_PLAYER.with(Cell::get) { PLAYER.with(FakeObject::ptr) } else { ptr::null_mut() },
    target: || if HAS_TARGET.with(Cell::get) { TARGET.with(FakeObject::ptr) } else { ptr::null_mut() },
    spawn_manager: || if HAS_MANAGER.with(Cell::get) { MANAGER.with(FakeObject::ptr) } else { ptr::null_mut() },
    local_pc: || LOCAL_PC.with(Cell::get),
    spell_manager: || SPELL_MANAGER.with(Cell::get),
    world_data: || WORLD_DATA.with(Cell::get),
    zone_info: || ZONE_INFO.with(Cell::get),
    ground_items: || GROUND_ITEMS.with(Cell::get),
    everquest_info: || INFO.with(FakeObject::ptr),
    target_buffs_populated: || WINDOWS.with(Cell::get).target_buffs_populated,
    target_window: || WINDOWS.with(Cell::get).target_window,
    pet_window: || WINDOWS.with(Cell::get).pet_window,
    raid: || WINDOWS.with(Cell::get).raid,
    mercenary_manager: || WINDOWS.with(Cell::get).mercenary_manager,
    aggro: || WINDOWS.with(Cell::get).aggro,
    merchant_window: || WINDOWS.with(Cell::get).merchant_window,
    loot_window: || WINDOWS.with(Cell::get).loot_window,
    active_merchant: || WINDOWS.with(Cell::get).active_merchant,
    active_corpse: || WINDOWS.with(Cell::get).active_corpse,
    window_manager: || WINDOWS.with(Cell::get).window_manager,
    context_menu_manager: || WINDOWS.with(Cell::get).context_menu_manager,
    inv_slot_manager: || WINDOWS.with(Cell::get).inv_slot_manager,
    trade_window: || WINDOWS.with(Cell::get).trade_window,
    item_display_manager: || WINDOWS.with(Cell::get).item_display_manager,
    spell_display_manager: ptr::null_mut,
    windows: &crate::windows::fake::API,
    game_fns: &crate::game_fns::fake::API,
    command_enabled: |command| DISABLED.with(Cell::get) != Some(command),
    execute_cmd: fake_execute_cmd,
    face_angle: || FACE_ANGLE.with(Cell::get),
    set_face_angle: |value| FACE_ANGLE.with(|angle| angle.set(value)),
    loaded_plugin_name: |name| LUA_LOADED_AS.with(Cell::get).filter(|_| name.eq_ignore_ascii_case("lua")).map(str::to_owned),
    config_dir: || std::path::PathBuf::from(r"C:\MQ\config"),
    data: data::FAKE_DATA,
    take_imgui_error: || IMGUI_ERROR.with(RefCell::take),
    // Recorder never draws; nothing calls these.
    imgui: &crate::imgui::fake::APIS,
    print_var: fake_print_var,
    key_binds: crate::key_binds::fake::API,
};

/// Expands `|${Me.Name}|` to "Tester"; anything else to itself.
unsafe extern "system" fn fake_print_var(name: *mut std::ffi::c_char, value: *mut std::ffi::c_char, cap: usize) -> u32 {
    let name = unsafe { std::ffi::CStr::from_ptr(name) }.to_bytes();
    let expanded: &[u8] = if name == b"${Me.Name}" { b"Tester" } else { name };
    let len = expanded.len().min(cap - 1);
    unsafe {
        ptr::copy_nonoverlapping(expanded.as_ptr().cast(), value, len);
        *value.add(len) = 0;
    }
    len as u32
}


fn chat_lines() -> Vec<String> {
    CHAT.with(|chat| chat.borrow().iter().map(|(text, _)| text.clone()).collect())
}

/// A leaked runtime wired to the fake host, with chat echoed back to it.
fn runtime() -> &'static Runtime<Recorder> {
    let rt: &'static Runtime<Recorder> = Box::leak(Box::new(Runtime::with_host("test", FAKE)));
    ECHO.with(|echo| {
        *echo.borrow_mut() = Some(Rc::new(move |text: &str, color: i32| {
            let text = CString::new(text).unwrap();
            unsafe { rt.write_chat_color(text.as_ptr(), color as u32) };
        }))
    });
    rt
}

fn set_hook(slot: &'static std::thread::LocalKey<RefCell<Option<Hook>>>, hook: impl Fn(&mut Recorder, &Mq<Recorder>) + 'static) {
    slot.with(|s| *s.borrow_mut() = Some(Rc::new(hook)));
}

fn run_hook(slot: &'static std::thread::LocalKey<RefCell<Option<Hook>>>, plugin: &mut Recorder, mq: &Mq<Recorder>) {
    let hook = slot.with(|s| s.borrow().clone());
    if let Some(hook) = hook {
        hook(plugin, mq);
    }
}

/// Owns a fake MainInterface installed as the host's for this thread.
struct FakeMain(*mut MainInterface);

impl FakeMain {
    fn install() -> FakeMain {
        let fake = unsafe { mqemu_test_fake_new() };
        MAIN.with(|main| main.set(fake));
        FakeMain(fake)
    }

    fn invoke(&self, name: &str, line: &str) -> bool {
        let (name, line) = (CString::new(name).unwrap(), CString::new(line).unwrap());
        unsafe { mqemu_test_fake_invoke(self.0, name.as_ptr(), ptr::null_mut(), line.as_ptr()) }
    }

    fn log(&self) -> FakeLog {
        unsafe { *mqemu_test_fake_log(self.0) }
    }
}

impl Drop for FakeMain {
    fn drop(&mut self) {
        MAIN.with(|main| main.set(ptr::null_mut()));
        unsafe { mqemu_test_fake_free(self.0) }
    }
}

// ----- test plugin -----------------------------------------------------------

#[derive(Default)]
struct Recorder {
    log: Vec<String>,
    /// A task handle kept by the plugin, to misuse from a callback.
    saved_task: Option<crate::tasks::Task<Recorder>>,
    /// A message kept to reply to later.
    kept_message: Option<crate::actors::Message>,
}

impl Plugin for Recorder {
    fn new(mq: &Mq<Self>) -> Self {
        let mut plugin = Recorder::default();
        plugin.log.push("new".into());
        run_hook(&ON_NEW, &mut plugin, mq);
        plugin
    }

    fn shutdown(&mut self, _mq: &Mq<Self>) {
        self.log.push("shutdown".into());
    }

    fn on_pulse(&mut self, mq: &Mq<Self>) {
        self.log.push("pulse".into());
        run_hook(&ON_PULSE, self, mq);
    }

    fn on_incoming_chat(&mut self, _mq: &Mq<Self>, line: &str, _color: u32) -> bool {
        self.log.push(format!("incoming:{line}"));
        line.contains("secret")
    }

    fn on_write_chat_color(&mut self, mq: &Mq<Self>, line: &str, _color: u32) {
        self.log.push(format!("chat:{line}"));
        run_hook(&ON_CHAT, self, mq);
    }

    /// Doesn't draw: these tests have no ImGui context (see src/imgui/tests.rs).
    fn on_update_imgui(&mut self, mq: &Mq<Self>, _ui: &mut crate::imgui::Ui) {
        self.log.push("imgui".into());
        run_hook(&ON_IMGUI, self, mq);
    }

    fn on_clean_ui(&mut self, mq: &Mq<Self>) {
        self.log.push("clean_ui".into());
        run_hook(&ON_UI, self, mq);
    }

    fn on_reload_ui(&mut self, mq: &Mq<Self>) {
        self.log.push("reload_ui".into());
        run_hook(&ON_UI, self, mq);
    }

    fn on_set_game_state(&mut self, mq: &Mq<Self>, state: crate::GameState) {
        self.log.push(format!("game_state:{state:?}"));
        run_hook(&ON_UI, self, mq);
    }

    fn on_add_spawn(&mut self, _mq: &Mq<Self>, spawn: &Spawn<'_>) {
        self.log.push(format!("spawn:{}", spawn.id()));
    }

    fn on_message(&mut self, mq: &Mq<Self>, message: crate::actors::Message) {
        let from = message.sender().and_then(|a| a.character);
        self.log.push(format!("msg:{} from {from:?}", message.text()));
        if message.text() == "ping" {
            mq.reply(&message, "pong", 0).unwrap();
        }
        if message.text() == "panic" {
            panic!("bad message");
        }
    }
}

fn log_of(rt: &Runtime<Recorder>) -> Vec<String> {
    rt.peek_plugin(|p| p.log.clone()).unwrap_or_default()
}

// ----- lifecycle -------------------------------------------------------------

#[test]
fn callbacks_reach_the_plugin_and_pulses_are_counted() {
    let rt = runtime();
    rt.initialize();
    rt.pulse();
    rt.pulse();
    rt.zoned();
    assert_eq!(log_of(rt), ["new", "pulse", "pulse"]);
    assert_eq!(rt.stats().pulses, 2);
}

#[test]
fn nothing_is_delivered_before_initialize_or_after_shutdown() {
    let rt = runtime();
    rt.pulse();
    assert!(rt.peek_plugin(|_| ()).is_none());

    rt.initialize();
    rt.shutdown();
    assert!(rt.peek_plugin(|_| ()).is_none(), "plugin dropped on shutdown");
    rt.pulse();
    assert!(rt.peek_plugin(|_| ()).is_none());
}

#[test]
fn initialize_twice_keeps_the_first_plugin() {
    let rt = runtime();
    rt.initialize();
    rt.pulse();
    rt.initialize();
    assert_eq!(log_of(rt), ["new", "pulse"]);
}

// ----- re-entrancy -------------------------------------------------------------

#[test]
fn reentrant_event_is_delivered_right_after_the_callback() {
    let rt = runtime();
    set_hook(&ON_PULSE, |plugin, mq| {
        mq.chat("hi");
        plugin.log.push("pulse:end".into());
    });
    rt.initialize();
    rt.pulse();

    // The chat callback waited for on_pulse to finish, then ran in the same call.
    assert_eq!(log_of(rt), ["new", "pulse", "pulse:end", "chat:hi"]);
    assert_eq!(rt.stats().queued, 1);
    assert_eq!(rt.stats().dropped, 0);
}

#[test]
fn events_outside_a_callback_are_not_queued() {
    let rt = runtime();
    rt.initialize();
    let line = CString::new("direct").unwrap();
    unsafe { rt.write_chat_color(line.as_ptr(), 0) };
    assert_eq!(log_of(rt), ["new", "chat:direct"]);
    assert_eq!(rt.stats().queued, 0);
}

#[test]
fn feedback_loop_is_capped_and_counted() {
    let rt = runtime();
    // Echoing our own chat would loop forever without the cap.
    set_hook(&ON_PULSE, |_, mq| mq.chat("ping"));
    set_hook(&ON_CHAT, |_, mq| mq.chat("ping"));
    rt.initialize();
    rt.pulse();

    let chats = log_of(rt).iter().filter(|e| e.starts_with("chat:")).count();
    assert_eq!(chats, MAX_NESTED_DELIVERIES);
    let stats = rt.stats();
    assert_eq!(stats.cap_hits, 1);
    assert!(stats.dropped >= 1, "{stats:?}");

    // The runtime recovers: the next pulse is delivered normally.
    ON_CHAT.with(|hook| *hook.borrow_mut() = None);
    ON_PULSE.with(|hook| *hook.borrow_mut() = None);
    rt.pulse();
    assert_eq!(log_of(rt).last().unwrap(), "pulse");
}

#[test]
fn a_wide_burst_is_delivered_in_full() {
    // A long report written in one callback is one generation of events, not
    // a loop: every line comes back.
    let rt = runtime();
    set_hook(&ON_PULSE, |_, mq| {
        for n in 0..200 {
            mq.chat(format!("line {n}"));
        }
    });
    rt.initialize();
    rt.pulse();
    let chats = log_of(rt).iter().filter(|e| e.starts_with("chat:")).count();
    assert_eq!(chats, 200);
    let stats = rt.stats();
    assert_eq!((stats.queued, stats.dropped, stats.cap_hits), (200, 0, 0));
}

#[test]
fn more_than_the_queue_holds_drops_only_the_excess() {
    let rt = runtime();
    set_hook(&ON_PULSE, |_, mq| {
        for n in 0..MAX_QUEUE + 10 {
            mq.chat(format!("line {n}"));
        }
    });
    rt.initialize();
    rt.pulse();
    let chats: Vec<_> = log_of(rt).into_iter().filter(|e| e.starts_with("chat:")).collect();
    assert_eq!(chats.len(), MAX_QUEUE);
    assert_eq!(chats.last().unwrap(), &format!("chat:line {}", MAX_QUEUE - 1), "the first ones are kept, in order");
    let stats = rt.stats();
    assert_eq!((stats.dropped, stats.cap_hits), (10, 0));
}

#[test]
fn a_loop_that_fans_out_still_ends() {
    // Each line causes two more: the queue limit and the depth limit both stop it.
    let rt = runtime();
    set_hook(&ON_PULSE, |_, mq| mq.chat("seed"));
    set_hook(&ON_CHAT, |_, mq| {
        mq.chat("a");
        mq.chat("b");
    });
    rt.initialize();
    rt.pulse();
    let stats = rt.stats();
    assert!(stats.dropped > 0, "{stats:?}");
    // The runtime recovers: the next pulse is delivered normally.
    ON_CHAT.with(|hook| *hook.borrow_mut() = None);
    ON_PULSE.with(|hook| *hook.borrow_mut() = None);
    rt.pulse();
    assert_eq!(log_of(rt).last().unwrap(), "pulse");
}

#[test]
fn spawn_events_during_a_callback_are_dropped_not_queued() {
    let rt = runtime();
    let mut storage = vec![0u64; size_of::<PlayerClient>().div_ceil(8)];
    let spawn = storage.as_mut_ptr().cast::<PlayerClient>();
    unsafe { (*spawn).set_SpawnID(9) };

    set_hook(&ON_PULSE, move |_, _| {
        // MacroQuest adding a spawn while on_pulse runs: the pointer might be
        // stale by the time a queued event ran, so it must be dropped.
        let rt: &Runtime<Recorder> = RUNTIME_FOR_SPAWN.with(Cell::get).unwrap();
        rt.add_spawn(spawn);
    });
    RUNTIME_FOR_SPAWN.with(|cell| cell.set(Some(rt)));
    rt.initialize();
    rt.pulse();
    assert_eq!(log_of(rt), ["new", "pulse"]);
    assert_eq!(rt.stats().dropped, 1);

    // Outside a callback the same event is delivered.
    rt.add_spawn(spawn);
    assert_eq!(log_of(rt).last().unwrap(), "spawn:9");
}

thread_local! {
    static RUNTIME_FOR_SPAWN: Cell<Option<&'static Runtime<Recorder>>> = const { Cell::new(None) };
}

// ----- panics -------------------------------------------------------------

#[test]
fn panics_are_caught_reported_once_and_counted() {
    let rt = runtime();
    set_hook(&ON_PULSE, |_, _| panic!("boom"));
    rt.initialize();
    rt.pulse();
    rt.pulse();

    let reports: Vec<_> = chat_lines().into_iter().filter(|l| l.contains("panicked")).collect();
    assert_eq!(reports, ["[test] panicked in on_pulse: boom"]);
    assert_eq!(rt.stats().panics, 2);

    // Not stuck busy: other callbacks still work.
    let line = CString::new("after").unwrap();
    unsafe { rt.write_chat_color(line.as_ptr(), 0) };
    assert!(log_of(rt).contains(&"chat:after".to_owned()));
}

#[test]
fn a_reported_panic_is_not_repeated_as_a_warning() {
    // Seen in game: "panicked in a command handler: ..." followed by "warning: 1 panic".
    let rt = runtime();
    let fail_once = Cell::new(true);
    let fail_once = Rc::new(fail_once);
    let flag = fail_once.clone();
    set_hook(&ON_PULSE, move |_, _| {
        if flag.replace(false) {
            panic!("once");
        }
    });
    rt.initialize();
    rt.pulse();
    rt.pulse();
    let lines = chat_lines();
    assert_eq!(lines, ["[test] panicked in on_pulse: once"], "{lines:?}");

    // A second, silenced panic of the same kind does show up in a warning.
    fail_once.set(true);
    advance_clock(WARNING_INTERVAL_FOR_TESTS);
    rt.pulse();
    assert_eq!(chat_lines().last().unwrap(), "[test] warning: 1 panic");
}

const WARNING_INTERVAL_FOR_TESTS: Duration = crate::stats::WARNING_INTERVAL;

#[test]
fn panic_in_new_leaves_no_plugin() {
    let rt = runtime();
    set_hook(&ON_NEW, |_, _| panic!("bad config"));
    rt.initialize();
    assert!(rt.peek_plugin(|_| ()).is_none());
    assert!(chat_lines().contains(&"[test] panicked in Plugin::new: bad config".to_owned()));
    rt.pulse(); // ignored, not a crash
}

// ----- chat input -------------------------------------------------------------

#[test]
fn incoming_chat_is_lossy_and_can_be_suppressed() {
    let rt = runtime();
    rt.initialize();
    let secret = b"secret caf\xe9\0";
    assert!(unsafe { rt.incoming_chat(secret.as_ptr().cast(), 0) });
    let plain = CString::new("hello").unwrap();
    assert!(!unsafe { rt.incoming_chat(plain.as_ptr(), 0) });
    assert!(!unsafe { rt.incoming_chat(ptr::null(), 0) });
    assert_eq!(log_of(rt), ["new", "incoming:secret caf\u{fffd}", "incoming:hello", "incoming:"]);
}

// ----- commands -------------------------------------------------------------

fn register_echo_command(name: &'static str) {
    set_hook(&ON_NEW, move |_, mq| {
        mq.add_command(name, |plugin: &mut Recorder, _mq, args| plugin.log.push(format!("cmd:{args}"))).unwrap();
    });
}

#[test]
fn commands_route_to_their_closure_with_the_plugin_handle() {
    let main = FakeMain::install();
    let rt = runtime();
    rt.init_handle(ptr::null_mut(), MQPluginHandle { pluginID: 0x42 });
    register_echo_command("/rt");
    rt.initialize();

    let log = main.log();
    assert_eq!((log.add_count, log.last_handle, log.last_parse), (1, 0x42, true));
    assert!(main.invoke("/rt", "stats now"));
    assert_eq!(log_of(rt), ["new", "cmd:stats now"]);
}

#[test]
fn command_invoked_during_a_callback_is_queued() {
    let main = FakeMain::install();
    let rt = runtime();
    register_echo_command("/rt");
    let fake = main.0;
    set_hook(&ON_PULSE, move |plugin, _| {
        let (name, line) = (CString::new("/rt").unwrap(), CString::new("inner").unwrap());
        unsafe { mqemu_test_fake_invoke(fake, name.as_ptr(), ptr::null_mut(), line.as_ptr()) };
        plugin.log.push("pulse:end".into());
    });
    rt.initialize();
    rt.pulse();
    assert_eq!(log_of(rt), ["new", "pulse", "pulse:end", "cmd:inner"]);
}

#[test]
fn a_handler_can_remove_its_own_command() {
    let main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.add_command("/once", |plugin: &mut Recorder, mq, _| {
            plugin.log.push(format!("removed:{}", mq.remove_command("/once")));
        })
        .unwrap();
    });
    rt.initialize();
    assert!(main.invoke("/once", ""));
    assert!(!main.invoke("/once", ""), "gone from MacroQuest");
    assert_eq!(log_of(rt), ["new", "removed:true"]);
}

#[test]
fn shutdown_removes_every_command() {
    let main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.add_command("/a", |_: &mut Recorder, _, _| {}).unwrap();
        mq.add_command("/b", |_: &mut Recorder, _, _| {}).unwrap();
    });
    rt.initialize();
    rt.shutdown();
    assert_eq!(main.log().remove_count, 2);
    assert!(!main.invoke("/a", ""));
    assert!(!main.invoke("/b", ""));
}

#[test]
fn bad_command_registrations_are_rejected() {
    let main = FakeMain::install();
    // Another plugin already owns /taken.
    let other = MQPluginHandle { pluginID: 7 };
    unsafe extern "C" fn noop(_: *mut std::ffi::c_void, _: *mut PlayerClient, _: *const c_char) {}
    assert!(unsafe { shim::mqemu_add_command(main.0, &other, "/taken".as_ptr().cast(), 6, noop, ptr::null_mut(), false, true, false) });

    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| {
        let mut results = Vec::new();
        for name in ["rt", "/", "/a b", "/ok", "/OK", "/taken"] {
            results.push(format!("{name}={:?}", mq.add_command(name, |_: &mut Recorder, _, _| {})));
        }
        plugin.log.extend(results);
    });
    rt.initialize();
    assert_eq!(
        log_of(rt),
        [
            "new",
            "rt=Err(InvalidName)",
            "/=Err(InvalidName)",
            "/a b=Err(InvalidName)",
            "/ok=Ok(())",
            "/OK=Err(AlreadyRegistered)",
            "/taken=Err(Rejected)",
        ]
    );
}

#[test]
fn commands_are_unavailable_without_macroquest() {
    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| {
        plugin.log.push(format!("{:?}", mq.add_command("/rt", |_: &mut Recorder, _, _| {})));
        plugin.log.push(format!("{}", mq.command("/echo hi")));
    });
    rt.initialize();
    assert_eq!(log_of(rt), ["new", "Err(Unavailable)", "false"]);
}

#[test]
fn do_and_timed_commands_reach_macroquest() {
    let main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_PULSE, |_, mq| {
        assert!(mq.command("/echo later"));
        assert!(mq.timed_command("/echo timed", Duration::from_millis(1500)));
    });
    rt.initialize();
    rt.pulse();
    let log = main.log();
    assert_eq!((log.do_count, log.last_delayed, log.timed_count, log.last_ms), (1, true, 1, 1500));
}

// ----- warnings -------------------------------------------------------------

#[test]
fn slow_pulses_warn_at_most_every_interval() {
    let rt = runtime();
    set_hook(&ON_PULSE, |_, _| advance_clock(Duration::from_millis(20)));
    rt.initialize();

    rt.pulse();
    rt.pulse();
    let warnings = || chat_lines().into_iter().filter(|l| l.contains("warning")).collect::<Vec<_>>();
    assert_eq!(warnings(), ["[test] warning: 1 slow pulse (worst 20.0 ms)"]);

    advance_clock(Duration::from_secs(30));
    rt.pulse();
    assert_eq!(warnings().len(), 2);
    assert_eq!(warnings()[1], "[test] warning: 2 slow pulses (worst 20.0 ms)");
    assert_eq!(rt.stats().slowest_pulse, Duration::from_millis(20));
}

#[test]
fn dismissed_problems_are_not_warned_about() {
    let rt = runtime();
    let slow = Rc::new(Cell::new(true));
    let flag = slow.clone();
    set_hook(&ON_PULSE, move |_, mq| {
        if flag.get() {
            advance_clock(Duration::from_millis(20));
        } else {
            mq.dismiss_pending_warnings();
        }
    });
    rt.initialize();
    rt.pulse(); // warned at once
    rt.pulse(); // pending, held back by the rate limit
    slow.set(false);
    rt.pulse(); // dismissed
    advance_clock(Duration::from_secs(30));
    rt.pulse();
    let warnings = || chat_lines().into_iter().filter(|l| l.contains("warning")).collect::<Vec<_>>();
    assert_eq!(warnings(), ["[test] warning: 1 slow pulse (worst 20.0 ms)"]);
    assert_eq!(rt.stats().pulses, 4, "stats keep counting");

    // Negative: problems after a dismissal are still reported.
    slow.set(true);
    rt.pulse();
    assert_eq!(warnings().len(), 2, "{:?}", warnings());
}

#[test]
fn fast_pulses_are_silent() {
    let rt = runtime();
    rt.initialize();
    for _ in 0..100 {
        rt.pulse();
    }
    assert!(chat_lines().is_empty(), "{:?}", chat_lines());
}

// ----- linking -------------------------------------------------------------

#[test]
fn unit_tests_do_not_import_macroquest() {
    // Everything reachable from these tests goes through the fake host, so the
    // test binary must start without MQ2Main.dll or eqlib.dll.
    use object::read::pe::PeFile32;
    let exe = std::fs::read(std::env::current_exe().unwrap()).unwrap();
    let file = PeFile32::parse(&*exe).unwrap();
    let imports = file.import_table().unwrap().unwrap();
    let mut descriptors = imports.descriptors().unwrap();
    while let Some(descriptor) = descriptors.next().unwrap() {
        let name = imports.name(descriptor.name.get(object::LittleEndian)).unwrap();
        let name = String::from_utf8_lossy(name).to_ascii_lowercase();
        assert!(name != "mq2main.dll" && name != "eqlib.dll", "test binary imports {name}");
    }
}

// ----- game state and movement -------------------------------------------------

/// Runs `f` with an `Mq` inside an `on_pulse`, the way plugin code would.
fn in_pulse(f: impl Fn(&mut Recorder, &Mq<Recorder>) + 'static) -> &'static Runtime<Recorder> {
    let rt = runtime();
    set_hook(&ON_PULSE, f);
    rt.initialize();
    rt.pulse();
    rt
}

#[test]
fn me_and_target_come_from_the_game() {
    player().set_SpawnID(1);
    unsafe { (*TARGET.with(FakeObject::ptr)).set_SpawnID(2) };
    let rt = in_pulse(|plugin, mq| {
        plugin.log.push(format!("me:{:?} target:{:?}", mq.me().map(|s| s.id().0), mq.target().map(|s| s.id().0)));
        HAS_TARGET.with(|t| t.set(true));
        plugin.log.push(format!("target:{:?}", mq.target().map(|s| s.id().0)));
    });
    assert_eq!(log_of(rt)[2..], ["me:Some(1) target:None", "target:Some(2)"]);
}

#[test]
fn keys_are_held_and_released_through_the_game() {
    let rt = in_pulse(|plugin, mq| {
        mq.set_key(MoveKey::Forward, true).unwrap();
        plugin.log.push(format!("held:{}", mq.is_key_down(MoveKey::Forward)));
        mq.set_key(MoveKey::Forward, false).unwrap();
        plugin.log.push(format!("held:{}", mq.is_key_down(MoveKey::Forward)));
        mq.tap_key(MoveKey::Jump).unwrap();
    });
    assert_eq!(log_of(rt)[2..], ["held:true", "held:false"]);
    assert_eq!(executed(), [(2, true), (2, false), (1, true), (1, false)]);
}

#[test]
fn nothing_is_sent_to_the_game_outside_the_in_game_state() {
    // Game commands outside the in-game state crash the client.
    GAME_STATE.with(|state| state.set(mq_emu_sys::constants::GAMESTATE_CHARSELECT));
    let rt = in_pulse(|plugin, mq| {
        plugin.log.push(format!("{:?}", mq.set_key(MoveKey::Forward, true)));
        plugin.log.push(format!("{:?}", mq.tap_key(MoveKey::Jump)));
        plugin.log.push(format!("{:?}", mq.stop_moving()));
        plugin.log.push(format!("{:?}", mq.face(Heading::EAST)));
        plugin.log.push(format!("{:?}", mq.face_now(Heading::EAST)));
    });
    assert_eq!(log_of(rt)[2..], ["Err(NotInGame)"; 5]);
    assert!(executed().is_empty());
    assert_eq!(FACE_ANGLE.with(Cell::get), mq_emu_sys::constants::FACE_ANGLE_INACTIVE);
}

#[test]
fn stop_moving_releases_held_keys_and_stops_turning() {
    in_pulse(|_, mq| {
        mq.set_key(MoveKey::Forward, true).unwrap();
        mq.set_key(MoveKey::StrafeLeft, true).unwrap();
        mq.face(Heading::SOUTH).unwrap();
        mq.stop_moving().unwrap();
        for key in MoveKey::HELD {
            assert!(!mq.is_key_down(key), "{key:?} still held");
        }
        assert!(!mq.is_turning());
    });
    // Only the keys that were down were released.
    assert_eq!(executed(), [(2, true), (6, true), (2, false), (6, false)]);
}

#[test]
fn face_point_turns_smoothly_towards_it() {
    player().set_X(0.0);
    player().set_Y(0.0);
    let rt = in_pulse(|plugin, mq| {
        // +X is west in EverQuest.
        mq.face_point(&Position { x: 50.0, y: 0.0, z: 0.0 }).unwrap();
        plugin.log.push(format!("turning:{} to:{:?}", mq.is_turning(), mq.turn_target().map(|h| h.units())));
        mq.stop_turning();
        plugin.log.push(format!("turning:{}", mq.is_turning()));
    });
    assert_eq!(log_of(rt)[2..], ["turning:true to:Some(128.0)", "turning:false"]);
}

#[test]
fn face_now_sets_the_heading_and_cancels_a_smooth_turn() {
    in_pulse(|_, mq| {
        mq.face(Heading::SOUTH).unwrap();
        mq.face_now(Heading::EAST).unwrap();
        assert!(!mq.is_turning());
        assert_eq!(mq.controlled().unwrap().heading(), Heading::EAST);
    });
    assert_eq!(player().Heading(), 384.0);
}

#[test]
fn turning_needs_a_controlled_spawn() {
    HAS_PLAYER.with(|p| p.set(false));
    let rt = in_pulse(|plugin, mq| {
        plugin.log.push(format!("{:?}", mq.face(Heading::EAST)));
        plugin.log.push(format!("{:?}", mq.face_point(&Position::default())));
        plugin.log.push(format!("{:?}", mq.face_now(Heading::EAST)));
    });
    assert_eq!(log_of(rt)[2..], ["Err(NoPlayer)"; 3]);
    assert_eq!(FACE_ANGLE.with(Cell::get), mq_emu_sys::constants::FACE_ANGLE_INACTIVE);
}

#[test]
fn disabled_commands_are_not_pressed_but_can_be_released() {
    DISABLED.with(|d| d.set(Some(MoveKey::Forward.command())));
    let rt = in_pulse(|plugin, mq| {
        plugin.log.push(format!("{:?}", mq.set_key(MoveKey::Forward, true)));
        plugin.log.push(format!("{:?}", mq.set_key(MoveKey::Forward, false)));
        plugin.log.push(format!("{:?}", mq.set_key(MoveKey::Back, true)));
    });
    assert_eq!(log_of(rt)[2..], ["Err(Disabled)", "Ok(())", "Ok(())"]);
    // Forward was never pressed; its release and Back went through.
    assert_eq!(executed(), [(2, false), (3, true)]);
}

// ----- spawn list -------------------------------------------------------------

/// Fills the fake zone: you (the fake player) first, then `others` as
/// (id, shown name, type, x, y), linked the way eqlib links them.
fn set_zone(me: (u32, f32, f32), others: &[(u32, &str, u8, f32, f32)]) {
    player().set_SpawnID(me.0);
    player().set_X(me.1);
    player().set_Y(me.2);
    let mut nodes = vec![PLAYER.with(FakeObject::ptr)];
    ZONE.with(|zone| {
        let mut zone = zone.borrow_mut();
        zone.clear();
        for (id, name, kind, x, y) in others {
            let spawn = FakeObject::<PlayerClient>::new();
            let raw = unsafe { &*spawn.ptr() };
            raw.set_SpawnID(*id);
            raw.set_Type(*kind);
            raw.set_X(*x);
            raw.set_Y(*y);
            let mut shown = raw.DisplayedName();
            for (d, c) in shown.iter_mut().zip(name.bytes().chain([0])) {
                *d = c as c_char;
            }
            raw.set_DisplayedName(shown);
            nodes.push(spawn.ptr());
            zone.push(spawn);
        }
    });
    for pair in nodes.windows(2) {
        unsafe {
            (*pair[0]).set_m_pNext(pair[1]);
            (*pair[1]).set_m_pPrev(pair[0]);
        }
    }
    let last = *nodes.last().unwrap();
    unsafe { (*last).set_m_pNext(ptr::null_mut()) };
    let list = unsafe { (*MANAGER.with(FakeObject::ptr)).PlayerList() };
    list.set_m_pFirstNode(nodes[0]);
    list.set_m_pLastNode(last);
}

const ZONE_SPAWNS: &[(u32, &str, u8, f32, f32)] = &[
    (10, "a rat", 1, 30.0, 0.0),
    (11, "Soandso", 0, 5.0, 5.0),
    (12, "a rat", 1, 8.0, 0.0),
    (13, "a rat's corpse", 2, 1.0, 0.0),
];

#[test]
fn spawns_walks_the_whole_list_in_order() {
    set_zone((1, 0.0, 0.0), ZONE_SPAWNS);
    let rt = in_pulse(|plugin, mq| {
        let ids: Vec<u32> = mq.spawns().map(|s| s.id().0).collect();
        plugin.log.push(format!("{ids:?}"));
    });
    assert_eq!(log_of(rt)[2..], ["[1, 10, 11, 12, 13]"]);
}

#[test]
fn missing_or_empty_lists_have_no_spawns() {
    HAS_MANAGER.with(|m| m.set(false));
    let rt = in_pulse(|plugin, mq| plugin.log.push(format!("{}", mq.spawns().count())));
    assert_eq!(log_of(rt)[2..], ["0"]);

    HAS_MANAGER.with(|m| m.set(true));
    let rt = in_pulse(|plugin, mq| plugin.log.push(format!("{}", mq.spawns().count())));
    assert_eq!(log_of(rt)[2..], ["0"], "a zeroed manager has an empty list");
}

#[test]
fn spawns_are_found_by_id_and_name() {
    set_zone((1, 0.0, 0.0), ZONE_SPAWNS);
    let rt = in_pulse(|plugin, mq| {
        plugin.log.push(format!("{:?}", mq.spawn(SpawnId(12)).map(|s| s.name())));
        plugin.log.push(format!("{:?}", mq.spawn(SpawnId(99)).map(|s| s.name())));
        plugin.log.push(format!("{:?}", mq.spawn_by_name("SOANDSO").map(|s| s.id().0)));
        // The first match wins.
        plugin.log.push(format!("{:?}", mq.spawn_by_name("a rat").map(|s| s.id().0)));
        plugin.log.push(format!("{:?}", mq.spawn_by_name("nobody").map(|s| s.id().0)));
    });
    assert_eq!(log_of(rt)[2..], [r#"Some("a rat")"#, "None", "Some(11)", "Some(10)", "None"]);
}

#[test]
fn nearest_excludes_you_and_honours_the_filter() {
    set_zone((1, 0.0, 0.0), ZONE_SPAWNS);
    let rt = in_pulse(|plugin, mq| {
        plugin.log.push(format!("{:?}", mq.nearest(|_| true).map(|s| s.id().0)));
        plugin.log.push(format!("{:?}", mq.nearest(|s| s.is_npc()).map(|s| s.id().0)));
        plugin.log.push(format!("{:?}", mq.nearest(|s| s.is_player()).map(|s| s.id().0)));
        plugin.log.push(format!("{:?}", mq.nearest(|s| s.level() > 100).map(|s| s.id().0)));
        let far_east = Position { x: 40.0, y: 0.0, z: 0.0 };
        plugin.log.push(format!("{:?}", mq.nearest_to(&far_east, |s| s.is_npc()).map(|s| s.id().0)));
    });
    // Corpse 13 is closest overall; rat 12 beats rat 10; you (1) never count.
    assert_eq!(log_of(rt)[2..], ["Some(13)", "Some(12)", "Some(11)", "None", "Some(10)"]);
}

#[test]
fn nearest_needs_a_controlled_spawn() {
    set_zone((1, 0.0, 0.0), ZONE_SPAWNS);
    HAS_PLAYER.with(|p| p.set(false));
    let rt = in_pulse(|plugin, mq| plugin.log.push(format!("{:?}", mq.nearest(|_| true).map(|s| s.id().0))));
    assert_eq!(log_of(rt)[2..], ["None"]);
}

#[test]
fn a_cyclic_list_stops_at_the_cap() {
    // A damaged list must not hang the game.
    set_zone((1, 0.0, 0.0), &ZONE_SPAWNS[..1]);
    let rat = ZONE.with(|zone| zone.borrow()[0].ptr());
    unsafe { (*rat).set_m_pNext(PLAYER.with(FakeObject::ptr)) };
    let rt = in_pulse(|plugin, mq| plugin.log.push(format!("{}", mq.spawns().count())));
    assert_eq!(log_of(rt)[2..], [crate::spawns::MAX_SPAWNS.to_string()]);
}

// ----- async tasks ----------------------------------------------------------------

use crate::tasks::{Task, TaskId};

fn note(task: &Task<Recorder>, text: &str) {
    task.with_plugin(|plugin, _| plugin.log.push(text.to_owned()));
}

/// Starts `make` as a task from `Plugin::new`.
fn with_task<Fut>(make: impl Fn(Task<Recorder>) -> Fut + 'static) -> &'static Runtime<Recorder>
where
    Fut: std::future::Future<Output = ()> + 'static,
{
    let make = Rc::new(make);
    let rt = runtime();
    set_hook(&ON_NEW, move |_, mq| {
        let make = make.clone();
        mq.spawn_task(move |task| make(task));
    });
    rt.initialize();
    rt.with_plugin(|plugin, _| plugin.log.clear());
    rt
}

#[test]
fn tasks_resume_once_per_pulse_after_on_pulse() {
    let rt = with_task(|task| async move {
        note(&task, "t1");
        task.next_pulse().await;
        note(&task, "t2");
        task.next_pulse().await;
        note(&task, "t3");
    });
    assert_eq!(Mq::new(rt).task_count(), 1);
    for _ in 0..4 {
        rt.pulse();
    }
    assert_eq!(log_of(rt), ["pulse", "t1", "pulse", "t2", "pulse", "t3", "pulse"]);
    assert_eq!(Mq::new(rt).task_count(), 0);
}

#[test]
fn tasks_spawned_by_tasks_start_on_the_next_pulse() {
    let rt = with_task(|task| async move {
        note(&task, "outer");
        task.with(|mq| {
            mq.spawn_task(|inner: Task<Recorder>| async move { note(&inner, "inner") });
        });
    });
    rt.pulse();
    rt.pulse();
    assert_eq!(log_of(rt), ["pulse", "outer", "pulse", "inner"]);
}

#[test]
fn delay_waits_on_the_clock() {
    let rt = with_task(|task| async move {
        note(&task, "start");
        task.delay(Duration::from_secs(1)).await;
        note(&task, "done");
    });
    rt.pulse();
    advance_clock(Duration::from_millis(500));
    rt.pulse();
    assert!(!log_of(rt).contains(&"done".to_owned()));
    advance_clock(Duration::from_millis(600));
    rt.pulse();
    assert_eq!(log_of(rt), ["pulse", "start", "pulse", "pulse", "done"]);
}

#[test]
fn wait_until_checks_every_pulse_and_can_time_out() {
    let rt = with_task(|task| async move {
        task.wait_until(|mq| mq.target().is_some()).await;
        note(&task, "target seen");
        let lost = task.wait_until_timeout(|mq| mq.target().is_none(), Duration::from_secs(2)).await;
        note(&task, &format!("target lost: {lost}"));
    });
    rt.pulse();
    rt.pulse();
    HAS_TARGET.with(|t| t.set(true));
    rt.pulse();
    advance_clock(Duration::from_secs(3));
    rt.pulse();
    assert_eq!(log_of(rt), ["pulse", "pulse", "pulse", "target seen", "pulse", "target lost: false"]);
}

#[test]
fn a_failed_task_is_reported_and_removed() {
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.spawn_fallible_task(|task: Task<Recorder>| async move {
            task.next_pulse().await;
            Err::<(), _>("no target")
        });
    });
    rt.initialize();
    rt.pulse();
    rt.pulse();
    assert!(chat_lines().contains(&"[test] task failed: no target".to_owned()));
    assert_eq!(Mq::new(rt).task_count(), 0);
}

#[test]
fn a_panicking_task_is_removed_and_others_keep_running() {
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.spawn_task(|_task: Task<Recorder>| async move { panic!("boom") });
        mq.spawn_task(|task: Task<Recorder>| async move {
            for i in 0..3 {
                note(&task, &format!("still running {i}"));
                task.next_pulse().await;
            }
        });
    });
    rt.initialize();
    rt.pulse();
    rt.pulse();
    assert!(chat_lines().contains(&"[test] panicked in a task: boom".to_owned()));
    assert_eq!(rt.stats().panics, 1);
    assert!(log_of(rt).contains(&"still running 1".to_owned()));
    assert_eq!(Mq::new(rt).task_count(), 1);
}

thread_local! {
    static TASK_ID: Cell<Option<TaskId>> = const { Cell::new(None) };
    static DROPPED: Cell<u32> = const { Cell::new(0) };
}

#[test]
fn cancelled_tasks_stop() {
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        let id = mq.spawn_task(|task: Task<Recorder>| async move {
            loop {
                note(&task, "tick");
                task.next_pulse().await;
            }
        });
        TASK_ID.with(|t| t.set(Some(id)));
    });
    rt.initialize();
    rt.pulse();
    rt.pulse();
    let id = TASK_ID.with(Cell::get).unwrap();
    assert!(Mq::new(rt).cancel_task(id));
    assert!(!Mq::new(rt).cancel_task(id), "already cancelled");
    rt.pulse();
    let ticks = log_of(rt).iter().filter(|e| *e == "tick").count();
    assert_eq!(ticks, 2);
    assert_eq!(Mq::new(rt).task_count(), 0);
}

#[test]
fn a_task_can_cancel_itself() {
    let rt = with_task(|task| async move {
        note(&task, "before");
        let id = task.id();
        task.with(|mq| assert!(mq.cancel_task(id)));
        task.next_pulse().await;
        note(&task, "never");
    });
    rt.pulse();
    rt.pulse();
    assert_eq!(log_of(rt), ["pulse", "before", "pulse"]);
    assert_eq!(Mq::new(rt).task_count(), 0);
}

struct CountDrop;
impl Drop for CountDrop {
    fn drop(&mut self) {
        DROPPED.with(|d| d.set(d.get() + 1));
    }
}

#[test]
fn shutdown_drops_running_tasks() {
    let rt = with_task(|task| async move {
        let _guard = CountDrop;
        loop {
            task.next_pulse().await;
        }
    });
    rt.pulse();
    assert_eq!(DROPPED.with(Cell::get), 0);
    rt.shutdown();
    assert_eq!(DROPPED.with(Cell::get), 1);
}

#[test]
fn chat_from_a_task_is_delivered_after_the_tasks() {
    let rt = with_task(|task| async move {
        task.with(|mq| mq.chat("from task"));
        note(&task, "task end");
    });
    rt.pulse();
    assert_eq!(log_of(rt), ["pulse", "task end", "chat:from task"]);
}

#[test]
fn with_plugin_refuses_while_a_callback_holds_the_plugin() {
    let rt = with_task(|task| async move {
        let saved = task.clone();
        task.with_plugin(move |plugin, _| plugin.saved_task = Some(saved));
    });
    rt.pulse();
    set_hook(&ON_PULSE, |plugin, _| {
        let task = plugin.saved_task.clone().unwrap();
        task.with_plugin(|_, _| ());
    });
    rt.pulse();
    let reports: Vec<_> = chat_lines().into_iter().filter(|l| l.contains("panicked in on_pulse")).collect();
    assert_eq!(reports.len(), 1, "{reports:?}");
    assert!(reports[0].contains("already borrowed"), "{reports:?}");
}

#[test]
fn fallible_tasks_can_use_the_question_mark_and_succeed_quietly() {
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.spawn_fallible_task(|task: Task<Recorder>| async move {
            task.with(|mq| mq.set_key(MoveKey::Forward, true))?;
            task.next_pulse().await;
            task.with(|mq| mq.stop_moving())
        });
    });
    rt.initialize();
    rt.pulse();
    rt.pulse();
    assert!(chat_lines().is_empty(), "{:?}", chat_lines());
    assert_eq!(executed(), [(2, true), (2, false)]);

    // And a `?` that fails ends the task with a report.
    GAME_STATE.with(|state| state.set(mq_emu_sys::constants::GAMESTATE_CHARSELECT));
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.spawn_fallible_task(|task: Task<Recorder>| async move {
            task.with(|mq| mq.set_key(MoveKey::Forward, true))?;
            note(&task, "unreachable");
            Ok::<(), MoveError>(())
        });
    });
    rt.initialize();
    rt.pulse();
    assert!(chat_lines().contains(&"[test] task failed: not in game".to_owned()), "{:?}", chat_lines());
}

#[test]
fn endless_tasks_need_no_annotation() {
    // `loop {}` has type `!`; spawn_task's `()` output accepts it as is.
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.spawn_task(|task: Task<Recorder>| async move {
            loop {
                task.next_pulse().await;
            }
        });
    });
    rt.initialize();
    rt.pulse();
    assert_eq!(Mq::new(rt).task_count(), 1);
}

// ----- actors (messaging) ------------------------------------------------------------

use crate::actors::Address;

impl FakeMain {
    fn has_actor(&self, name: &str) -> bool {
        let name = CString::new(name).unwrap();
        unsafe { mqemu_test_fake_has_actor(self.0, name.as_ptr()) }
    }

    fn deliver(&self, mailbox: &str, from: Option<&str>, data: &str) -> bool {
        let mailbox = CString::new(mailbox).unwrap();
        let from = from.map(|f| CString::new(f).unwrap());
        let from_ptr = from.as_ref().map_or(ptr::null(), |f| f.as_ptr());
        unsafe { mqemu_test_fake_deliver(self.0, mailbox.as_ptr(), from_ptr, ptr::null(), data.as_ptr().cast(), data.len()) }
    }

    fn respond(&self, status: i32, data: &str) -> bool {
        unsafe { mqemu_test_fake_respond(self.0, status, data.as_ptr().cast(), data.len()) }
    }

    fn last_sent(&self) -> String {
        let mut buffer = [0 as c_char; 512];
        unsafe { mqemu_test_fake_last_sent(self.0, buffer.as_mut_ptr(), buffer.len()) };
        unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().into_owned()
    }

    fn last_reply(&self) -> Option<(String, u8)> {
        let mut buffer = [0 as c_char; 512];
        let mut status = 0u8;
        unsafe { mqemu_test_fake_last_reply(self.0, buffer.as_mut_ptr(), buffer.len(), &mut status) }
            .then(|| (unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy().into_owned(), status))
    }
}

#[test]
fn the_default_mailbox_feeds_on_message() {
    let main = FakeMain::install();
    let rt = runtime();
    rt.initialize();
    assert!(main.has_actor("test"), "registered under the plugin's name");
    assert!(main.deliver("test", Some("Bob"), "hello"));
    assert_eq!(log_of(rt), ["new", r#"msg:hello from Some("Bob")"#]);
}

#[test]
fn named_mailboxes_route_to_their_handler() {
    let main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| {
        mq.add_mailbox("follow", |plugin: &mut Recorder, _, message| plugin.log.push(format!("follow:{}", message.text()))).unwrap();
        for name in ["follow", "", "two words", "Other:box"] {
            plugin.log.push(format!("{name:?}={:?}", mq.add_mailbox(name, |_: &mut Recorder, _, _| {})));
        }
    });
    rt.initialize();
    assert!(main.has_actor("test:follow"));
    assert!(main.deliver("test:follow", None, "come here"));
    assert_eq!(
        log_of(rt),
        [
            "new",
            r#""follow"=Err(AlreadyRegistered)"#,
            r#"""=Err(InvalidName)"#,
            r#""two words"=Err(InvalidName)"#,
            r#""Other:box"=Err(InvalidName)"#,
            "follow:come here",
        ]
    );
}

#[test]
fn removed_mailboxes_are_gone() {
    let main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| mq.add_mailbox("temp", |_: &mut Recorder, _, _| {}).unwrap());
    rt.initialize();
    set_hook(&ON_PULSE, |plugin, mq| {
        plugin.log.push(format!("{} {}", mq.remove_mailbox("temp"), mq.remove_mailbox("temp")));
    });
    rt.pulse();
    assert!(!main.has_actor("test:temp"));
    assert!(!main.deliver("test:temp", None, "x"));
    assert_eq!(log_of(rt)[1..], ["pulse", "true false"]);
}

#[test]
fn sends_to_lua_scripts_use_the_name_mq2lua_was_loaded_as() {
    let main = FakeMain::install();
    for (loaded_as, mailbox) in [(Some("lua"), "lua:boxer"), (Some("Lua"), "Lua:boxer"), (None, "lua:boxer")] {
        LUA_LOADED_AS.with(|name| name.set(loaded_as));
        in_pulse(|_, mq| mq.send(&Address::lua_script("boxer").on_character("Bob"), "x").unwrap());
        assert_eq!(main.last_sent(), format!("dropbox=0 callback=0 character=Bob mailbox={mailbox} absolute=1 data=x"), "loaded as {loaded_as:?}");
    }
    // Another plugin's mailbox is left alone.
    in_pulse(|_, mq| mq.send(&Address::character("Bob").mailbox("Other:lua").absolute(), "x").unwrap());
    assert_eq!(main.last_sent(), "dropbox=0 callback=0 character=Bob mailbox=Other:lua absolute=1 data=x");
}

#[test]
fn send_passes_the_address_through() {
    let main = FakeMain::install();
    in_pulse(|_, mq| {
        mq.send(&Address::character("Bob").mailbox("follow"), "come").unwrap();
    });
    assert_eq!(main.last_sent(), "dropbox=0 callback=0 character=Bob mailbox=follow absolute=0 data=come");
    in_pulse(|_, mq| {
        mq.send(&Address::character("Bob").mailbox("lua_box").absolute(), [1u8, 2]).unwrap();
    });
    assert!(main.last_sent().starts_with("dropbox=0 callback=0 character=Bob mailbox=lua_box absolute=1 data="));
}

#[test]
fn messages_can_be_replied_to() {
    let main = FakeMain::install();
    let rt = runtime();
    rt.initialize();
    assert!(main.deliver("test", Some("Bob"), "ping"));
    assert_eq!(main.last_reply(), Some(("pong".to_owned(), 0)));
}

#[test]
fn replying_after_the_mailbox_is_removed_fails_cleanly() {
    let main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| {
        mq.add_mailbox("rpc", |plugin: &mut Recorder, _, message| plugin.kept_message = Some(message)).unwrap();
    });
    rt.initialize();
    assert!(main.deliver("test:rpc", None, "later"));
    set_hook(&ON_PULSE, |plugin, mq| {
        let message = plugin.kept_message.take().unwrap();
        plugin.log.push(format!("{:?}", mq.reply(&message, "now", 0)));
        mq.remove_mailbox("rpc");
        plugin.log.push(format!("{:?}", mq.reply(&message, "too late", 0)));
    });
    rt.pulse();
    assert_eq!(log_of(rt)[1..], ["pulse", "Ok(())", "Err(MailboxGone)"]);
    assert_eq!(unsafe { mqemu_test_fake_reply_count(main.0) }, 1);
}

fn request_task(timeout: Duration) -> &'static Runtime<Recorder> {
    with_task(move |task| async move {
        let result = task.request_timeout(&Address::character("Bob"), "ping", timeout).await;
        note(&task, &format!("reply:{:?}", result.map(|m| m.text().into_owned())));
    })
}

#[test]
fn a_request_resolves_with_the_reply() {
    let main = FakeMain::install();
    let rt = request_task(Duration::from_secs(5));
    rt.pulse();
    assert_eq!(main.last_sent(), "dropbox=1 callback=1 character=Bob mailbox=- absolute=0 data=ping");
    assert!(main.respond(0, "pong"));
    rt.pulse();
    assert_eq!(log_of(rt), ["pulse", "pulse", r#"reply:Ok("pong")"#]);
}

#[test]
fn a_request_reports_routing_failures() {
    let main = FakeMain::install();
    let rt = request_task(Duration::from_secs(5));
    rt.pulse();
    assert!(main.respond(-2, ""));
    rt.pulse();
    assert_eq!(log_of(rt)[2..], ["reply:Err(NoConnection)"]);
}

#[test]
fn a_request_times_out_and_a_late_reply_is_harmless() {
    let main = FakeMain::install();
    let rt = request_task(Duration::from_secs(5));
    rt.pulse();
    advance_clock(Duration::from_secs(6));
    rt.pulse();
    assert_eq!(log_of(rt)[2..], ["reply:Err(Timeout)"]);
    // The reply arrives after the task gave up: freed, not delivered anywhere.
    assert!(main.respond(0, "pong"));
    rt.pulse();
    assert_eq!(unsafe { mqemu_test_fake_pending_count(main.0) }, 0);
}

#[test]
fn a_message_arriving_mid_callback_is_queued() {
    let main = FakeMain::install();
    let fake = main.0;
    let rt = runtime();
    set_hook(&ON_PULSE, move |plugin, _| {
        let (mailbox, data) = (CString::new("test").unwrap(), "while busy");
        unsafe { mqemu_test_fake_deliver(fake, mailbox.as_ptr(), ptr::null(), ptr::null(), data.as_ptr().cast(), data.len()) };
        plugin.log.push("pulse:end".into());
    });
    rt.initialize();
    rt.pulse();
    assert_eq!(log_of(rt), ["new", "pulse", "pulse:end", "msg:while busy from None"]);
}

#[test]
fn shutdown_closes_every_mailbox() {
    let main = FakeMain::install();
    let rt = runtime();
    set_hook(&ON_NEW, |_, mq| mq.add_mailbox("a", |_: &mut Recorder, _, _| {}).unwrap());
    rt.initialize();
    assert_eq!(unsafe { mqemu_test_fake_actor_count(main.0) }, 2);
    rt.shutdown();
    assert_eq!(unsafe { mqemu_test_fake_actor_count(main.0) }, 0);
}

#[test]
fn messaging_is_unavailable_without_macroquest() {
    let rt = with_task(|task| async move {
        let request = task.request(&Address::character("Bob"), "x").await;
        note(&task, &format!("{:?}", request.map(|_| ())));
    });
    set_hook(&ON_PULSE, |plugin, mq| {
        plugin.log.push(format!("{:?}", mq.add_mailbox("box", |_: &mut Recorder, _, _| {})));
        plugin.log.push(format!("{:?}", mq.send(&Address::character("Bob"), "x")));
    });
    rt.pulse();
    assert_eq!(log_of(rt), ["pulse", "Err(Unavailable)", "Err(Unavailable)", "Err(Unavailable)"]);
}

#[test]
fn a_panicking_mailbox_handler_is_contained() {
    let main = FakeMain::install();
    let rt = runtime();
    rt.initialize();
    assert!(main.deliver("test", None, "panic"));
    assert!(chat_lines().contains(&"[test] panicked in a mailbox handler: bad message".to_owned()), "{:?}", chat_lines());
    assert!(main.deliver("test", None, "after"));
    assert_eq!(log_of(rt).last().unwrap(), "msg:after from None");
}

#[test]
fn mailboxes_use_macroquests_name_for_the_plugin() {
    // Found in game: MacroQuest calls MQ2RustPluginTest.dll "RustPluginTest"
    // and addresses its mailboxes that way, so registering them under the
    // crate name meant nothing was ever delivered.
    let main = FakeMain::install();
    let name = CString::new("RustPluginTest").unwrap();
    let plugin = unsafe { mq_emu_sys::shim::testing::mqemu_test_new_plugin(name.as_ptr()) };
    let rt = runtime();
    rt.init_handle(plugin, MQPluginHandle { pluginID: 7 });
    set_hook(&ON_NEW, |_, mq| mq.add_mailbox("follow", |_: &mut Recorder, _, _| {}).unwrap());
    rt.initialize();
    assert!(main.has_actor("RustPluginTest"));
    assert!(main.has_actor("RustPluginTest:follow"));
    assert!(!main.has_actor("test"), "not the crate name");
    rt.shutdown();
    unsafe { mq_emu_sys::shim::testing::mqemu_test_free_plugin(plugin) };
}

#[test]
fn the_plugins_ini_is_named_as_a_cpp_plugins_would_be() {
    // A C++ plugin's INIFileName is "<gPathConfig>\<PreSetup name>.ini", e.g.
    // MQ2Map.ini: the DLL's name, not the shorter one MacroQuest uses for
    // mailboxes.
    let rt = runtime();
    set_hook(&ON_NEW, |plugin, mq| {
        plugin.log.push(format!("{} {}", mq.config_dir().display(), mq.ini().path().display()));
    });
    rt.initialize();
    assert_eq!(log_of(rt), ["new", r"C:\MQ\config C:\MQ\config\test.ini"]);
}
