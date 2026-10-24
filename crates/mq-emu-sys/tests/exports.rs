//! Checks every symbol mq-emu-sys declares against the export tables of the
//! MacroQuest DLLs it links to. A typo or a symbol removed upstream fails here
//! instead of at plugin load time.
//!
//! These tests only *reference* the name lists, never the imports themselves,
//! so the test binary doesn't need the MQ DLLs to start.

use std::collections::HashSet;
use std::path::PathBuf;

use object::read::pe::PeFile32;
use object::{LittleEndian, pe};

fn bin_dir() -> PathBuf {
    PathBuf::from(env!("MQ_BIN_DIR"))
}

fn read_dll(name: &str) -> Vec<u8> {
    let path = bin_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

fn export_names(image: &[u8]) -> HashSet<String> {
    let file = PeFile32::parse(image).expect("not a 32-bit PE image");
    file.export_table()
        .expect("malformed export table")
        .expect("DLL has no export table")
        .exports()
        .expect("failed to read exports")
        .into_iter()
        .filter_map(|export| export.name)
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

/// Returns the names in `wanted` that `image` does not export.
fn missing_exports<'a>(image: &[u8], wanted: &[&'a str]) -> Vec<&'a str> {
    let exports = export_names(image);
    wanted.iter().copied().filter(|name| !exports.contains(*name)).collect()
}

fn assert_all_exported(dll: &str, declared: &[&str]) {
    let missing = missing_exports(&read_dll(dll), declared);
    assert!(missing.is_empty(), "{dll} does not export: {missing:?}");
}

const PLUGIN_SENTINELS: [&str; 2] = [mq_emu_sys::plugin::EXPORT_IS_BUILT_FOR_NEXT, mq_emu_sys::plugin::EXPORT_EVERQUEST_VERSION];

#[test]
fn mq2main_exports_every_declared_symbol() {
    assert_all_exported("MQ2Main.dll", mq_emu_sys::mq2main::MQ2MAIN_IMPORTS);
}

#[test]
fn eqlib_exports_every_declared_symbol() {
    assert_all_exported("eqlib.dll", mq_emu_sys::eqlib::EQLIB_IMPORTS);
}

#[test]
fn declared_names_are_unique() {
    let all: Vec<_> = mq_emu_sys::mq2main::MQ2MAIN_IMPORTS.iter().chain(mq_emu_sys::eqlib::EQLIB_IMPORTS).collect();
    let unique: HashSet<_> = all.iter().collect();
    assert_eq!(all.len(), unique.len(), "duplicate import declarations");
}

#[test]
fn mq_dlls_are_32_bit() {
    for dll in ["MQ2Main.dll", "eqlib.dll"] {
        let image = read_dll(dll);
        let file = PeFile32::parse(&*image).unwrap_or_else(|e| panic!("{dll} is not a 32-bit PE: {e}"));
        let machine = file.nt_headers().file_header.machine.get(LittleEndian);
        assert_eq!(machine, pe::IMAGE_FILE_MACHINE_I386, "{dll} is not an x86 image");
    }
}

#[test]
fn existing_plugin_exports_the_sentinels_we_document() {
    // Use a C++ plugin from the same build as a reference for what the loader expects.
    let plugins = bin_dir().join("plugins");
    let reference = std::fs::read_dir(&plugins)
        .unwrap_or_else(|e| panic!("failed to list {}: {e}", plugins.display()))
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("dll")))
        .expect("no built plugin DLLs to compare against");

    let missing = missing_exports(&std::fs::read(&reference).unwrap(), &PLUGIN_SENTINELS);
    assert!(missing.is_empty(), "{} does not export: {missing:?}", reference.display());
}

// Negative tests: the checks above must actually reject bad input.

#[test]
fn undeclared_name_is_reported_missing() {
    let missing = missing_exports(&read_dll("MQ2Main.dll"), &["WriteChatf", "IsAliasTypo", "pLocalPlayer"]);
    // pLocalPlayer lives in eqlib.dll, so it must not be found in MQ2Main.dll either.
    assert_eq!(missing, ["IsAliasTypo", "pLocalPlayer"]);
}

#[test]
#[should_panic(expected = "MQ2Main.dll does not export: [\"IsAliasTypo\"]")]
fn assert_all_exported_panics_on_missing_name() {
    assert_all_exported("MQ2Main.dll", &["WriteChatf", "IsAliasTypo"]);
}

#[test]
fn command_functions_are_no_longer_c_exports() {
    // These moved behind MacroQuest's C++ MainInterface (mq/api/CommandAPI.h).
    // If they ever come back as C exports, they can be bound directly again.
    let moved = ["AddCommand", "RemoveCommand", "IsCommand", "HideDoCommand", "TimedCommand", "AddAlias"];
    assert_eq!(missing_exports(&read_dll("MQ2Main.dll"), &moved), moved);
}

#[test]
fn non_plugin_dll_lacks_plugin_sentinels() {
    assert_eq!(missing_exports(&read_dll("MQ2Main.dll"), &PLUGIN_SENTINELS), PLUGIN_SENTINELS);
}
