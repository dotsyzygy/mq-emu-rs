//! Checks the vtable offsets generated from `eqlib.pdb` (`layouts::VTABLES`,
//! the `VTABLE_*` constants) against the `VirtualFunctionTable` structs eqlib
//! keeps by hand in its headers, with their `/*0x...*/` offset comments.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mq_emu_sys::layouts::{VTABLES, VtableInfo};

/// `class -> (function -> offset)` from every header's `VirtualFunctionTable`.
type HeaderTables = HashMap<String, HashMap<String, u32>>;

fn header_tables() -> HeaderTables {
    let mut tables = HeaderTables::new();
    let root = PathBuf::from(env!("MQ_ROOT")).join("src/eqlib/include/eqlib");
    let mut dirs = vec![root];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "h") {
                parse_header(&path, &mut tables);
            }
        }
    }
    tables
}

/// The name a `class`/`struct` line opens, skipping forward declarations and
/// attributes (`class [[offsetcomments]] EQLIB_OBJECT CXWnd : ...`).
fn opened_class(line: &str) -> Option<&str> {
    let line = line.trim();
    let rest = line.strip_prefix("class ").or_else(|| line.strip_prefix("struct "))?;
    if line.ends_with(';') {
        return None;
    }
    rest.split(|c: char| c.is_whitespace() || c == ':' || c == '{')
        .find(|w| !w.is_empty() && !w.starts_with("[[") && *w != "EQLIB_OBJECT" && *w != "final")
}

fn parse_header(path: &Path, tables: &mut HeaderTables) {
    let text = std::fs::read_to_string(path).unwrap();
    let mut class: Option<&str> = None;
    let mut table: Option<HashMap<String, u32>> = None;
    for line in text.lines() {
        if let Some(entries) = table.as_mut() {
            let trimmed = line.trim();
            if trimmed.starts_with("};") {
                if let Some(owner) = class {
                    tables.entry(owner.to_owned()).or_default().extend(table.take().unwrap());
                }
                table = None;
            } else if let Some(rest) = trimmed.strip_prefix("/*0x") {
                // /*0x124*/ void* SetWindowText;
                let (hex, decl) = rest.split_once("*/").unwrap();
                if let Some(name) = decl.trim().strip_prefix("void*").map(|n| n.trim().trim_end_matches(';')) {
                    entries.insert(name.to_owned(), u32::from_str_radix(hex, 16).unwrap());
                }
            }
            continue;
        }
        match opened_class(line) {
            Some("VirtualFunctionTable") => table = Some(HashMap::new()),
            Some(name) => class = Some(name),
            None => {}
        }
    }
}

/// Rows whose offset disagrees with the header's, and how many rows the
/// headers had an entry for.
fn compare(rows: &[VtableInfo], tables: &HeaderTables) -> (Vec<String>, usize) {
    let mut mismatches = Vec::new();
    let mut compared = 0;
    for row in rows {
        let Some(&expected) = tables.get(row.record).and_then(|t| t.get(row.function)) else { continue };
        compared += 1;
        if row.offset != expected {
            mismatches.push(format!("{}::{}: generated {:#x}, header {expected:#x}", row.record, row.function, row.offset));
        }
    }
    (mismatches, compared)
}

#[test]
fn generated_vtable_offsets_match_eqlibs_headers() {
    let tables = header_tables();
    let (mismatches, compared) = compare(VTABLES, &tables);
    assert!(mismatches.is_empty(), "vtable offsets disagree:\n{}", mismatches.join("\n"));
    // 97 when written (CXWnd has most); a parser that finds nothing must not pass.
    assert!(compared >= 90, "only {compared} vtable entries compared");
}

#[test]
fn a_wrong_offset_is_reported() {
    let tables = header_tables();
    let mut wrong = *VTABLES.iter().find(|r| r.record == "CXWnd" && r.function == "SetWindowText").unwrap();
    wrong.offset += 4;
    let (mismatches, compared) = compare(&[wrong], &tables);
    assert_eq!(compared, 1);
    assert_eq!(mismatches, ["CXWnd::SetWindowText: generated 0x128, header 0x124"]);
}
