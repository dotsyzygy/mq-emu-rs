//! Checks mq-emu-sys's list of plugin exports against what MacroQuest's plugin
//! loader actually looks up (`GetProcAddress(..., "Name")` in
//! `src/main/MQPluginHandler.cpp`), in both directions.

use std::collections::BTreeSet;
use std::path::PathBuf;

use mq_emu_sys::plugin::{CALLBACK_EXPORTS, EXPORT_EVERQUEST_VERSION, EXPORT_IS_BUILT_FOR_NEXT};

/// Looked up by the loader but deliberately not part of our table: the legacy
/// `float MQ2Version` (C++-mangled), which plugins may omit.
const IGNORED: &[&str] = &["?MQ2Version@@3MA"];

/// Every string literal passed as the name to `GetProcAddress`.
fn looked_up_names(source: &str) -> BTreeSet<String> {
    source
        .match_indices("GetProcAddress(")
        .filter_map(|(start, _)| {
            // Up to the end of the statement: arguments like `hModule.get()` contain parens.
            let call = &source[start..];
            let call = &call[..call.find(';')?];
            let open = call.find('"')? + 1;
            let close = open + call[open..].find('"')?;
            Some(call[open..close].to_owned())
        })
        .collect()
}

fn ours() -> BTreeSet<String> {
    CALLBACK_EXPORTS
        .iter()
        .chain([&EXPORT_IS_BUILT_FOR_NEXT, &EXPORT_EVERQUEST_VERSION])
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn documented_exports_match_the_loader() {
    let path = PathBuf::from(env!("MQ_ROOT")).join("src/main/MQPluginHandler.cpp");
    let source = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let mut loader = looked_up_names(&source);
    for name in IGNORED {
        loader.remove(*name);
    }
    let ours = ours();

    let missing: Vec<_> = loader.difference(&ours).collect();
    let extra: Vec<_> = ours.difference(&loader).collect();
    assert!(missing.is_empty(), "the loader looks up exports mq-emu-sys doesn't document: {missing:?}");
    assert!(extra.is_empty(), "mq-emu-sys documents exports the loader never looks up: {extra:?}");
}

#[test]
fn no_duplicate_exports() {
    assert_eq!(ours().len(), CALLBACK_EXPORTS.len() + 2);
}

// Negative tests for the scanner.

#[test]
fn scanner_finds_only_quoted_proc_names() {
    let source = r#"
        auto a = GetProcAddress(hModule.get(), "IsBuiltForNext");
        pPlugin->Pulse = (fMQPulse)GetProcAddress(pPlugin->hModule, "OnPulse");
        auto b = GetProcAddress(pLook->hModule, proc);
        // "NotACall"
    "#;
    let names: Vec<_> = looked_up_names(source).into_iter().collect();
    assert_eq!(names, ["IsBuiltForNext", "OnPulse"]);
}

#[test]
fn a_new_loader_export_would_be_reported() {
    let source = r#"GetProcAddress(m, "OnPulse"); GetProcAddress(m, "OnSomethingNew");"#;
    let loader = looked_up_names(source);
    assert!(loader.difference(&ours()).any(|name| name == "OnSomethingNew"));
}
