//! Checks mq-emu-sys's ImGui and ImPlot constants against MacroQuest's
//! `imgui.h`, `imgui_internal.h` and `implot.h` (and MacroQuest's own ImGui
//! widget flags): every value matches, and every public name of the bound
//! enums is bound.

use std::collections::BTreeMap;
use std::path::PathBuf;

use mq_emu_sys::imgui::ALL_CONSTANTS;

/// The enums mq-emu-sys binds, by name prefix; `Enum:Prefix_` for the names
/// with that prefix in `enum Enum`.
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
    "ImGuiButtonFlags_",
    "ImGuiMouseButton_",
    "ImGuiDragDropFlags_",
    "ImGuiDockNodeFlags_",
    "ImDrawFlags_",
    "ImGuiDir_",
    "ImGuiListClipperFlags_",
    "ImGuiMultiSelectFlags_",
    "ImGuiSelectionRequestType_",
    "ImGuiKey:ImGuiMod_",
];

/// The `imgui_internal.h` enums mq-emu-sys binds.
const INTERNAL_ENUMS: &[&str] = &["ImGuiDockNodeFlagsPrivate_:ImGuiDockNodeFlags_"];

/// The ImPlot enums mq-emu-sys binds.
const IMPLOT_ENUMS: &[&str] = &[
    "ImAxis_",
    "ImPlotFlags_",
    "ImPlotAxisFlags_",
    "ImPlotSubplotFlags_",
    "ImPlotLegendFlags_",
    "ImPlotMouseTextFlags_",
    "ImPlotDragToolFlags_",
    "ImPlotColormapScaleFlags_",
    "ImPlotItemFlags_",
    "ImPlotLineFlags_",
    "ImPlotScatterFlags_",
    "ImPlotStairsFlags_",
    "ImPlotShadedFlags_",
    "ImPlotBarsFlags_",
    "ImPlotBarGroupsFlags_",
    "ImPlotErrorBarsFlags_",
    "ImPlotStemsFlags_",
    "ImPlotInfLinesFlags_",
    "ImPlotPieChartFlags_",
    "ImPlotHeatmapFlags_",
    "ImPlotHistogramFlags_",
    "ImPlotDigitalFlags_",
    "ImPlotImageFlags_",
    "ImPlotTextFlags_",
    "ImPlotDummyFlags_",
    "ImPlotCond_",
    "ImPlotCol_",
    "ImPlotStyleVar_",
    "ImPlotScale_",
    "ImPlotMarker_",
    "ImPlotColormap_",
    "ImPlotLocation_",
    "ImPlotBin_",
];

fn read(relative: &str) -> String {
    let path = PathBuf::from(env!("MQ_ROOT")).join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

fn header() -> String {
    read("contrib/imgui/imgui.h")
}

fn internal_header() -> String {
    read("contrib/imgui/imgui_internal.h")
}

fn implot_header() -> String {
    read("src/imgui/implot/implot.h")
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

/// Evaluates an enum value as written in imgui.h and implot.h: numbers
/// (possibly negative), `a << b`, `a - b`, earlier names, and `|` of those,
/// possibly in parentheses.
fn eval(expr: &str, known: &BTreeMap<String, i64>) -> Result<i64, String> {
    let expr = expr.trim().trim_start_matches('(').trim_end_matches(')');
    expr.split('|').try_fold(0, |acc, term| {
        let term = term.trim();
        let value = if let Some((a, b)) = term.split_once("<<") {
            eval_atom(a, known)? << eval_atom(b, known)?
        } else if let Some((a, b)) = term.get(1..).and_then(|rest| rest.split_once('-')) {
            // After the first character, so a leading minus stays a negative number.
            eval_atom(&term[..1 + a.len()], known)? - eval_atom(b, known)?
        } else {
            eval_atom(term, known)?
        };
        Ok(acc | value)
    })
}

fn eval_atom(atom: &str, known: &BTreeMap<String, i64>) -> Result<i64, String> {
    let atom = atom.trim();
    if let Some(negated) = atom.strip_prefix('-') {
        eval_atom(negated, known).map(|v| -v)
    } else if let Some(hex) = atom.strip_prefix("0x").or_else(|| atom.strip_prefix("0X")) {
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
    header_constants_of(header, ENUMS, &mut BTreeMap::new())
}

/// The body of `enum name` in `text` (between its braces), skipping forward
/// declarations (`enum ImGuiDir : int;`) and longer names.
fn enum_body<'t>(text: &'t str, name: &str) -> Option<&'t str> {
    let declaration = format!("enum {name}");
    text.match_indices(&declaration).find_map(|(start, _)| {
        let rest = &text[start + declaration.len()..];
        if rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_') {
            return None;
        }
        let open = rest.find('{')?;
        if rest.find(';').is_some_and(|semicolon| semicolon < open) {
            return None;
        }
        Some(&rest[open + 1..open + rest[open..].find("};")?])
    })
}

/// Every entry of `enums` in `header`. `known` holds the values of names seen
/// so far (ImPlot's refer to ImGui's) and gets this header's.
fn header_constants_of(header: &str, enums: &[&str], known: &mut BTreeMap<String, i64>) -> Vec<(String, i64)> {
    let text = strip(header);
    let mut found = Vec::new();
    for spec in enums {
        let (body, prefix) = match spec.split_once(':') {
            Some((name, prefix)) => (enum_body(&text, name), prefix),
            None => (enum_body(&text, spec).or_else(|| enum_body(&text, &spec[..spec.len() - 1])), *spec),
        };
        let body = body.unwrap_or_else(|| panic!("enum for {spec} not found"));
        let mut next = 0;
        for entry in body.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (name, value) = match entry.split_once('=') {
                Some((name, expr)) => match eval(expr, known) {
                    Ok(value) => (name.trim(), value),
                    // ImGui's private masks use forms this doesn't evaluate (`~0`, casts).
                    Err(_) if name.trim().ends_with('_') => continue,
                    Err(e) => panic!("{entry}: {e}"),
                },
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
            Some(&v) => problems.push(format!("{name}: ours {value}, header {v}")),
            None => problems.push(format!("{name}: not in the header")),
        }
    }
    for (name, _) in header {
        if !ours_by_name.contains_key(name.as_str()) {
            problems.push(format!("{name}: in the header but not bound"));
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
fn implot_constants_match_the_header() {
    let mut known = BTreeMap::new();
    header_constants_of(&header(), &["ImGuiCond_"], &mut known);
    let header = header_constants_of(&implot_header(), IMPLOT_ENUMS, &mut known);
    assert!(header.len() > 200, "parsed only {} constants", header.len());
    let problems = mismatches(mq_emu_sys::implot::ALL_CONSTANTS, &header);
    assert!(problems.is_empty(), "{} problem(s):
{}", problems.len(), problems.join("
"));
}

#[test]
fn internal_dock_node_flags_match_the_header() {
    let mut known = BTreeMap::new();
    header_constants_of(&header(), &["ImGuiDockNodeFlags_"], &mut known);
    let header = header_constants_of(&internal_header(), INTERNAL_ENUMS, &mut known);
    assert!(header.len() > 10, "parsed only {} constants", header.len());
    let problems = mismatches(mq_emu_sys::imgui::INTERNAL_CONSTANTS, &header);
    assert!(problems.is_empty(), "{} problem(s):\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn macroquest_widget_flags_match_their_headers() {
    let widgets = header_constants_of(&read("include/mq/imgui/Widgets.h"), &["ImGuiSpellGemFlags_"], &mut BTreeMap::new());
    let problems = mismatches(mq_emu_sys::imgui_mq::SPELL_GEM_CONSTANTS, &widgets);
    assert!(problems.is_empty(), "{} problem(s):\n{}", problems.len(), problems.join("\n"));
    let graphics = read("src/eqlib/include/eqlib/graphics/GraphicsEngine.h");
    let text = header_constants_of(&graphics, &["DrawTextFlags:DrawText_"], &mut BTreeMap::new());
    let problems = mismatches(mq_emu_sys::imgui_mq::DRAW_TEXT_CONSTANTS, &text);
    assert!(problems.is_empty(), "{} problem(s):\n{}", problems.len(), problems.join("\n"));
}

#[test]
fn a_wrong_implot_value_is_caught() {
    let mut known = BTreeMap::new();
    header_constants_of(&header(), &["ImGuiCond_"], &mut known);
    let header = header_constants_of(&implot_header(), IMPLOT_ENUMS, &mut known);
    let mut ours = mq_emu_sys::implot::ALL_CONSTANTS.to_vec();
    ours.iter_mut().find(|(n, _)| *n == "ImPlotMarker_None").expect("bound").1 = 0;
    assert_eq!(mismatches(&ours, &header), ["ImPlotMarker_None: ours 0, header -1"]);
}

#[test]
fn a_wrong_value_a_stray_name_and_a_missing_name_are_all_caught() {
    let header = header_constants(&header());
    let mut ours: Vec<(&str, i32)> = ALL_CONSTANTS.iter().copied().filter(|(n, _)| *n != "ImGuiCond_Once").collect();
    ours.iter_mut().find(|(n, _)| *n == "ImGuiCond_Always").expect("bound").1 = 3;
    ours.push(("ImGuiCond_Sometimes", 99));
    let problems = mismatches(&ours, &header);
    assert_eq!(problems, [
        "ImGuiCond_Always: ours 3, header 1",
        "ImGuiCond_Sometimes: not in the header",
        "ImGuiCond_Once: in the header but not bound",
    ]);
}

#[test]
fn the_parser_skips_forward_declarations_and_longer_names() {
    let text = "enum ImGuiDir : int; enum ImGuiDirection { ImGuiDir_Wrong = 9 };\n\
                enum ImGuiDir : int { ImGuiDir_None = -1, ImGuiDir_Left = 0, ImGuiDir_COUNT };";
    let found = header_constants_of(text, &["ImGuiDir_"], &mut BTreeMap::new());
    assert_eq!(found, [("ImGuiDir_None".to_owned(), -1), ("ImGuiDir_Left".to_owned(), 0)]);
    assert_eq!(enum_body("enum ImGuiDir : int;", "ImGuiDir"), None);
}

#[test]
fn the_parser_takes_a_prefix_within_an_enum_and_skips_masks_it_cannot_evaluate() {
    let text = "enum ImGuiKey : int { ImGuiKey_None = 0, ImGuiKey_Begin = 512, ImGuiKey_Tab = 512, ImGuiKey_A, ImGuiKey_End,\n\
                ImGuiKey_Count = ImGuiKey_End - ImGuiKey_Begin, ImGuiMod_None = 0, ImGuiMod_Ctrl = 1 << 12,\n\
                ImGuiMod_All_ = ~0, ImGuiMod_Cast_ = (int)ImGuiMod_Ctrl };";
    let mut known = BTreeMap::new();
    let found = header_constants_of(text, &["ImGuiKey:ImGuiMod_"], &mut known);
    assert_eq!(found, [("ImGuiMod_None".to_owned(), 0), ("ImGuiMod_Ctrl".to_owned(), 4096)]);
    assert_eq!(known.get("ImGuiKey_Count"), Some(&2), "a - b");
}

#[test]
#[should_panic(expected = "unknown name")]
fn a_value_the_parser_cannot_evaluate_fails_unless_it_is_a_mask() {
    header_constants_of("enum ImGuiCond_ { ImGuiCond_Bad = ~0 };", &["ImGuiCond_"], &mut BTreeMap::new());
}

#[test]
fn the_parser_evaluates_every_form_imgui_h_uses() {
    let text = "enum ImGuiCond_ { ImGuiCond_None = 0, ImGuiCond_A = 1 << 3, ImGuiCond_B, ImGuiCond_C = 0x1F, ImGuiCond_N = -2,\n\
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
    assert_eq!(names, [
        ("ImGuiCond_None", 0),
        ("ImGuiCond_A", 8),
        ("ImGuiCond_B", 9),
        ("ImGuiCond_C", 31),
        ("ImGuiCond_N", -2),
        ("ImGuiCond_D", 31)
    ]);
}
