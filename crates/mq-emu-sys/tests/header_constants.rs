//! Checks mq-emu-sys's mirrored constants against the MacroQuest headers in `$MQ_ROOT`.

use std::path::PathBuf;

fn read_header(relative: &str) -> String {
    let path = PathBuf::from(env!("MQ_ROOT")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Finds `#define NAME value` or `constexpr <type> NAME = value;` (or `const <type> NAME = value;`)
/// and returns the raw value text.
fn find_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header.lines().find_map(|line| {
        let line = line.trim();
        let after_name = |text: &'a str| -> Option<&'a str> {
            let pos = text.find(name)?;
            let rest = &text[pos + name.len()..];
            rest.starts_with(|c: char| c.is_whitespace() || c == '=').then_some(rest)
        };

        if let Some(rest) = line.strip_prefix("#define") {
            let rest = after_name(rest.trim_start())?;
            rest.split_whitespace().next()
        } else if line.starts_with("constexpr ") || line.starts_with("const ") {
            let rest = after_name(line)?.trim_start().strip_prefix('=')?;
            Some(rest.split(';').next()?.trim())
        } else {
            None
        }
    })
}

fn parse_int(text: &str) -> i64 {
    let parsed = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => i64::from_str_radix(hex, 16),
        None => text.parse(),
    };
    parsed.unwrap_or_else(|e| panic!("can't parse {text:?} as an integer: {e}"))
}

fn assert_header_value(header: &str, file: &str, name: &str, ours: i64) {
    let raw = find_value(header, name).unwrap_or_else(|| panic!("{name} not found in {file}"));
    assert_eq!(parse_int(raw), ours, "{name}: header {file} says {raw}");
}

macro_rules! check {
    ($header:expr, $file:expr, [$($name:ident),* $(,)?]) => {
        $( assert_header_value(&$header, $file, stringify!($name), mq_emu_sys::constants::$name as i64); )*
    };
}

#[test]
fn common_constants_match() {
    let file = "include/mq/base/Base.h";
    check!(read_header(file), file, [MAX_STRING]);
}

#[test]
fn game_states_match() {
    let file = "src/eqlib/include/eqlib/game/Constants.h";
    check!(read_header(file), file, [
        GAMESTATE_PRECHARSELECT,
        GAMESTATE_CHARSELECT,
        GAMESTATE_CHARCREATE,
        GAMESTATE_POSTCHARSELECT,
        GAMESTATE_SOMETHING,
        GAMESTATE_INGAME,
        GAMESTATE_LOGGINGIN,
        GAMESTATE_UNLOADING,
    ]);
}

#[test]
fn chat_colors_match() {
    let file = "src/eqlib/include/eqlib/game/ChatFilters.h";
    check!(read_header(file), file, [
        USERCOLOR_DEFAULT,
        CONCOLOR_GREEN,
        CONCOLOR_WHITE,
        CONCOLOR_RED,
        CONCOLOR_YELLOW,
        CONCOLOR_LIGHTBLUE,
    ]);
}

#[test]
fn game_limits_match() {
    let file = "src/eqlib/include/eqlib/game/Constants.h";
    check!(read_header(file), file, [
        ITEMTYPE_PACK,
        MAX_AUG_SOCKETS,
        NUM_BANK_SLOTS,
        NUM_SHAREDBANK_SLOTS,
        NUM_LONG_BUFFS,
        NUM_TEMP_BUFFS,
        MAX_MEMORIZED_SPELLS,
        NUM_SPELL_GEMS,
        NUM_BOOK_SLOTS,
        MAX_GROUP_SIZE,
    ]);
    let file = "src/eqlib/include/eqlib/game/Spells.h";
    check!(read_header(file), file, [TOTAL_SPELL_COUNT, MAX_EFFECT_SLOTS]);
    let file = "src/eqlib/include/eqlib/game/EQClasses.h";
    check!(read_header(file), file, [MAX_ZONES]);
}

#[test]
fn raid_skill_and_alt_ability_limits_match() {
    let file = "src/eqlib/include/eqlib/game/Constants.h";
    let header = read_header(file);
    check!(header, file, [MAX_RAID_SIZE, MAX_RAID_LOOTERS, NUM_SKILLS, AA_CHAR_MAX_REAL, NUM_ALT_ABILITIES]);
    // Defined as sums of other constants, which find_value can't evaluate.
    assert_eq!(find_value(&header, "MAX_TOTAL_BUFFS"), Some("NUM_LONG_BUFFS + NUM_TEMP_BUFFS"));
    assert_eq!(find_value(&header, "MAX_TOTAL_BUFFS_NPC"), Some("MAX_TOTAL_BUFFS"));
    let file = "src/eqlib/include/eqlib/game/EQClasses.h";
    let header = read_header(file);
    check!(header, file, [MAX_AGGRO_METER_SIZE]);
    let aggro = enum_values(&header, "AggroDataTypes");
    assert_eq!(aggro.iter().find(|(n, _)| n == "AD_xTarget1").map(|(_, v)| *v), Some(mq_emu_sys::constants::AD_XTARGET1 as i64));

    let strings = enum_values(&header, "eDatabaseStringType");
    let ours = [
        ("eAltAbilityName", mq_emu_sys::constants::DB_STRING_ALT_ABILITY_NAME),
        ("eAltAbilityDescription", mq_emu_sys::constants::DB_STRING_ALT_ABILITY_DESCRIPTION),
        ("eAltAbilityCategory", mq_emu_sys::constants::DB_STRING_ALT_ABILITY_CATEGORY),
        ("eExpansionName", mq_emu_sys::constants::DB_STRING_EXPANSION_NAME),
        ("eMercenaryStanceName", mq_emu_sys::constants::DB_STRING_MERCENARY_STANCE_NAME),
    ];
    for (name, value) in ours {
        let header_value = strings.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name} not in enum")).1;
        assert_eq!(i64::from(value), header_value, "{name}");
    }
}

#[test]
fn window_constants_match() {
    use mq_emu_sys::constants::*;
    let file = "src/eqlib/include/eqlib/game/XMLData.h";
    let types = enum_values(&read_header(file), "UIType");
    let ours = [
        ("UI_Unknown", UI_UNKNOWN),
        ("UI_StaticScreenPiece", UI_STATIC_SCREEN_PIECE),
        ("UI_StaticAnimation", UI_STATIC_ANIMATION),
        ("UI_StaticTintedBlendAnimation", UI_STATIC_TINTED_BLEND_ANIMATION),
        ("UI_StaticText", UI_STATIC_TEXT),
        ("UI_StaticFrame", UI_STATIC_FRAME),
        ("UI_StaticHeader", UI_STATIC_HEADER),
        ("UI_Listbox", UI_LISTBOX),
        ("UI_Button", UI_BUTTON),
        ("UI_Gauge", UI_GAUGE),
        ("UI_SpellGem", UI_SPELL_GEM),
        ("UI_HtmlComponent", UI_HTML_COMPONENT),
        ("UI_InvSlot", UI_INV_SLOT),
        ("UI_EditBox", UI_EDIT_BOX),
        ("UI_Slider", UI_SLIDER),
        ("UI_Label", UI_LABEL),
        ("UI_STMLBox", UI_STML_BOX),
        ("UI_TreeView", UI_TREE_VIEW),
        ("UI_Combobox", UI_COMBOBOX),
        ("UI_Page", UI_PAGE),
        ("UI_TabBox", UI_TAB_BOX),
        ("UI_LayoutBox", UI_LAYOUT_BOX),
        ("UI_HorizontalLayoutBox", UI_HORIZONTAL_LAYOUT_BOX),
        ("UI_VerticalLayoutBox", UI_VERTICAL_LAYOUT_BOX),
        ("UI_FinderBox", UI_FINDER_BOX),
        ("UI_TileLayoutBox", UI_TILE_LAYOUT_BOX),
        ("UI_Screen", UI_SCREEN),
        ("UI_HotButton", UI_HOT_BUTTON),
    ];
    for (name, value) in ours {
        let header_value = types.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name} not in UIType")).1;
        assert_eq!(i64::from(value), header_value, "{name}");
    }

    // The enum ends in entries defined by expressions (`XWM_USER_DEFINED + 1`)
    // that enum_values can't evaluate; the ones bound come before them.
    let file = "src/eqlib/include/eqlib/game/CXWnd.h";
    let header = read_header(file);
    let start = header.find("enum eWndNotification").expect("enum eWndNotification");
    let end = start + header[start..].find("XWM_USER_DEFINED").expect("XWM_USER_DEFINED");
    let notifications = enum_values(&format!("{}}};", &header[start..end]), "eWndNotification");
    let ours = [
        ("XWM_LCLICK", XWM_LCLICK),
        ("XWM_LMOUSEUP", XWM_LMOUSEUP),
        ("XWM_RCLICK", XWM_RCLICK),
        ("XWM_HITENTER", XWM_HITENTER),
        ("XWM_QMARKBOX", XWM_QMARKBOX),
        ("XWM_CLOSE", XWM_CLOSE),
        ("XWM_NEWVALUE", XWM_NEWVALUE),
        ("XWM_MENUSELECT", XWM_MENUSELECT),
        ("XWM_MOUSEOVER", XWM_MOUSEOVER),
        ("XWM_HISTORY", XWM_HISTORY),
        ("XWM_LCLICKHOLD", XWM_LCLICKHOLD),
        ("XWM_MAXIMIZEBOX", XWM_MAXIMIZEBOX),
        ("XWM_LINK", XWM_LINK),
    ];
    for (name, value) in ours {
        let header_value = notifications.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name} not in eWndNotification")).1;
        assert_eq!(i64::from(value), header_value, "{name}");
    }

    let file = "src/eqlib/include/eqlib/game/UI.h";
    check!(read_header(file), file, [MAX_CONTEXT_MENU_DEPTH, MAX_INV_SLOTS]);
}

#[test]
fn slot_hot_button_and_trade_limits_match() {
    let file = "src/eqlib/include/eqlib/game/Constants.h";
    check!(read_header(file), file, [
        MAX_TRADE_SLOTS,
        NUM_SOCIAL_PAGES,
        SOCIALS_PER_PAGE,
        NUM_HOTBUTTON_WINDOWS,
        NUM_HOTBUTTON_PAGES,
        HOTBUTTONS_PER_PAGE,
    ]);
}

#[test]
fn everquest_version_matches_eqgame_header() {
    let header = read_header("src/eqlib/include/eqlib/offsets/eqgame.h");
    let unquote = |name| {
        let raw = find_value(&header, name).unwrap_or_else(|| panic!("{name} not found in eqgame.h"));
        // find_value stops at whitespace, so read the full quoted string from its start.
        let start = header.find(raw).unwrap() + 1;
        let end = start + header[start..].find('"').unwrap();
        header[start..end].to_owned()
    };
    let expected = format!("{} {}", unquote("__ExpectedVersionDate"), unquote("__ExpectedVersionTime"));
    assert_eq!(mq_emu_sys::EVERQUEST_VERSION, expected);
}

// Negative tests: the comparisons above must actually reject bad values.

#[test]
#[should_panic(expected = "MAX_STRING: header include/mq/base/Base.h says 2048")]
fn wrong_value_is_rejected() {
    let file = "include/mq/base/Base.h";
    assert_header_value(&read_header(file), file, "MAX_STRING", 4096);
}

#[test]
#[should_panic(expected = "GAMESTATE_NOT_REAL not found")]
fn unknown_name_is_rejected() {
    let file = "src/eqlib/include/eqlib/game/Constants.h";
    assert_header_value(&read_header(file), file, "GAMESTATE_NOT_REAL", 0);
}

#[test]
#[should_panic(expected = "can't parse")]
fn non_integer_value_is_rejected() {
    assert_header_value("#define FOO \"text\"", "inline", "FOO", 0);
}

#[test]
fn find_value_handles_every_form() {
    let header = "#define FOO_BAR 0x1f // comment\n#define FOO 7\nconstexpr int BAZ = -1;\nconstexpr int BAZZ = 9;\nconst int QUX = 4;\nconsteval int QUUX = 5;";
    assert_eq!(find_value(header, "FOO"), Some("7"));
    assert_eq!(find_value(header, "FOO_BAR"), Some("0x1f"));
    assert_eq!(find_value(header, "BAZ"), Some("-1"));
    assert_eq!(find_value(header, "QUX"), Some("4"));
    assert_eq!(find_value(header, "QUUX"), None, "not a constant this reads");
    assert_eq!(find_value(header, "MISSING"), None);
    assert_eq!(parse_int("0x1f"), 31);
}

/// Reads `enum Name { A = 0, B, C = 12, ... };` into (name, value) pairs,
/// numbering implicit entries the way C++ does (previous value + 1).
fn enum_values(header: &str, enum_name: &str) -> Vec<(String, i64)> {
    let start = header
        .lines()
        .position(|line| {
            let mut words = line.split_whitespace();
            words.next() == Some("enum") && words.next().map(|w| w.trim_end_matches('{')) == Some(enum_name)
        })
        .unwrap_or_else(|| panic!("enum {enum_name} not found"));

    let mut values = Vec::new();
    let mut next = 0i64;
    for line in header.lines().skip(start + 1) {
        let code = line.split("//").next().unwrap().trim();
        if code.starts_with('}') {
            return values;
        }
        let entry = code.trim_end_matches(',').trim();
        if entry.is_empty() || entry == "{" {
            continue;
        }
        let (name, value) = match entry.split_once('=') {
            Some((name, value)) => (name.trim(), parse_int(value.trim())),
            None => (entry, next),
        };
        values.push((name.to_owned(), value));
        next = value + 1;
    }
    panic!("enum {enum_name} isn't closed");
}

#[test]
fn keybind_commands_match() {
    let file = "src/eqlib/include/eqlib/game/Constants.h";
    let header = read_header(file);
    // `nEQMappableCommands` in the header; our name is Rust-style.
    assert_header_value(&header, file, "nEQMappableCommands", mq_emu_sys::constants::N_EQ_MAPPABLE_COMMANDS as i64);
    assert_header_value(&header, file, "nNormalEQMappableCommands", mq_emu_sys::constants::N_NORMAL_EQ_MAPPABLE_COMMANDS as i64);

    let values = enum_values(&header, "KeybindCommand");
    let ours = [
        ("CMD_AUTORUN", mq_emu_sys::constants::CMD_AUTORUN),
        ("CMD_JUMP", mq_emu_sys::constants::CMD_JUMP),
        ("CMD_FORWARD", mq_emu_sys::constants::CMD_FORWARD),
        ("CMD_BACK", mq_emu_sys::constants::CMD_BACK),
        ("CMD_RIGHT", mq_emu_sys::constants::CMD_RIGHT),
        ("CMD_LEFT", mq_emu_sys::constants::CMD_LEFT),
        ("CMD_STRAFE_LEFT", mq_emu_sys::constants::CMD_STRAFE_LEFT),
        ("CMD_STRAFE_RIGHT", mq_emu_sys::constants::CMD_STRAFE_RIGHT),
        ("CMD_DUCK", mq_emu_sys::constants::CMD_DUCK),
        ("CMD_RUN_WALK", mq_emu_sys::constants::CMD_RUN_WALK),
    ];
    for (name, value) in ours {
        let header_value = values.iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name} not in enum")).1;
        assert_eq!(i64::from(value), header_value, "{name}");
    }
    assert_eq!(values.len(), ours.len(), "KeybindCommand has entries we don't mirror: {values:?}");
}

#[test]
fn enum_values_number_implicit_entries() {
    let header = "enum Keys\n{\n\tA = 0,\n\tB,\n\tC, // comment\n\n\tD = 12,\n\tE = 0x20,\n\tF\n};";
    let values = enum_values(header, "Keys");
    let expected: Vec<(String, i64)> =
        [("A", 0), ("B", 1), ("C", 2), ("D", 12), ("E", 32), ("F", 33)].iter().map(|(n, v)| (n.to_string(), *v)).collect();
    assert_eq!(values, expected);
}

#[test]
#[should_panic(expected = "enum Missing not found")]
fn missing_enum_is_rejected() {
    enum_values("enum Other { A };", "Missing");
}

/// Reads eqdata/dikeys.h's `{"Name", code},` rows, undoing the C escapes it uses.
fn di_keys(header: &str) -> Vec<(String, i64)> {
    header
        .lines()
        .filter_map(|line| {
            let row = line.trim().strip_prefix("{\"")?;
            let (name, rest) = row.split_once("\",")?;
            let code = rest.trim().trim_end_matches(',').trim_end_matches('}').trim();
            Some((name.replace("\\\\", "\\"), parse_int(code)))
        })
        .collect()
}

fn assert_di_keys(header: &str, ours: &[(&str, u8)]) {
    let theirs = di_keys(header);
    let ours: Vec<(String, i64)> = ours.iter().map(|(name, code)| ((*name).to_owned(), i64::from(*code))).collect();
    assert_eq!(ours, theirs, "DI_KEYS differs from dikeys.h");
}

#[test]
fn key_names_match() {
    let header = read_header("src/eqlib/include/eqdata/dikeys.h");
    assert_di_keys(&header, &mq_emu_sys::constants::DI_KEYS);
    // What mq-emu's key lookups rely on.
    let keys = &mq_emu_sys::constants::DI_KEYS;
    for (i, (name, code)) in keys.iter().enumerate() {
        assert_ne!(*code, 0, "{name}: 0 ends MacroQuest's table");
        for (other, other_code) in &keys[i + 1..] {
            assert!(!name.eq_ignore_ascii_case(other), "{name} twice");
            assert_ne!(code, other_code, "{name} and {other} share a code");
        }
    }
}

#[test]
fn di_keys_reads_escapes() {
    let header = "\t{\"1\",              2},\n\t{\"\\\\\",             43},\n\t{ 0, 0 }";
    assert_eq!(di_keys(header), [("1".to_owned(), 2), ("\\".to_owned(), 43)]);
}

#[test]
#[should_panic(expected = "DI_KEYS differs from dikeys.h")]
fn a_wrong_key_code_is_rejected() {
    let header = read_header("src/eqlib/include/eqdata/dikeys.h");
    let mut keys = mq_emu_sys::constants::DI_KEYS;
    keys[0].1 += 1;
    assert_di_keys(&header, &keys);
}

#[test]
#[should_panic(expected = "DI_KEYS differs from dikeys.h")]
fn a_missing_key_is_rejected() {
    let header = read_header("src/eqlib/include/eqdata/dikeys.h");
    let keys = mq_emu_sys::constants::DI_KEYS;
    assert_di_keys(&header, &keys[1..]);
}
