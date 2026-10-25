//! Imports from `MQ2Main.dll` (`MQLIB_API` / `MQLIB_VAR`).
//!
//! C++ default arguments don't exist across FFI; every parameter must be passed.
//!
//! Commands and aliases (`AddCommand`, `DoCommand`, `TimedCommand`, ...) are not
//! C exports: plugins reach them through MacroQuest's C++ `MainInterface`, so
//! they aren't declared here.

use core::ffi::{c_char, c_int, c_void};

use crate::constants::{MAX_PATH, MAX_STRING};

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

        // Macro data (mq/api/MacroAPI.h)
        /// Expands `${...}` in place. `buffer` must hold `buffer_size` bytes.
        pub fn ParseMacroData(buffer: *mut c_char, buffer_size: usize) -> bool;

        // Game state (MQ2Main.h)
        /// One of the `GAMESTATE_*` constants.
        pub fn GetGameState() -> c_int;

        // Key bindings (mq/utils/Keybinds.h)
        /// The `CMD_*` index of a key-bindable command by its game name
        /// (case-insensitive, e.g. "forward"), or -1.
        pub fn FindMappableCommand(name: *const c_char) -> c_int;

        // Plugins (mq/api/PluginAPI.h)
        /// `GetProcAddress` against another loaded plugin, by plugin name.
        pub fn GetPluginProc(plugin: *const c_char, proc_name: *const c_char) -> *mut c_void;
    }
    statics {
        // MQ2Globals.h
        /// Heading (EverQuest units, 0..512) MacroQuest is turning the
        /// controlled player towards, a little each pulse; `FACE_ANGLE_INACTIVE`
        /// when not turning. Writing it starts a smooth turn.
        pub static mut gFaceAngle: f64;
        /// Like `gFaceAngle`, for the camera's look (pitch) angle.
        pub static mut gLookAngle: f64;
        /// Raw game state; prefer `GetGameState`.
        pub static mut gGameState: c_int;
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
