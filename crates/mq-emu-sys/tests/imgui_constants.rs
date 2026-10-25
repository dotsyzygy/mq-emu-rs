//! Checks mq-emu-sys's ImGui constants against MacroQuest's `imgui.h`: every
//! value matches, and every public name of the bound enums is bound.

use std::collections::BTreeMap;
use std::path::PathBuf;

use mq_emu_sys::imgui::ALL_CONSTANTS;

/// The enums mq-emu-sys binds, by name prefix.
const ENUMS: &[&str] = &[
    "ImGuiWindowFlags_",
    "ImGuiChildFlags_",
    "ImGuiCond_",
    "ImGuiTableFlags_",
    "ImGuiTableColumnFlags_",
    "ImGuiTableRowFlags_",
    "ImGuiTreeNodeFlags_",
    "ImGuiSelectableFlags_",
    "ImGuiTabBarFlags_",
    "ImGuiTabItemFlags_",
    "ImGuiComboFlags_",
    "ImGuiInputTextFlags_",
    "ImGuiSliderFlags_",
    "ImGuiHoveredFlags_",
    "ImGuiPopupFlags_",
    "ImGuiCol_",
    "ImGuiStyleVar_",
    "ImGuiColorEditFlags_",
];

fn header() -> String {
    let path = PathBuf::from(env!("MQ_ROOT")).join("contrib/imgui/imgui.h");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Removes `#ifndef IMGUI_DISABLE_OBSOLETE_FUNCTIONS ... #endif` blocks
/// (obsolete aliases mq-emu-sys leaves out) and `//` comments.
fn strip(header: &str) -> String {
    let mut out = String::new();
    let mut obsolete = false;
    for line in header.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#ifndef IMGUI_DISABLE_OBSOLETE_FUNCTIONS") {
            obsolete = true;
        } else if obsolete && trimmed.starts_with("#endif") {
            obsolete = false;
        } else if !obsolete && !trimmed.starts_with('#') {
            out.push_str(line.split("//").next().unwrap_or(""));
            out.push('\n');
        }
    }
    out
}

/// Evaluates an enum value as written in imgui.h: numbers, `a << b`, earlier
/// names, and `|` of those, possibly in parentheses.
fn eval(expr: &str, known: &BTreeMap<String, i64>) -> Result<i64, String> {
    let expr = expr.trim().trim_start_matches('(').trim_end_matches(')');
    expr.split('|').try_fold(0, |acc, term| {
        let term = term.trim();
        let value = if let Some((a, b)) = term.split_once("<<") {
            eval_atom(a, known)? << eval_atom(b, known)?
        } else {
            eval_atom(term, known)?
        };
        Ok(acc | value)
    })
}

fn eval_atom(atom: &str, known: &BTreeMap<String, i64>) -> Result<i64, String> {
    let atom = atom.trim();
    if let Some(hex) = atom.strip_prefix("0x").or_else(|| atom.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).map_err(|e| format!("{atom:?}: {e}"))
    } else if atom.starts_with(|c: char| c.is_ascii_digit()) {
        atom.parse().map_err(|e| format!("{atom:?}: {e}"))
    } else {
        known.get(atom).copied().ok_or_else(|| format!("unknown name {atom:?}"))
    }
}

/// Every entry of the enums in `ENUMS`, with its value, in header order
/// (deprecated names left out).
fn header_constants(header: &str) -> Vec<(String, i64)> {
    let text = strip(header);
    let mut known = BTreeMap::new();
    let mut found = Vec::new();
    for prefix in ENUMS {
        let start = text.find(&format!("enum {prefix}")).or_else(|| text.find(&format!("enum {}", &prefix[..prefix.len() - 1])));
        let start = start.unwrap_or_else(|| panic!("enum for {prefix} not found"));
        let body = &text[start..];
        let body = &body[body.find('{').expect("enum body") + 1..body.find("};").expect("enum end")];
        let mut next = 0;
        for entry in body.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (name, value) = match entry.split_once('=') {
                Some((name, expr)) => (name.trim(), eval(expr, &known).unwrap_or_else(|e| panic!("{entry}: {e}"))),
                None => (entry, next),
            };
            next = value + 1;
            // MacroQuest marks some aliases `Name DEPRECATE("...")`.
            if name.contains(char::is_whitespace) {
                continue;
            }
            known.insert(name.to_owned(), value);
            // Names ending in `_` are ImGui's private masks; COUNT isn't a value.
            if name.starts_with(prefix) && !name.ends_with('_') && !name.ends_with("COUNT") {
                found.push((name.to_owned(), value));
            }
        }
    }
    found
}

/// The differences between our constants and the header's.
fn mismatches(ours: &[(&str, i32)], header: &[(String, i64)]) -> Vec<String> {
    let theirs: BTreeMap<&str, i64> = header.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    let ours_by_name: BTreeMap<&str, i32> = ours.iter().copied().collect();
    let mut problems = Vec::new();
    for (name, value) in ours {
        match theirs.get(name) {
            Some(&v) if v == i64::from(*value) => {}
            Some(&v) => problems.push(format!("{name}: ours {value}, imgui.h {v}")),
            None => problems.push(format!("{name}: not in imgui.h")),
        }
    }
    for (name, _) in header {
        if !ours_by_name.contains_key(name.as_str()) {
            problems.push(format!("{name}: in imgui.h but not bound"));
        }
    }
    problems
}

#[test]
fn imgui_constants_match_the_header() {
    let header = header_constants(&header());
    assert!(header.len() > 300, "parsed only {} constants", header.len());
    let problems = mismatches(ALL_CONSTANTS, &header);
    assert!(problems.is_empty(), "{} problem(s):\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn a_wrong_value_a_stray_name_and_a_missing_name_are_all_caught() {
    let header = header_constants(&header());
    let mut ours: Vec<(&str, i32)> = ALL_CONSTANTS.iter().copied().filter(|(n, _)| *n != "ImGuiCond_Once").collect();
    ours.iter_mut().find(|(n, _)| *n == "ImGuiCond_Always").expect("bound").1 = 3;
    ours.push(("ImGuiCond_Sometimes", 99));
    let problems = mismatches(&ours, &header);
    assert_eq!(problems, [
        "ImGuiCond_Always: ours 3, imgui.h 1",
        "ImGuiCond_Sometimes: not in imgui.h",
        "ImGuiCond_Once: in imgui.h but not bound",
    ]);
}

#[test]
fn the_parser_evaluates_every_form_imgui_h_uses() {
    let text = "enum ImGuiCond_ { ImGuiCond_None = 0, ImGuiCond_A = 1 << 3, ImGuiCond_B, ImGuiCond_C = 0x1F,\n\
                ImGuiCond_D = (ImGuiCond_A | ImGuiCond_C), // comment, with a comma\n\
                ImGuiCond_E DEPRECATE(\"x\") = 7, ImGuiCond_Mask_ = 255,\n\
                #ifndef IMGUI_DISABLE_OBSOLETE_FUNCTIONS\n ImGuiCond_Old = 5,\n#endif\n };";
    let mut header = String::new();
    // header_constants wants every enum in ENUMS; give the others empty bodies.
    for prefix in &ENUMS[1..] {
        if *prefix != "ImGuiCond_" {
            header.push_str(&format!("enum {prefix} {{ }};\n"));
        }
    }
    header.push_str(text);
    header.push_str("\nenum ImGuiWindowFlags_ { };");
    let found = header_constants(&header);
    let names: Vec<(&str, i64)> = found.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    assert_eq!(names, [("ImGuiCond_None", 0), ("ImGuiCond_A", 8), ("ImGuiCond_B", 9), ("ImGuiCond_C", 31), ("ImGuiCond_D", 31)]);
}
