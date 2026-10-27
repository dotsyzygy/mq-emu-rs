//! Imports from `MQ2Main.dll` (`MQLIB_API` / `MQLIB_VAR`).
//!
//! C++ default arguments don't exist across FFI; every parameter must be passed.
//!
//! Commands and aliases (`AddCommand`, `DoCommand`, `TimedCommand`, ...) are not
//! C exports: plugins reach them through MacroQuest's C++ `MainInterface`, so
//! they aren't declared here.

use core::ffi::{c_char, c_int, c_void};

use crate::constants::{MAX_PATH, MAX_STRING};
use crate::layouts::{ItemClient, ItemGlobalIndex, KeyCombo};
use crate::types::fEQCommand;

/// A MacroQuest key bind's function (`fMQExecuteCmd`, mq/utils/Keybinds.h):
/// the bind's name, and whether its key went down (true) or up.
pub type fMQExecuteCmd = unsafe extern "C" fn(name: *const c_char, down: bool);

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
        /// Adds a MacroQuest key bind, `/bind`'s to set, starting with the
        /// keys saved in MacroQuest.ini (`[Key Binds]`, `<name>_Nrm` and
        /// `<name>_Alt`). `function` is called with the bind's name as its key
        /// goes down and up. False if a bind by that name (in any case)
        /// exists. MacroQuest doesn't remove a plugin's binds when it unloads.
        pub fn AddMQ2KeyBind(name: *const c_char, function: fMQExecuteCmd) -> bool;
        /// Removes any MacroQuest key bind, whoever added it. Its keys stay
        /// in MacroQuest.ini.
        pub fn RemoveMQ2KeyBind(name: *const c_char) -> bool;
        /// Sets a MacroQuest key bind's normal or alternate keys and saves
        /// them to MacroQuest.ini. `combo`'s key must be one `gDiKeyName`
        /// names: saving writes its name with `DescribeKeyCombo`.
        pub fn SetMQ2KeyBind(name: *const c_char, alternate: bool, combo: *mut KeyCombo) -> bool;
        /// A MacroQuest key bind's normal or alternate keys.
        pub fn GetMQ2KeyBind(name: *const c_char, alternate: bool, combo: *mut KeyCombo) -> bool;
        /// Calls a MacroQuest key bind's function as if its key went down,
        /// then (unless `hold`) up.
        pub fn PressMQ2KeyBind(name: *const c_char, hold: bool) -> bool;
        /// Sets one of the game's own key bindings (by its game name) and,
        /// for the game's commands (below `N_NORMAL_EQ_MAPPABLE_COMMANDS`),
        /// saves it as the game does. Needs `pKeypressHandler`.
        pub fn SetEQKeyBind(name: *const c_char, alternate: bool, combo: *mut KeyCombo) -> bool;
        /// Reads `/bind`'s notation ("ctrl+shift+F1", "clear") into `combo`.
        /// `text` must be shorter than `MAX_STRING`.
        pub fn ParseKeyCombo(text: *const c_char, combo: *mut KeyCombo) -> bool;
        /// Writes `combo` in `/bind`'s notation into `out` and returns it.
        /// Its key must be one `gDiKeyName` names, or this reads a null name.
        pub fn DescribeKeyCombo(combo: *const KeyCombo, out: *mut c_char, size: usize) -> *mut c_char;
        /// Writes every binding as `/bind` lines to `file` (relative to the
        /// config directory; `.cfg` added if there's no extension).
        pub fn DumpBinds(file: *const c_char) -> bool;

        // Chat text (MQ2Main.h)
        /// Copies `input` to `out` without MacroQuest's color codes (`\a...`)
        /// and line breaks. `out` must hold `strlen(input) + 1` bytes.
        pub fn StripMQChat(input: *const c_char, out: *mut c_char);

        // The UI's XML (MQ2Main.h)
        /// Adds a file of UI XML (screen pieces) to what MacroQuest includes
        /// when the UI next loads. Looked for in `uifiles\<skin>` then
        /// `uifiles\default`, under MacroQuest's resources or EverQuest; says
        /// so in chat if it isn't there, or if the UI must be reloaded.
        pub fn AddXMLFile(filename: *const c_char);
        /// Takes a file [`AddXMLFile`] added back out, from the next UI load.
        pub fn RemoveXMLFile(filename: *const c_char);
        /// Whether the loaded UI has a screen piece (a window's template) by
        /// this name.
        pub fn IsScreenPieceLoaded(name: *const c_char) -> bool;

        // Items (MQ2Main.h), what /itemnotify does without a slot window
        /// Moves the item at `index` to the cursor (or, with the merchant
        /// window open, selects it). False, said in chat, if it can't.
        pub fn PickupItem(index: *const ItemGlobalIndex) -> bool;
        /// Moves the item on the cursor to `index`.
        pub fn DropItem(index: *const ItemGlobalIndex) -> bool;
        /// Opens a bag through its slot window; true only if it was open already.
        pub fn OpenContainer(item: *mut ItemClient, hidden: bool, allow_tradeskill: bool) -> bool;
        /// Closes a bag; true if it was open.
        pub fn CloseContainer(item: *mut ItemClient) -> bool;
        /// The item at `index`, in any of your containers.
        pub fn FindItemByGlobalIndex(index: *const ItemGlobalIndex) -> *mut ItemClient;
        /// Whether an item is on the cursor.
        pub fn ItemOnCursor() -> bool;

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
        /// Set once the server has sent the current target's buffs
        /// (`${Target.BuffsPopulated}`); cleared when the target changes.
        pub static mut gTargetbuffs: bool;
        /// The game's own `/useitem` (MacroQuest's replaces it): takes "<slot>
        /// <slot in bag>" (-1 for none). Null before MacroQuest has it.
        pub static mut cmdUseItem: fEQCommand;
    }
}
