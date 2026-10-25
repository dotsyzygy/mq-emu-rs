//! The [`Plugin`] trait and the [`plugin!`](crate::plugin!) macro.

use mq_emu_sys::types::EQGroundItem;

use crate::actors::Message;
use crate::game::{GameState, Spawn};
use crate::imgui::Ui;
use crate::mq::Mq;

/// A MacroQuest plugin. Implement the callbacks you need; the rest default to
/// doing nothing. Then export it with [`plugin!`](crate::plugin!).
///
/// Every callback runs on the game thread, inside the game's frame: time spent
/// here is frame time. A panic in a callback is caught, reported once in chat,
/// and counted in [`Mq::stats`]; the plugin keeps running.
///
/// If a callback causes another callback for this plugin (for example
/// [`Mq::chat`] inside `on_pulse` triggers `on_write_chat_color`), the second
/// one is queued and delivered as soon as the first returns, still within the
/// same MacroQuest call. Spawn and ground item events can't be queued (the
/// object may be gone), so in that situation they're dropped and counted.
#[allow(unused_variables)]
pub trait Plugin: Sized + 'static {
    /// Creates the plugin when MacroQuest loads it. Register commands here.
    fn new(mq: &Mq<Self>) -> Self;

    /// Called once before unloading. Commands are removed automatically afterwards.
    fn shutdown(&mut self, mq: &Mq<Self>) {}

    /// Called every frame. Keep it fast; use counters or timers to do work less often.
    fn on_pulse(&mut self, mq: &Mq<Self>) {}

    /// A line of chat arrived (after MacroQuest's filters and events). Return
    /// `true` to suppress it.
    fn on_incoming_chat(&mut self, mq: &Mq<Self>, line: &str, color: u32) -> bool {
        false
    }

    /// Something was written to the MacroQuest chat window, by MacroQuest or any plugin.
    fn on_write_chat_color(&mut self, mq: &Mq<Self>, line: &str, color: u32) {}

    /// Entered a zone and it's fully loaded.
    fn on_zoned(&mut self, mq: &Mq<Self>) {}
    /// The UI is being torn down (e.g. `/loadskin`, character select). Destroy custom windows.
    fn on_clean_ui(&mut self, mq: &Mq<Self>) {}
    /// The UI was (re)loaded. Recreate custom windows.
    fn on_reload_ui(&mut self, mq: &Mq<Self>) {}
    /// The HUD is being drawn (only while it's shown).
    fn on_draw_hud(&mut self, mq: &Mq<Self>) {}
    /// The game state changed (also called once after loading).
    fn on_set_game_state(&mut self, mq: &Mq<Self>, state: GameState) {}

    /// A spawn appeared (also called for every existing spawn after loading).
    fn on_add_spawn(&mut self, mq: &Mq<Self>, spawn: &Spawn<'_>) {}
    /// A spawn is about to disappear.
    fn on_remove_spawn(&mut self, mq: &Mq<Self>, spawn: &Spawn<'_>) {}
    /// A ground item appeared (also called for every existing one after loading).
    fn on_add_ground_item(&mut self, mq: &Mq<Self>, item: &EQGroundItem) {}
    /// A ground item is about to disappear.
    fn on_remove_ground_item(&mut self, mq: &Mq<Self>, item: &EQGroundItem) {}

    /// Zoning started (loading screen).
    fn on_begin_zone(&mut self, mq: &Mq<Self>) {}
    /// Zoning finished loading, before spawns are added and `on_zoned`.
    fn on_end_zone(&mut self, mq: &Mq<Self>) {}
    /// MacroQuest is drawing its ImGui overlay: draw your windows with `ui`
    /// (see [`crate::imgui`]). Called every frame, even with nothing to show,
    /// so return quickly when hidden. Never queued: if it arrives while
    /// another callback is running, that frame is skipped.
    fn on_update_imgui(&mut self, mq: &Mq<Self>, ui: &mut Ui) {}

    fn on_macro_start(&mut self, mq: &Mq<Self>, name: &str) {}
    fn on_macro_stop(&mut self, mq: &Mq<Self>, name: &str) {}
    /// A plugin (possibly this one) finished loading.
    fn on_load_plugin(&mut self, mq: &Mq<Self>, name: &str) {}
    /// A plugin (possibly this one) is about to unload.
    fn on_unload_plugin(&mut self, mq: &Mq<Self>, name: &str) {}
    /// Another plugin finished unloading.
    fn on_post_unload_plugin(&mut self, mq: &Mq<Self>, name: &str) {}

    /// A message arrived at this plugin's default mailbox (the one named after
    /// the plugin; see [`Mq::add_mailbox`] for named ones). Reply with
    /// [`Mq::reply`], now or later.
    fn on_message(&mut self, mq: &Mq<Self>, message: Message) {}
}

/// Exports a [`Plugin`] type as a MacroQuest plugin.
///
/// Use it once, in a `cdylib` crate whose library name is the plugin name
/// (e.g. `[lib] name = "MQ2Example"` produces `MQ2Example.dll`):
///
/// ```ignore
/// struct Example;
///
/// impl mq_emu::Plugin for Example {
///     fn new(mq: &mq_emu::Mq<Self>) -> Self {
///         mq.chat("Example loaded");
///         Example
///     }
/// }
///
/// mq_emu::plugin!(Example);
/// ```
///
/// This generates the symbols MacroQuest's loader looks for: the
/// `IsBuiltForNext` and `EverQuestVersion` sentinels and every callback.
#[macro_export]
macro_rules! plugin {
    ($plugin:ty) => {
        #[doc(hidden)]
        static __MQ_EMU_RUNTIME: $crate::__private::Runtime<$plugin> =
            $crate::__private::Runtime::new(::core::env!("CARGO_CRATE_NAME"));

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        #[unsafe(no_mangle)]
        pub static IsBuiltForNext: bool = true;

        #[doc(hidden)]
        #[allow(non_upper_case_globals)]
        #[unsafe(no_mangle)]
        pub static EverQuestVersion: [u8; $crate::sys::EVERQUEST_VERSION_NUL.len()] = $crate::sys::EVERQUEST_VERSION_NUL;

        #[doc(hidden)]
        #[allow(non_snake_case)]
        pub mod __mq_emu_exports {
            use super::__MQ_EMU_RUNTIME as RT;
            use ::core::ffi::c_char;
            use $crate::__private::NamedEvent;
            use $crate::sys::plugin::MQPluginHandle;
            use $crate::sys::types::{EQGroundItem, MQPlugin, PlayerClient};

            #[unsafe(no_mangle)]
            pub extern "C" fn InitPluginHandle(plugin: *mut MQPlugin, handle: MQPluginHandle) {
                RT.init_handle(plugin, handle)
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn InitializePlugin() {
                RT.initialize()
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn ShutdownPlugin() {
                RT.shutdown()
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnPulse() {
                RT.pulse()
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn OnIncomingChat(line: *const c_char, color: u32) -> bool {
                unsafe { RT.incoming_chat(line, color) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn OnWriteChatColor(line: *const c_char, color: u32, _filter: u32) -> i32 {
                unsafe { RT.write_chat_color(line, color) }
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnZoned() {
                RT.zoned()
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnCleanUI() {
                RT.clean_ui()
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnReloadUI() {
                RT.reload_ui()
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnDrawHUD() {
                RT.draw_hud()
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn SetGameState(state: i32) {
                RT.set_game_state(state)
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnAddSpawn(spawn: *mut PlayerClient) {
                RT.add_spawn(spawn)
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnRemoveSpawn(spawn: *mut PlayerClient) {
                RT.remove_spawn(spawn)
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnAddGroundItem(item: *mut EQGroundItem) {
                RT.add_ground_item(item)
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnRemoveGroundItem(item: *mut EQGroundItem) {
                RT.remove_ground_item(item)
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnBeginZone() {
                RT.begin_zone()
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnEndZone() {
                RT.end_zone()
            }
            #[unsafe(no_mangle)]
            pub extern "C" fn OnUpdateImGui() {
                RT.update_imgui()
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn OnMacroStart(name: *const c_char) {
                unsafe { RT.named(NamedEvent::MacroStart, name) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn OnMacroStop(name: *const c_char) {
                unsafe { RT.named(NamedEvent::MacroStop, name) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn OnLoadPlugin(name: *const c_char) {
                unsafe { RT.named(NamedEvent::LoadPlugin, name) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn OnUnloadPlugin(name: *const c_char) {
                unsafe { RT.named(NamedEvent::UnloadPlugin, name) }
            }
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn OnPostUnloadPlugin(name: *const c_char) {
                unsafe { RT.named(NamedEvent::PostUnloadPlugin, name) }
            }
        }
    };
}
