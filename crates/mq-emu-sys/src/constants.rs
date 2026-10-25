//! Constants mirrored from the MacroQuest and eqlib headers.
//!
//! `tests/header_constants.rs` checks these against the headers in `$MQ_ROOT`.

/// `MAX_STRING` (`mq/base/Base.h`): size of MacroQuest's standard string buffers.
pub const MAX_STRING: usize = 2048;
/// Win32 `MAX_PATH`: size of the `gPath*` buffers.
pub const MAX_PATH: usize = 260;

// Game states (eqlib `game/Constants.h`), as returned by `GetGameState`.
pub const GAMESTATE_PRECHARSELECT: i32 = -1;
pub const GAMESTATE_CHARSELECT: i32 = 1;
pub const GAMESTATE_CHARCREATE: i32 = 2;
pub const GAMESTATE_POSTCHARSELECT: i32 = 3;
pub const GAMESTATE_SOMETHING: i32 = 4;
pub const GAMESTATE_INGAME: i32 = 5;
pub const GAMESTATE_LOGGINGIN: i32 = 253;
pub const GAMESTATE_UNLOADING: i32 = 255;

// Chat colors (eqlib `game/ChatFilters.h`), for `WriteChatColor`.
pub const USERCOLOR_DEFAULT: i32 = 273;
pub const CONCOLOR_GREEN: i32 = 0x02;
pub const CONCOLOR_WHITE: i32 = 0x0a;
pub const CONCOLOR_RED: i32 = 0x0d;
pub const CONCOLOR_YELLOW: i32 = 0x0f;
pub const CONCOLOR_LIGHTBLUE: i32 = 0x12;

/// `nEQMappableCommands` (eqlib `game/Constants.h`): number of key-bindable commands.
pub const N_EQ_MAPPABLE_COMMANDS: usize = 500;

// Key-bindable commands (eqlib `game/Constants.h`, `enum KeybindCommand`), for
// `EQExecuteCmd` and `KeypressHandler::CommandState`.
pub const CMD_AUTORUN: u32 = 0;
pub const CMD_JUMP: u32 = 1;
pub const CMD_FORWARD: u32 = 2;
pub const CMD_BACK: u32 = 3;
pub const CMD_RIGHT: u32 = 4;
pub const CMD_LEFT: u32 = 5;
pub const CMD_STRAFE_LEFT: u32 = 6;
pub const CMD_STRAFE_RIGHT: u32 = 7;
pub const CMD_DUCK: u32 = 12;
pub const CMD_RUN_WALK: u32 = 284;

/// The value of `gFaceAngle` / `gLookAngle` when no turn is in progress
/// (MacroQuest's `MQ2Pulse.cpp` compares against `10000.0f`).
pub const FACE_ANGLE_INACTIVE: f64 = 10000.0;

// Items, inventory and bank (eqlib `game/Constants.h`).
/// `ItemDefinition::Type` of a container (a bag).
pub const ITEMTYPE_PACK: u8 = 1;
pub const MAX_AUG_SOCKETS: usize = 6;
pub const NUM_BANK_SLOTS: usize = 24;
pub const NUM_SHAREDBANK_SLOTS: usize = 2;

// Buffs and spells (eqlib `game/Constants.h`, `game/Spells.h`).
/// Long buff slots, at the start of `BaseProfile::Buffs`.
pub const NUM_LONG_BUFFS: usize = 42;
/// Short buff (song) slots, after the long ones.
pub const NUM_TEMP_BUFFS: usize = 55;
/// Spell gems.
pub const MAX_MEMORIZED_SPELLS: usize = 16;
pub const NUM_BOOK_SLOTS: usize = 720;
/// Size of `ClientSpellManager::Spells`; spell IDs are below this.
pub const TOTAL_SPELL_COUNT: usize = 45001;
/// Effect slots per spell (`EQ_Spell::Attrib`, `Base`, ...).
pub const MAX_EFFECT_SLOTS: usize = 12;

// Groups and zones (eqlib `game/Constants.h`, `game/EQClasses.h`).
pub const MAX_GROUP_SIZE: usize = 6;
/// Size of `EQWorldData::ZoneArray`; zone IDs are below this.
pub const MAX_ZONES: usize = 1000;
