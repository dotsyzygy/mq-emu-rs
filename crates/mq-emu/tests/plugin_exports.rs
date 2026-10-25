//! Checks the DLL that `plugin!` produces (examples/minimal_plugin.rs) against
//! what MacroQuest's loader needs: every callback mq-emu-sys documents, the two
//! sentinels, and an `EverQuestVersion` with the right bytes.

use std::collections::HashSet;
use std::path::PathBuf;

use mq_emu::sys::plugin::{CALLBACK_EXPORTS, EXPORT_EVERQUEST_VERSION, EXPORT_IS_BUILT_FOR_NEXT};
use object::read::pe::PeFile32;
use object::{LittleEndian, Object, ObjectSection};

fn plugin_dll() -> Vec<u8> {
    // target/<triple>/<profile>/deps/<this test>.exe -> .../<profile>/examples/minimal_plugin.dll
    let exe = std::env::current_exe().unwrap();
    let profile_dir = exe.parent().and_then(|deps| deps.parent()).unwrap();
    let dll: PathBuf = profile_dir.join("examples").join("minimal_plugin.dll");
    std::fs::read(&dll).unwrap_or_else(|e| panic!("{}: {e} (cargo test builds examples first)", dll.display()))
}

fn exports(file: &PeFile32<'_>) -> HashSet<String> {
    file.export_table()
        .unwrap()
        .unwrap()
        .exports()
        .unwrap()
        .into_iter()
        .filter_map(|e| e.name)
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect()
}

#[test]
fn exports_every_loader_symbol() {
    let image = plugin_dll();
    let file = PeFile32::parse(&*image).unwrap();
    let exports = exports(&file);
    let missing: Vec<_> = CALLBACK_EXPORTS
        .iter()
        .chain([&EXPORT_IS_BUILT_FOR_NEXT, &EXPORT_EVERQUEST_VERSION])
        // GetPluginInterface is optional and mq-emu doesn't provide an interface yet.
        .filter(|name| **name != "GetPluginInterface")
        .filter(|name| !exports.contains(**name))
        .collect();
    assert!(missing.is_empty(), "plugin DLL is missing exports: {missing:?}");
}

#[test]
fn everquest_version_export_holds_the_expected_bytes() {
    let image = plugin_dll();
    let file = PeFile32::parse(&*image).unwrap();
    let export_table = file.export_table().unwrap().unwrap();
    let export = export_table
        .exports()
        .unwrap()
        .into_iter()
        .find(|e| e.name == Some(EXPORT_EVERQUEST_VERSION.as_bytes()))
        .expect("EverQuestVersion exported");
    let object::read::pe::ExportTarget::Address(rva) = export.target else { panic!("forwarded export") };

    let base = file.relative_address_base();
    let address = base + u64::from(rva);
    let expected = mq_emu::sys::EVERQUEST_VERSION_NUL;
    let bytes = file
        .sections()
        .find_map(|section| section.data_range(address, expected.len() as u64).ok().flatten())
        .expect("EverQuestVersion points into a section");
    assert_eq!(bytes, expected);
    let _ = LittleEndian;
}

fn imports(file: &PeFile32<'_>) -> Vec<String> {
    let table = file.import_table().unwrap().unwrap();
    let mut descriptors = table.descriptors().unwrap();
    let mut names = Vec::new();
    while let Some(descriptor) = descriptors.next().unwrap() {
        let name = table.name(descriptor.name.get(LittleEndian)).unwrap();
        names.push(String::from_utf8_lossy(name).to_ascii_lowercase());
    }
    names
}

/// MacroQuest links the C runtime statically, so players needn't have the
/// Visual C++ redistributable; a plugin that needed it could fail to load.
#[test]
fn plugin_needs_no_visual_cpp_redistributable() {
    let image = plugin_dll();
    let file = PeFile32::parse(&*image).unwrap();
    let imports = imports(&file);
    let runtime: Vec<_> = imports
        .iter()
        .filter(|name| name.starts_with("vcruntime") || name.starts_with("msvcp") || name.starts_with("api-ms-win-crt"))
        .collect();
    assert!(runtime.is_empty(), "plugin imports the dynamic C runtime: {runtime:?} (all imports: {imports:?})");
    assert!(imports.contains(&"mq2main.dll".to_owned()), "{imports:?}");
}

// Negative test: a DLL that isn't a plugin lacks the sentinels.
#[test]
fn a_non_plugin_dll_fails_the_same_check() {
    let image = std::fs::read(std::env::current_exe().unwrap()).unwrap();
    let file = PeFile32::parse(&*image).unwrap();
    let exports = file.export_table().ok().flatten().map(|t| {
        t.exports().unwrap().into_iter().filter_map(|e| e.name).map(|n| String::from_utf8_lossy(n).into_owned()).collect::<HashSet<_>>()
    });
    assert!(!exports.unwrap_or_default().contains(EXPORT_IS_BUILT_FOR_NEXT));
}
