//! `Ini` against real files through Windows' INI functions, the ones
//! MacroQuest's helpers wrap. Each guard has a negative test showing what
//! Windows does without it.

use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

/// A fresh file path in its own temporary directory, removed when dropped.
struct TempIni {
    dir: PathBuf,
    ini: Ini,
}

impl TempIni {
    fn new() -> TempIni {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!("mq-emu-ini-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).unwrap();
        let ini = Ini::new(dir.join("Test.ini")).unwrap();
        TempIni { dir, ini }
    }

    /// A file with these exact contents, as a user or another program wrote it.
    fn with(contents: &str) -> TempIni {
        let temp = TempIni::new();
        std::fs::write(temp.ini.path(), contents).unwrap();
        temp
    }

    fn contents(&self) -> String {
        std::fs::read_to_string(self.ini.path()).unwrap()
    }

    /// Reads with the Windows function alone, as `${Ini}` does: a missing key
    /// gives the default.
    fn raw_get(&self, section: &str, key: &str, size: usize) -> String {
        let (section, key) = (CString::new(section).unwrap(), CString::new(key).unwrap());
        let mut out = vec![0u8; size];
        let len = unsafe {
            GetPrivateProfileStringA(section.as_ptr(), key.as_ptr(), c"".as_ptr(), out.as_mut_ptr().cast(), size as u32, self.ini.ansi_path.as_ptr())
        };
        String::from_utf8_lossy(&out[..len as usize]).into_owned()
    }

    fn raw_set(&self, section: &str, key: &str, value: &str) {
        let (section, key, value) = (CString::new(section).unwrap(), CString::new(key).unwrap(), CString::new(value).unwrap());
        assert_ne!(unsafe { WritePrivateProfileStringA(section.as_ptr(), key.as_ptr(), value.as_ptr(), self.ini.ansi_path.as_ptr()) }, 0);
    }
}

impl Drop for TempIni {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl std::ops::Deref for TempIni {
    type Target = Ini;
    fn deref(&self) -> &Ini {
        &self.ini
    }
}

#[test]
fn values_written_read_back() {
    let ini = TempIni::new();
    ini.set("Settings", "Radius", 42.5).unwrap();
    ini.set("Settings", "Name", "Fippy Darkpaw").unwrap();
    ini.set("Other", "Enabled", true).unwrap();
    assert_eq!(ini.get("Settings", "Radius").as_deref(), Some("42.5"));
    assert_eq!(ini.get("Settings", "Name").as_deref(), Some("Fippy Darkpaw"));
    assert_eq!(ini.get_bool("Other", "Enabled"), Some(true));
    assert_eq!(ini.contents(), "[Settings]\r\nRadius=42.5\r\nName=Fippy Darkpaw\r\n[Other]\r\nEnabled=true\r\n");

    ini.set("Settings", "Radius", 10).unwrap();
    assert_eq!(ini.get_parsed::<i32>("Settings", "Radius"), Some(10), "overwritten in place");
}

#[test]
fn names_are_case_insensitive() {
    let ini = TempIni::new();
    ini.set("Settings", "Radius", 1).unwrap();
    assert_eq!(ini.get("SETTINGS", "radius").as_deref(), Some("1"));
    ini.set("settings", "RADIUS", 2).unwrap();
    assert_eq!(ini.section("Settings"), [("Radius".to_owned(), "2".to_owned())], "the existing key is updated");
}

#[test]
fn missing_keys_files_and_sections_are_none_but_empty_values_are_not() {
    let ini = TempIni::with("[Settings]\r\nEmpty=\r\nSpaces=   \r\n");
    assert_eq!(ini.get("Settings", "Empty").as_deref(), Some(""));
    assert_eq!(ini.get("Settings", "Spaces").as_deref(), Some(""));
    assert_eq!(ini.get("Settings", "Missing"), None);
    assert_eq!(ini.get("Missing", "Empty"), None);
    assert_eq!(ini.get_parsed::<i32>("Settings", "Empty"), None);

    let nowhere = Ini::new(ini.dir.join("Missing.ini")).unwrap();
    assert_eq!(nowhere.get("Settings", "Empty"), None);
    assert!(nowhere.sections().is_empty() && nowhere.keys("Settings").is_empty());
    assert!(!nowhere.path().exists(), "reading doesn't create the file");
}

#[test]
fn without_the_key_check_missing_and_empty_look_the_same() {
    let ini = TempIni::with("[Settings]\r\nEmpty=\r\n");
    assert_eq!(ini.raw_get("Settings", "Empty", 64), ini.raw_get("Settings", "Missing", 64));
}

#[test]
fn values_are_read_as_macroquest_reads_them() {
    let ini = TempIni::with("[S]\r\n  Padded  =  some value  \r\nQuoted=\"in quotes\"\r\nSingle='single'\r\nHalf=\"half\r\n");
    for (key, expected) in [("Padded", "some value"), ("Quoted", "in quotes"), ("Single", "single"), ("Half", "\"half")] {
        assert_eq!(ini.get("S", key).as_deref(), Some(expected), "{key}");
        assert_eq!(ini.raw_get("S", key, 64), expected, "{key} through Windows alone");
    }
    assert_eq!(ini.keys("S"), ["Padded", "Quoted", "Single", "Half"]);
}

#[test]
fn values_that_reading_would_change_are_quoted() {
    let ini = TempIni::new();
    let values = ["  padded  ", "\"quoted\"", "'single'", "\"\"", "\"half", "\"", " ", ""];
    for (i, value) in values.iter().enumerate() {
        ini.set("S", &format!("K{i}"), value).unwrap();
    }
    for (i, value) in values.iter().enumerate() {
        assert_eq!(ini.get("S", &format!("K{i}")).as_deref(), Some(*value), "{value:?}");
        assert_eq!(ini.raw_get("S", &format!("K{i}"), 64), *value, "{value:?} through Windows alone, as ${{Ini}} reads it");
    }
    assert!(ini.contents().contains("K0=\"  padded  \"\r\n") && ini.contents().contains("K5=\"\r\n"));
}

#[test]
fn without_quoting_windows_changes_such_values() {
    let ini = TempIni::new();
    ini.raw_set("S", "Padded", "  padded  ");
    ini.raw_set("S", "Quoted", "\"quoted\"");
    assert_eq!(ini.get("S", "Padded").as_deref(), Some("padded"));
    assert_eq!(ini.get("S", "Quoted").as_deref(), Some("quoted"));
}

#[test]
fn long_values_and_lists_are_read_whole() {
    let ini = TempIni::new();
    let long = "x".repeat(10_000);
    ini.set("S", "Long", &long).unwrap();
    assert_eq!(ini.get("S", "Long").as_deref(), Some(long.as_str()));

    let keys: Vec<String> = (0..1000).map(|i| format!("Key{i}")).collect();
    ini.set_section("Many", keys.iter().map(|k| (k, 1))).unwrap();
    assert_eq!(ini.keys("Many"), keys);
    let sections: Vec<String> = (0..500).map(|i| format!("Section{i}")).collect();
    for section in &sections {
        ini.set(section, "K", 1).unwrap();
    }
    assert_eq!(ini.sections()[2..], sections[..]);
}

#[test]
fn without_growing_macroquests_buffer_cuts_long_values() {
    let ini = TempIni::new();
    ini.set("S", "Long", "x".repeat(10_000)).unwrap();
    assert_eq!(ini.raw_get("S", "Long", FIRST_BUFFER).len(), FIRST_BUFFER - 1);
}

#[test]
fn utf8_round_trips() {
    let ini = TempIni::new();
    ini.set("Sé", "Kë", "Fïppy — ✓").unwrap();
    assert_eq!(ini.get("Sé", "Kë").as_deref(), Some("Fïppy — ✓"));
    assert_eq!(ini.sections(), ["Sé"]);
    assert_eq!(std::fs::read(ini.path()).unwrap(), "[Sé]\r\nKë=Fïppy — ✓\r\n".as_bytes(), "stored as UTF-8, as MacroQuest stores it");
}

#[test]
fn text_that_isnt_utf8_reads_lossily() {
    let ini = TempIni::new();
    std::fs::write(ini.path(), b"[S]\r\nK=caf\xe9\r\n").unwrap();
    assert_eq!(ini.get("S", "K").as_deref(), Some("caf\u{fffd}"));
}

#[test]
fn bools_follow_macroquests_rules() {
    let ini = TempIni::with(
        "[S]\r\na=TRUE\r\nb=off\r\nc=Yes\r\nd=no\r\ne=1\r\nf=0\r\ng=-3\r\nh=maybe\r\ni= on \r\nj=\r\n",
    );
    let read: Vec<_> = "abcdefghij".chars().map(|k| ini.get_bool("S", &k.to_string())).collect();
    let (t, f) = (Some(true), Some(false));
    assert_eq!(read, [t, f, t, f, t, f, t, None, t, None]);
    assert_eq!(ini.get_bool("S", "missing"), None);
}

#[test]
fn parsed_values_are_trimmed_and_strict() {
    let ini = TempIni::with("[S]\r\nn= 12 \r\nf=1.5\r\nbad=12abc\r\n");
    assert_eq!(ini.get_parsed::<u8>("S", "n"), Some(12));
    assert_eq!(ini.get_parsed::<f32>("S", "f"), Some(1.5));
    assert_eq!(ini.get_parsed::<i32>("S", "bad"), None);
    assert_eq!(ini.get_parsed::<u8>("S", "f"), None);
}

#[test]
fn keys_and_sections_are_removed() {
    let ini = TempIni::new();
    ini.set("A", "One", 1).unwrap();
    ini.set("A", "Two", 2).unwrap();
    ini.set("B", "Three", 3).unwrap();
    ini.remove("A", "one").unwrap();
    ini.remove("A", "Missing").unwrap();
    assert_eq!(ini.section("A"), [("Two".to_owned(), "2".to_owned())]);
    ini.remove_section("a").unwrap();
    ini.remove_section("Missing").unwrap();
    assert_eq!(ini.sections(), ["B"]);
    assert_eq!(ini.contents(), "[B]\r\nThree=3\r\n");
}

#[test]
fn set_section_replaces_a_sections_contents() {
    let ini = TempIni::with("[A]\r\nOld=1\r\n[B]\r\nKeep=2\r\n");
    ini.set_section("A", [("New", " padded "), ("Other", "x")]).unwrap();
    assert_eq!(ini.section("A"), [("New".to_owned(), " padded ".to_owned()), ("Other".to_owned(), "x".to_owned())]);
    assert_eq!(ini.get("B", "Keep").as_deref(), Some("2"));
    ini.set_section("C", Vec::<(&str, i32)>::new()).unwrap();
    assert_eq!(ini.sections(), ["A", "B", "C"]);
}

#[test]
fn names_and_values_that_would_break_the_file_are_refused() {
    let ini = TempIni::new();
    for section in ["", " S", "S ", "a]b", "a\nb", "a\0b"] {
        assert!(matches!(ini.set(section, "K", 1), Err(IniError::InvalidName)), "section {section:?}");
        assert!(matches!(ini.remove_section(section), Err(IniError::InvalidName)), "section {section:?}");
    }
    for key in ["", " K", "K ", "a=b", "[K", ";K", "a\rb", "a\0b"] {
        assert!(matches!(ini.set("S", key, 1), Err(IniError::InvalidName)), "key {key:?}");
        assert!(matches!(ini.remove("S", key), Err(IniError::InvalidName)), "key {key:?}");
    }
    for value in ["a\nb", "a\rb", "a\0b"] {
        assert!(matches!(ini.set("S", "K", value), Err(IniError::InvalidValue)), "value {value:?}");
    }
    let partly_bad = ini.set_section("S", [("Good", "1"), ("Bad", "a\nb")]);
    assert!(matches!(partly_bad, Err(IniError::InvalidValue)));
    assert!(!ini.path().exists(), "nothing was written");
}

#[test]
fn unchecked_names_and_values_break_the_file() {
    let ini = TempIni::new();
    ini.raw_set("S", "a=b", "1");
    ini.raw_set("S", "K", "line\r\n[Injected]\r\nX=1");
    assert_eq!(ini.get("S", "a").as_deref(), Some("b=1"), "the key became `a`");
    assert_eq!(ini.get("S", "K").as_deref(), Some("line"));
    assert_eq!(ini.sections(), ["S", "Injected"]);
}

#[test]
fn reads_with_unwritable_names_find_nothing() {
    let ini = TempIni::with("[S]\r\nK=1\r\n");
    assert_eq!(ini.get("S\0", "K"), None);
    assert_eq!(ini.get("S", "K\0"), None);
    assert!(ini.keys("S\0").is_empty());
}

#[test]
fn writing_where_windows_cant_is_an_io_error() {
    let temp = TempIni::new();
    let ini = Ini::new(temp.dir.join("missing dir").join("Test.ini")).unwrap();
    match ini.set("S", "K", 1) {
        Err(IniError::Io(error)) => assert_eq!(error.kind(), io::ErrorKind::NotFound),
        other => panic!("{other:?}"),
    }
}

#[test]
fn relative_paths_are_taken_from_the_current_directory() {
    let ini = Ini::new("Relative.ini").unwrap();
    assert_eq!(ini.path(), std::env::current_dir().unwrap().join("Relative.ini"));
}

#[test]
fn paths_round_trip_through_the_ansi_code_page() {
    let path = std::env::temp_dir().join("Fïppy.ini");
    let ansi = to_ansi(path.as_os_str()).unwrap();
    let chars: Vec<c_char> = ansi.as_bytes_with_nul().iter().map(|b| *b as c_char).collect();
    assert_eq!(from_ansi(&chars), path);
    assert_eq!(from_ansi(&[0; 4]), PathBuf::new());
}

#[test]
fn paths_the_code_page_cant_hold_are_refused() {
    unsafe extern "system" {
        fn GetACP() -> u32;
    }
    if unsafe { GetACP() } == 65001 {
        return; // UTF-8 holds everything.
    }
    assert!(matches!(Ini::new("C:\\中文\\😀.ini"), Err(IniError::UnrepresentablePath)));
}
