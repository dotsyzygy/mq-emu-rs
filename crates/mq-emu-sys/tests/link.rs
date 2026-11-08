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
    let (abi, decl, reference) = match kind {
        SymbolKind::Function => ("C", format!("fn {symbol}();"), format!("{symbol} as *const () as usize")),
        SymbolKind::Static => ("C", format!("static {symbol}: u8;"), format!("(&raw const {symbol}) as usize")),
        SymbolKind::Method => ("thiscall", format!("#[link_name = \"{symbol}\"] fn method();"), "method as *const () as usize".to_owned()),
    };
    let dir_name = format!("{dll}_{}", symbol.replace(|c: char| !c.is_ascii_alphanumeric(), "_"));
    let source = format!(
        "#![allow(non_snake_case, non_upper_case_globals)]\n\
         #[link(name = \"{dll}\")]\n\
         unsafe extern \"{abi}\" {{ {decl} }}\n\
         #[unsafe(no_mangle)]\n\
         pub extern \"C\" fn reference() -> usize {{ {reference} }}\n"
    );

    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("link_tests").join(dir_name);
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
    /// A C++ member function, by its mangled name.
    Method,
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
    // Mangled C++ names (`?...`) get no underscore, and come after the
    // demangled name, in parentheses.
    let found = if symbol.starts_with('?') {
        stderr.contains("unresolved external symbol") && [format!("({symbol})"), format!("(__imp_{symbol})")].iter().any(|s| stderr.contains(s.as_str()))
    } else {
        [format!("_{symbol} "), format!("__imp__{symbol} ")].iter().any(|decorated| stderr.contains(&format!("unresolved external symbol {decorated}")))
    };
    assert!(found, "{symbol} failed for an unexpected reason:\n{stderr}");
}

#[test]
fn real_function_links() {
    assert_links("MQ2Main", SymbolKind::Function, "GetGameState");
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
fn real_eqlib_method_links() {
    assert_links("eqlib", SymbolKind::Method, "?Max_HP@CharacterZoneClient@eqlib@@QAEHH_N@Z");
}

#[test]
fn method_with_wrong_signature_fails_to_link() {
    // Max_HP(int, bool) mangled as Max_HP(int): a signature mistake is a link error.
    assert_unresolved("eqlib", SymbolKind::Method, "?Max_HP@CharacterZoneClient@eqlib@@QAEHH@Z");
}

#[test]
fn bogus_function_fails_to_link() {
    assert_unresolved("MQ2Main", SymbolKind::Function, "GetGameStateTypo");
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
