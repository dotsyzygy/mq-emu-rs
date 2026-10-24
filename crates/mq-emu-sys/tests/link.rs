//! Link-level tests: compile tiny DLLs that import one symbol from the MQ
//! import libraries, and check that real symbols link and bogus ones don't.
//!
//! `examples/link_check.rs` covers every real import; these tests prove that a
//! bad declaration really does fail the link, for both functions and globals.

use std::path::PathBuf;
use std::process::{Command, Output};

const TARGET: &str = "i686-pc-windows-msvc";

/// Builds a cdylib that references `symbol` from `dll`'s import library.
fn link_one(dll: &str, kind: SymbolKind, symbol: &str) -> Output {
    let (decl, reference) = match kind {
        SymbolKind::Function => (format!("fn {symbol}();"), format!("{symbol} as *const () as usize")),
        SymbolKind::Static => (format!("static {symbol}: u8;"), format!("(&raw const {symbol}) as usize")),
    };
    let source = format!(
        "#![allow(non_snake_case, non_upper_case_globals)]\n\
         #[link(name = \"{dll}\")]\n\
         unsafe extern \"C\" {{ {decl} }}\n\
         #[unsafe(no_mangle)]\n\
         pub extern \"C\" fn reference() -> usize {{ {reference} }}\n"
    );

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("link_tests").join(format!("{dll}_{symbol}"));
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("probe.rs");
    std::fs::write(&src, source).unwrap();

    Command::new(std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
        .args(["--edition", "2024", "--crate-type", "cdylib", "--target", TARGET])
        .arg("-L")
        .arg(format!("native={}", env!("MQ_BIN_DIR")))
        .arg("--out-dir")
        .arg(&dir)
        .arg(&src)
        .output()
        .expect("failed to run rustc")
}

#[derive(Clone, Copy)]
enum SymbolKind {
    Function,
    Static,
}

fn assert_links(dll: &str, kind: SymbolKind, symbol: &str) {
    let out = link_one(dll, kind, symbol);
    assert!(out.status.success(), "{symbol} failed to link:\n{}", String::from_utf8_lossy(&out.stderr));
}

/// Asserts the link fails because `symbol` is unresolved. On x86 the linker
/// reports cdecl names with a leading underscore, either directly (`_Name`) or
/// through the import pointer (`__imp__Name`).
fn assert_unresolved(dll: &str, kind: SymbolKind, symbol: &str) {
    let out = link_one(dll, kind, symbol);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{symbol} unexpectedly linked");
    let found = [format!("_{symbol} "), format!("__imp__{symbol} ")]
        .iter()
        .any(|decorated| stderr.contains(&format!("unresolved external symbol {decorated}")));
    assert!(found, "{symbol} failed for an unexpected reason:\n{stderr}");
}

#[test]
fn real_function_links() {
    assert_links("MQ2Main", SymbolKind::Function, "IsAlias");
}

#[test]
fn real_static_links() {
    assert_links("MQ2Main", SymbolKind::Static, "gbInZone");
}

#[test]
fn real_eqlib_static_links() {
    assert_links("eqlib", SymbolKind::Static, "pLocalPlayer");
}

#[test]
fn bogus_function_fails_to_link() {
    assert_unresolved("MQ2Main", SymbolKind::Function, "IsAliasTypo");
}

#[test]
fn bogus_static_fails_to_link() {
    assert_unresolved("MQ2Main", SymbolKind::Static, "gbInZoneTypo");
}

#[test]
fn symbol_from_wrong_dll_fails_to_link() {
    // pLocalPlayer is exported by eqlib, not MQ2Main.
    assert_unresolved("MQ2Main", SymbolKind::Static, "pLocalPlayer");
}
