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
    let file = "include/mq/base/Common.h";
    check!(read_header(file), file, [MAX_STRING]);
}

#[test]
fn game_states_match() {
    let file = "src/main/MQ2Main.h";
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
    let file = "src/eqlib/Constants.h";
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
fn everquest_version_matches_eqgame_header() {
    let header = read_header("src/eqlib/eqgame.h");
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
#[should_panic(expected = "MAX_STRING: header include/mq/base/Common.h says 2048")]
fn wrong_value_is_rejected() {
    let file = "include/mq/base/Common.h";
    assert_header_value(&read_header(file), file, "MAX_STRING", 4096);
}

#[test]
#[should_panic(expected = "GAMESTATE_NOT_REAL not found")]
fn unknown_name_is_rejected() {
    let file = "src/main/MQ2Main.h";
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
