//! Checks mq-emu-sys's mirrored constants against the MacroQuest headers in `$MQ_ROOT`.

use std::path::PathBuf;

fn read_header(relative: &str) -> String {
    let path = PathBuf::from(env!("MQ_ROOT")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Finds `#define NAME value` or `constexpr <type> NAME = value;` and returns the raw value text.
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
        } else if line.starts_with("constexpr") {
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
fn find_value_handles_both_forms() {
    let header = "#define FOO_BAR 0x1f // comment\n#define FOO 7\nconstexpr int BAZ = -1;\nconstexpr int BAZZ = 9;";
    assert_eq!(find_value(header, "FOO"), Some("7"));
    assert_eq!(find_value(header, "FOO_BAR"), Some("0x1f"));
    assert_eq!(find_value(header, "BAZ"), Some("-1"));
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
