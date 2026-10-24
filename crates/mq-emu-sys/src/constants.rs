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
