//! Keys and key combinations, named as MacroQuest's `/bind` names them.

use std::fmt;
use std::num::NonZeroU8;
use std::str::FromStr;

use mq_emu_sys::constants::DI_KEYS;

/// A key (or mouse button), by its DirectInput scan code.
///
/// MacroQuest names 133 of them ([`Key::name`]); the game's own bindings may
/// hold others, which can be read but not bound ([`KeyBindError::UnnamedKey`](crate::KeyBindError::UnnamedKey)).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key(NonZeroU8);

impl Key {
    /// The key MacroQuest calls `name`, in any case: `"A"`, `"F1"`, `"Num_5"`,
    /// `"Page_Up"`, `"Mouse_Aux1"`, `";"`...
    pub fn named(name: &str) -> Option<Key> {
        DI_KEYS.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).and_then(|(_, code)| Key::from_scan_code(*code))
    }

    /// The key with this scan code; None for 0, which is no key.
    pub fn from_scan_code(code: u8) -> Option<Key> {
        NonZeroU8::new(code).map(Key)
    }

    pub fn scan_code(self) -> u8 {
        self.0.get()
    }

    /// MacroQuest's name for the key, if it has one.
    pub fn name(self) -> Option<&'static str> {
        DI_KEYS.iter().find(|(_, code)| *code == self.scan_code()).map(|(name, _)| *name)
    }
}

/// The key's name, or its scan code in hex (`0x56`) if MacroQuest has none.
impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => write!(f, "{:#04x}", self.scan_code()),
        }
    }
}

/// A key with the modifiers held with it, or no key at all ([`KeyCombo::CLEAR`]),
/// as one of a key bind's two bindings holds it.
///
/// Written (`Display`) and read (`FromStr`) in `/bind`'s notation:
/// `"shift+ctrl+alt+F1"`, modifiers first in that order, or `"clear"`.
/// Reading takes the words in any order and case, separated by `+` or spaces,
/// as MacroQuest does, but refuses words it doesn't know (MacroQuest skips
/// them). Two kinds of key can't be read back, in MacroQuest either: those
/// whose name contains a `+` (`Volume_+`), and the `Ctrl`, `Shift` and `Alt`
/// keys themselves, whose names read as modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KeyCombo {
    key: Option<Key>,
    alt: bool,
    ctrl: bool,
    shift: bool,
}

impl KeyCombo {
    /// No key: the binding does nothing.
    pub const CLEAR: KeyCombo = KeyCombo { key: None, alt: false, ctrl: false, shift: false };

    /// `key` on its own.
    pub fn new(key: Key) -> KeyCombo {
        KeyCombo { key: Some(key), ..KeyCombo::CLEAR }
    }

    /// With alt held too. A clear combo stays clear.
    pub fn with_alt(self) -> KeyCombo {
        KeyCombo { alt: self.key.is_some(), ..self }
    }

    /// With ctrl held too. A clear combo stays clear.
    pub fn with_ctrl(self) -> KeyCombo {
        KeyCombo { ctrl: self.key.is_some(), ..self }
    }

    /// With shift held too. A clear combo stays clear.
    pub fn with_shift(self) -> KeyCombo {
        KeyCombo { shift: self.key.is_some(), ..self }
    }

    pub fn key(self) -> Option<Key> {
        self.key
    }

    pub fn alt(self) -> bool {
        self.alt
    }

    pub fn ctrl(self) -> bool {
        self.ctrl
    }

    pub fn shift(self) -> bool {
        self.shift
    }

    pub fn is_clear(self) -> bool {
        self.key.is_none()
    }

    /// eqlib's `KeyCombo::Data`: alt, ctrl, shift, scan code.
    pub(crate) fn to_raw(self) -> [u8; 4] {
        [u8::from(self.alt), u8::from(self.ctrl), u8::from(self.shift), self.key.map_or(0, Key::scan_code)]
    }

    /// From eqlib's `KeyCombo::Data`. Modifiers without a key are clear,
    /// as `ParseKeyCombo` would read what `DescribeKeyCombo` writes for them.
    pub(crate) fn from_raw(data: [u8; 4]) -> KeyCombo {
        match Key::from_scan_code(data[3]) {
            Some(key) => KeyCombo { key: Some(key), alt: data[0] != 0, ctrl: data[1] != 0, shift: data[2] != 0 },
            None => KeyCombo::CLEAR,
        }
    }
}

/// `/bind`'s notation, as `DescribeKeyCombo` writes it.
impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(key) = self.key else { return f.write_str("clear") };
        for (held, name) in [(self.shift, "shift+"), (self.ctrl, "ctrl+"), (self.alt, "alt+")] {
            if held {
                f.write_str(name)?;
            }
        }
        write!(f, "{key}")
    }
}

/// Why text isn't a [`KeyCombo`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseKeyComboError {
    /// A word that's neither a modifier nor a key MacroQuest names.
    UnknownWord(String),
    /// Only modifiers, or nothing.
    NoKey,
    /// More than one key.
    TwoKeys,
}

impl fmt::Display for ParseKeyComboError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseKeyComboError::UnknownWord(word) => write!(f, "{word:?} is neither a modifier nor a key"),
            ParseKeyComboError::NoKey => f.write_str("no key, only modifiers"),
            ParseKeyComboError::TwoKeys => f.write_str("more than one key"),
        }
    }
}

impl std::error::Error for ParseKeyComboError {}

/// `/bind`'s notation, as `ParseKeyCombo` reads it (see [`KeyCombo`]).
impl FromStr for KeyCombo {
    type Err = ParseKeyComboError;

    fn from_str(text: &str) -> Result<KeyCombo, ParseKeyComboError> {
        if text.eq_ignore_ascii_case("clear") {
            return Ok(KeyCombo::CLEAR);
        }
        let mut combo = KeyCombo::CLEAR;
        for word in text.split(['+', ' ']).filter(|word| !word.is_empty()) {
            if word.eq_ignore_ascii_case("alt") {
                combo.alt = true;
            } else if word.eq_ignore_ascii_case("ctrl") {
                combo.ctrl = true;
            } else if word.eq_ignore_ascii_case("shift") {
                combo.shift = true;
            } else {
                let key = Key::named(word).ok_or_else(|| ParseKeyComboError::UnknownWord(word.to_owned()))?;
                if combo.key.replace(key).is_some() {
                    return Err(ParseKeyComboError::TwoKeys);
                }
            }
        }
        if combo.key.is_none() {
            return Err(ParseKeyComboError::NoKey);
        }
        Ok(combo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> Key {
        Key::named(name).unwrap()
    }

    #[test]
    fn keys_are_found_by_name_in_any_case() {
        assert_eq!(key("a").scan_code(), 30);
        assert_eq!(key("NUM_5").name(), Some("Num_5"));
        assert_eq!(key("\\").scan_code(), 43);
        assert_eq!(Key::named("Nope"), None);
        assert_eq!(Key::from_scan_code(0), None);
    }

    #[test]
    fn unnamed_keys_show_their_scan_code() {
        // 0x56 is the extra key on European keyboards, which MacroQuest doesn't name.
        let key = Key::from_scan_code(0x56).unwrap();
        assert_eq!(key.name(), None);
        assert_eq!(key.to_string(), "0x56");
    }

    #[test]
    fn combos_are_written_as_bind_writes_them() {
        assert_eq!(KeyCombo::CLEAR.to_string(), "clear");
        assert_eq!(KeyCombo::new(key("F1")).to_string(), "F1");
        // Modifiers always come out shift, ctrl, alt.
        assert_eq!(KeyCombo::new(key("q")).with_alt().with_shift().with_ctrl().to_string(), "shift+ctrl+alt+Q");
        assert_eq!(KeyCombo::new(key("Num_Plus")).with_ctrl().to_string(), "ctrl+Num_Plus");
    }

    #[test]
    fn combos_are_read_as_bind_reads_them() {
        let expected = KeyCombo::new(key("F1")).with_ctrl().with_shift();
        for text in ["ctrl+shift+F1", "F1+SHIFT+Ctrl", "ctrl shift f1", "+ctrl++shift F1+"] {
            assert_eq!(text.parse(), Ok(expected), "{text}");
        }
        assert_eq!("CLEAR".parse(), Ok(KeyCombo::CLEAR));
    }

    #[test]
    fn what_isnt_a_combo_is_refused() {
        assert_eq!("ctrl+blah+A".parse::<KeyCombo>(), Err(ParseKeyComboError::UnknownWord("blah".into())));
        assert_eq!("ctrl+shift".parse::<KeyCombo>(), Err(ParseKeyComboError::NoKey));
        assert_eq!("".parse::<KeyCombo>(), Err(ParseKeyComboError::NoKey));
        assert_eq!("A+B".parse::<KeyCombo>(), Err(ParseKeyComboError::TwoKeys));
        // "clear" only on its own.
        assert_eq!("shift+clear".parse::<KeyCombo>(), Err(ParseKeyComboError::UnknownWord("clear".into())));
    }

    #[test]
    fn every_named_key_reads_back_except_the_two_kinds_bind_cant() {
        let unreadable = ["Volume_+", "Ctrl", "Shift", "Alt"];
        for (name, _) in DI_KEYS {
            let combo = KeyCombo::new(key(name)).with_ctrl().with_alt();
            let back = combo.to_string().parse::<KeyCombo>();
            if unreadable.contains(&name) {
                assert_ne!(back, Ok(combo), "{name} reads back");
            } else {
                assert_eq!(back, Ok(combo), "{name}");
            }
        }
    }

    #[test]
    fn clear_stays_clear_and_raw_data_round_trips() {
        assert!(KeyCombo::CLEAR.with_alt().with_ctrl().with_shift().is_clear());
        assert_eq!(KeyCombo::CLEAR.with_alt().to_raw(), [0; 4]);
        let combo = KeyCombo::new(key("Z")).with_shift();
        assert_eq!(combo.to_raw(), [0, 0, 1, 44]);
        assert_eq!(KeyCombo::from_raw(combo.to_raw()), combo);
        // Modifiers with no key are no binding.
        assert_eq!(KeyCombo::from_raw([1, 1, 1, 0]), KeyCombo::CLEAR);
    }
}
