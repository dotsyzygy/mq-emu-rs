//! Imports from `MQ2Main.dll` (`MQLIB_API` / `MQLIB_VAR`).
//!
//! C++ default arguments don't exist across FFI; every parameter must be passed.

use core::ffi::{c_char, c_int, c_void};

use crate::constants::{MAX_PATH, MAX_STRING};
use crate::types::{PlayerClient, fEQCommand};

imports! {
    dll: "MQ2Main",
    names: MQ2MAIN_IMPORTS,
    functions {
        // Chat output (MQ2Main.h)
        /// `printf`-style output to the MQ chat window.
        pub fn WriteChatf(format: *const c_char, ...);
        /// Writes `line` in `color` (e.g. `USERCOLOR_DEFAULT`). `filter` is normally 0.
        pub fn WriteChatColor(line: *const c_char, color: c_int, filter: c_int);
        pub fn WriteChatColorf(format: *const c_char, color: c_int, ...);
        /// Debug output (OutputDebugString), only when debug spew is enabled.
        pub fn DebugSpew(format: *const c_char, ...);
        /// Debug output (OutputDebugString), always.
        pub fn DebugSpewAlways(format: *const c_char, ...);

        // Commands (MQ2Main.h). C++ defaults: eq = false, parse = true, in_game = false.
        pub fn AddCommand(command: *const c_char, function: fEQCommand, eq: bool, parse: bool, in_game: bool);
        pub fn RemoveCommand(command: *const c_char) -> bool;
        pub fn IsCommand(command: *const c_char) -> bool;
        pub fn AddAlias(short_command: *const c_char, long_command: *const c_char);
        pub fn RemoveAlias(short_command: *const c_char) -> bool;
        pub fn IsAlias(alias: *const c_char) -> bool;
        /// Executes `line` as if typed. `player` may be null; pass `delayed = true`
        /// to queue it for the next pulse (what `DoCommand` does from plugins).
        pub fn HideDoCommand(player: *mut PlayerClient, line: *const c_char, delayed: bool);
        /// Runs `command` after `ms_delay` milliseconds.
        pub fn TimedCommand(command: *const c_char, ms_delay: c_int);

        // Macro data (MQ2Main.h)
        /// Expands `${...}` in place. `buffer` must hold `buffer_size` bytes.
        pub fn ParseMacroData(buffer: *mut c_char, buffer_size: usize) -> bool;

        // Game state (MQ2Main.h)
        /// One of the `GAMESTATE_*` constants.
        pub fn GetGameState() -> c_int;

        // Plugins (MQ2Main.h)
        pub fn IsPluginsInitialized() -> bool;
        /// `GetProcAddress` against another loaded plugin, by plugin name.
        pub fn GetPluginProc(plugin: *const c_char, proc_name: *const c_char) -> *mut c_void;
    }
    statics {
        // MQ2Globals.h
        /// Raw game state; prefer `GetGameState`.
        pub static mut gGameState: u32;
        pub static mut gbInZone: bool;
        /// Name of the running macro, empty if none.
        pub static mut gszMacroName: [c_char; MAX_STRING];
        pub static mut gPathMQRoot: [c_char; MAX_PATH];
        pub static mut gPathConfig: [c_char; MAX_PATH];
        pub static mut gPathMacros: [c_char; MAX_PATH];
        pub static mut gPathLogs: [c_char; MAX_PATH];
        pub static mut gPathPlugins: [c_char; MAX_PATH];
        pub static mut gPathResources: [c_char; MAX_PATH];
    }
}
