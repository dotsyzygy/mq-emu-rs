//! Chat color codes.
//!
//! MacroQuest colors chat with a BEL character (`\a`, byte 7) followed by a
//! color letter; in its C++ source that's written `"\ag"`. Rust has no `\a`
//! escape (a literal `\\ag` just prints), so use these constants:
//!
//! ```ignore
//! use mq_emu::color::{GREEN, RESET};
//! mq.chat(format!("{GREEN}ready{RESET}"));
//! ```

pub const RESET: &str = "\x07x";

pub const WHITE: &str = "\x07w";
pub const BLACK: &str = "\x07b";
pub const RED: &str = "\x07r";
pub const GREEN: &str = "\x07g";
pub const BLUE: &str = "\x07u";
pub const YELLOW: &str = "\x07y";
pub const ORANGE: &str = "\x07o";
pub const TEAL: &str = "\x07t";
pub const MAGENTA: &str = "\x07m";
pub const PURPLE: &str = "\x07p";

pub const DARK_WHITE: &str = "\x07-w";
pub const DARK_RED: &str = "\x07-r";
pub const DARK_GREEN: &str = "\x07-g";
pub const DARK_BLUE: &str = "\x07-u";
pub const DARK_YELLOW: &str = "\x07-y";
pub const DARK_ORANGE: &str = "\x07-o";
pub const DARK_TEAL: &str = "\x07-t";
pub const DARK_MAGENTA: &str = "\x07-m";
pub const DARK_PURPLE: &str = "\x07-p";

/// An exact color, e.g. `rgb(0xff, 0x80, 0x00)`.
pub fn rgb(r: u8, g: u8, b: u8) -> String {
    format!("\x07#{r:02x}{g:02x}{b:02x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: &[&str] = &[
        RESET, WHITE, BLACK, RED, GREEN, BLUE, YELLOW, ORANGE, TEAL, MAGENTA, PURPLE, DARK_WHITE, DARK_RED, DARK_GREEN,
        DARK_BLUE, DARK_YELLOW, DARK_ORANGE, DARK_TEAL, DARK_MAGENTA, DARK_PURPLE,
    ];

    #[test]
    fn codes_use_the_bel_character_not_a_backslash() {
        // The bug this module exists for: "\\ag" prints literally in game.
        for code in ALL {
            assert!(code.starts_with('\u{7}'), "{code:?}");
            assert!(!code.contains('\\'), "{code:?}");
        }
    }

    #[test]
    fn letters_match_macroquest() {
        // From MacroQuest's chat parser (MQ2Utilities.cpp): u is blue, b is black.
        assert_eq!(BLUE, "\u{7}u");
        assert_eq!(BLACK, "\u{7}b");
        assert_eq!(DARK_GREEN, "\u{7}-g");
    }

    #[test]
    fn rgb_is_six_hex_digits() {
        assert_eq!(rgb(0xff, 0x08, 0x00), "\u{7}#ff0800");
    }
}
