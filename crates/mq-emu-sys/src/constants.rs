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

/// `nNormalEQMappableCommands` (eqlib `game/Constants.h`): commands below this
/// are the ones the game saves key bindings for; the rest are MacroQuest's.
pub const N_NORMAL_EQ_MAPPABLE_COMMANDS: usize = 479;

/// Key names and their DirectInput scan codes, as `ParseKeyCombo` reads and
/// `DescribeKeyCombo` writes them (eqlib `eqdata/dikeys.h`, `gDiKeyID`), in
/// the header's order. Names are unique ignoring case, and so are codes.
pub const DI_KEYS: [(&str, u8); 133] = [
    ("1", 2),
    ("2", 3),
    ("3", 4),
    ("4", 5),
    ("5", 6),
    ("6", 7),
    ("7", 8),
    ("8", 9),
    ("9", 10),
    ("0", 11),
    ("-", 12),
    ("=", 13),
    ("Q", 16),
    ("W", 17),
    ("E", 18),
    ("R", 19),
    ("T", 20),
    ("Y", 21),
    ("U", 22),
    ("I", 23),
    ("O", 24),
    ("P", 25),
    ("[", 26),
    ("]", 27),
    ("A", 30),
    ("S", 31),
    ("D", 32),
    ("F", 33),
    ("G", 34),
    ("H", 35),
    ("J", 36),
    ("K", 37),
    ("L", 38),
    (";", 39),
    ("'", 40),
    ("`", 41),
    ("\\", 43),
    ("Z", 44),
    ("X", 45),
    ("C", 46),
    ("V", 47),
    ("B", 48),
    ("N", 49),
    ("M", 50),
    (",", 51),
    (".", 52),
    ("/", 53),
    (":", 146),
    ("F1", 59),
    ("F2", 60),
    ("F3", 61),
    ("F4", 62),
    ("F5", 63),
    ("F6", 64),
    ("F7", 65),
    ("F8", 66),
    ("F9", 67),
    ("F10", 68),
    ("F11", 87),
    ("F12", 88),
    ("F15", 126),
    ("Num_0", 82),
    ("Num_1", 79),
    ("Num_2", 80),
    ("Num_3", 81),
    ("Num_4", 75),
    ("Num_5", 76),
    ("Num_6", 77),
    ("Num_7", 71),
    ("Num_8", 72),
    ("Num_9", 73),
    ("Num_Enter", 156),
    ("Num_Del", 83),
    ("Num_Slash", 181),
    ("Num_Minus", 74),
    ("Num_Plus", 78),
    ("Num_Asterisk", 55),
    ("Up", 200),
    ("Left", 203),
    ("Right", 205),
    ("Down", 208),
    ("Home", 199),
    ("End", 207),
    ("Page_Up", 201),
    ("Page_Down", 209),
    ("Insert", 210),
    ("Delete", 211),
    ("Esc", 1),
    ("Enter", 28),
    ("Backspace", 14),
    ("Tab", 15),
    ("Space", 57),
    ("Ctrl", 29),
    ("Right_Ctrl", 157),
    ("Shift", 42),
    ("Right_Shift", 54),
    ("Alt", 56),
    ("Right_Alt", 184),
    ("Num_Lock", 69),
    ("Scroll_Lock", 70),
    ("Caps_Lock", 58),
    ("Prnt_Scrn", 183),
    ("Left_Windows", 219),
    ("Right_Windows", 220),
    ("Application", 221),
    ("Prev_Track", 144),
    ("Next_Track", 153),
    ("Mute", 160),
    ("Calculator", 161),
    ("Play/Pause", 162),
    ("Media_Stop", 164),
    ("Volume_-", 174),
    ("Volume_+", 176),
    ("Web/Home", 178),
    ("Pause", 197),
    ("Power", 222),
    ("Sleep", 223),
    ("Wake", 227),
    ("Search", 229),
    ("Favorites", 230),
    ("Refresh", 231),
    ("Web_Stop", 232),
    ("Forward", 233),
    ("Back", 234),
    ("My_Computer", 235),
    ("Mail", 236),
    ("Media", 237),
    ("Mouse_Mid", 240),
    ("Mouse_Aux1", 241),
    ("Mouse_Aux2", 242),
    ("Mouse_Aux3", 243),
    ("Mouse_Aux4", 244),
    ("Mouse_Aux5", 245),
];

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
/// Spell gems in the game's spell gem window (`CCastSpellWnd::SpellSlots`).
pub const NUM_SPELL_GEMS: usize = 12;
pub const NUM_BOOK_SLOTS: usize = 720;
/// Size of `ClientSpellManager::Spells`; spell IDs are below this.
pub const TOTAL_SPELL_COUNT: usize = 45001;
/// Effect slots per spell (`EQ_Spell::Attrib`, `Base`, ...).
pub const MAX_EFFECT_SLOTS: usize = 12;

// Groups and zones (eqlib `game/Constants.h`, `game/EQClasses.h`).
pub const MAX_GROUP_SIZE: usize = 6;
/// Size of `EQWorldData::ZoneArray`; zone IDs are below this.
pub const MAX_ZONES: usize = 1000;

// Target, pet, raid, extended target and aggro (eqlib `game/Constants.h`,
// `game/EQClasses.h`).
/// Buff slots in the target and pet windows (`CTargetWnd::BuffSpellID`,
/// `CPetInfoWnd::Buff`): every long and short buff slot.
pub const MAX_TOTAL_BUFFS: usize = NUM_LONG_BUFFS + NUM_TEMP_BUFFS;
pub const MAX_RAID_SIZE: usize = 72;
pub const MAX_RAID_LOOTERS: usize = 19;
/// Size of `AggroMeterManagerClient::aggroData`.
pub const MAX_AGGRO_METER_SIZE: usize = 27;
/// `aggroData` index of extended target 1 (`AggroDataTypes::AD_xTarget1`);
/// the others follow it.
pub const AD_XTARGET1: usize = 7;

// Skills and alternate abilities (eqlib `game/Constants.h`).
pub const NUM_SKILLS: usize = 100;
/// Owned alternate ability slots (`PcProfile::AAList`).
pub const AA_CHAR_MAX_REAL: usize = 300;
/// Alternate ability indexes (`AltAdvManager::GetAAById`) are below this.
pub const NUM_ALT_ABILITIES: usize = 49999;

// The game's string table (eqlib `game/EQClasses.h`, `enum eDatabaseStringType`).
pub const DB_STRING_ALT_ABILITY_NAME: i32 = 1;
pub const DB_STRING_ALT_ABILITY_DESCRIPTION: i32 = 4;
pub const DB_STRING_ALT_ABILITY_CATEGORY: i32 = 19;
pub const DB_STRING_EXPANSION_NAME: i32 = 20;
pub const DB_STRING_MERCENARY_STANCE_NAME: i32 = 24;

// EQ UI windows (eqlib `game/XMLData.h`, `enum UIType`): what kind of window
// `CXWnd::GetType` says one is. Only the kinds a window can be are here; the
// rest of the enum names other parts of the UI's XML.
pub const UI_UNKNOWN: i32 = -1;
pub const UI_STATIC_SCREEN_PIECE: i32 = 15;
pub const UI_STATIC_ANIMATION: i32 = 16;
pub const UI_STATIC_TINTED_BLEND_ANIMATION: i32 = 17;
pub const UI_STATIC_TEXT: i32 = 18;
pub const UI_STATIC_FRAME: i32 = 19;
pub const UI_STATIC_HEADER: i32 = 20;
pub const UI_LISTBOX: i32 = 28;
pub const UI_BUTTON: i32 = 29;
pub const UI_GAUGE: i32 = 30;
pub const UI_SPELL_GEM: i32 = 31;
pub const UI_HTML_COMPONENT: i32 = 32;
pub const UI_INV_SLOT: i32 = 33;
pub const UI_EDIT_BOX: i32 = 34;
pub const UI_SLIDER: i32 = 35;
pub const UI_LABEL: i32 = 36;
pub const UI_STML_BOX: i32 = 37;
pub const UI_TREE_VIEW: i32 = 38;
pub const UI_COMBOBOX: i32 = 39;
pub const UI_PAGE: i32 = 40;
pub const UI_TAB_BOX: i32 = 41;
pub const UI_LAYOUT_BOX: i32 = 42;
pub const UI_HORIZONTAL_LAYOUT_BOX: i32 = 43;
pub const UI_VERTICAL_LAYOUT_BOX: i32 = 44;
pub const UI_FINDER_BOX: i32 = 45;
pub const UI_TILE_LAYOUT_BOX: i32 = 46;
pub const UI_SCREEN: i32 = 49;
pub const UI_HOT_BUTTON: i32 = 53;

// Window notifications (eqlib `game/CXWnd.h`, `enum eWndNotification`): the
// messages `CXWnd::WndNotification` gets, the ones `/notify` can send.
pub const XWM_LCLICK: u32 = 1;
pub const XWM_LMOUSEUP: u32 = 2;
pub const XWM_RCLICK: u32 = 3;
pub const XWM_HITENTER: u32 = 6;
pub const XWM_QMARKBOX: u32 = 9;
pub const XWM_CLOSE: u32 = 10;
pub const XWM_NEWVALUE: u32 = 14;
pub const XWM_MENUSELECT: u32 = 20;
pub const XWM_MOUSEOVER: u32 = 21;
pub const XWM_HISTORY: u32 = 22;
pub const XWM_LCLICKHOLD: u32 = 23;
pub const XWM_MAXIMIZEBOX: u32 = 29;

/// Open context menus at once (eqlib `game/UI.h`): the size of
/// `CContextMenuManager::pCurrMenus`.
pub const MAX_CONTEXT_MENU_DEPTH: usize = 8;
