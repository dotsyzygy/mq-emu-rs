//! Cross-checks the generated layouts (from eqlib.pdb) against eqlib's headers,
//! an independent source. MacroQuest's `comment-update` tool annotates members
//! of `[[offsetcomments]]` classes as `/*0x064*/ float Y;`, where the number is
//! the member's offset within its declaring class plus the optional base given
//! as `[[offsetcomments(0x31ec)]]`. Header sizes come from `constexpr size_t
//! Foo_size = 0x...;`.

use std::collections::HashMap;
use std::path::PathBuf;

use mq_emu_sys::layouts::{FIELDS, RECORDS};

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeaderField {
    class: String,
    member: String,
    /// Offset as written in the comment (base already included).
    offset: u64,
    /// The `[[offsetcomments(base)]]` value.
    base: u64,
    location: String,
}

struct OpenClass {
    name: String,
    base: u64,
    depth: i32,
    opened: bool,
}

fn parse_hex(text: &str) -> Option<u64> {
    let text = text.trim();
    let negative = text.starts_with('-');
    let digits = text.trim_start_matches('-');
    let value = match digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    Some(if negative { value.wrapping_neg() } else { value })
}

/// Parses `class [[offsetcomments(0x10)]] Name : ...` into (name, base).
fn parse_class_line(line: &str) -> Option<(String, u64)> {
    let line = line.trim_start();
    let rest = line.strip_prefix("class ").or_else(|| line.strip_prefix("struct "))?;
    let rest = rest.trim_start().strip_prefix("[[offsetcomments")?;
    let (base, rest) = match rest.strip_prefix('(') {
        Some(args) => {
            let end = args.find(')')?;
            (parse_hex(&args[..end])?, args[end + 1..].strip_prefix("]]")?)
        }
        None => (0, rest.strip_prefix("]]")?),
    };
    let name: String = rest.trim_start().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    (!name.is_empty()).then_some((name, base))
}

/// Extracts the member name from the declaration after an offset comment, or
/// `None` for lines that aren't a single plain member.
fn member_name(declaration: &str) -> Option<String> {
    let code = declaration.split("//").next()?.trim();
    let code = code.strip_suffix(';').or_else(|| code.split(';').next())?;
    if code.contains('(') || code.is_empty() {
        return None;
    }
    let code = code.split('=').next()?; // default member initializer
    let code = code.split('[').next()?; // array dimensions
    let code = code.replace("::", "__");
    let code = code.split(':').next()?; // bitfield width
    // `int a, b;` declares two members; the comment only gives the first offset.
    let after_templates = code.rsplit('>').next()?;
    if after_templates.contains(',') {
        return None;
    }
    let name = code.split_whitespace().last()?.trim_start_matches(['*', '&']);
    let valid = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then(|| name.to_owned())
}

fn parse_header(text: &str, file: &str) -> Vec<HeaderField> {
    let mut fields = Vec::new();
    let mut stack: Vec<OpenClass> = Vec::new();
    let mut depth = 0i32;

    for (index, line) in text.lines().enumerate() {
        if let Some((name, base)) = parse_class_line(line) {
            if !line.trim_end().ends_with(';') {
                stack.push(OpenClass { name, base, depth, opened: false });
            }
        }

        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("/*") {
            if let Some((offset, declaration)) = rest.split_once("*/") {
                let declaration = declaration.trim();
                if let (Some(class), Some(offset), false) = (stack.last(), parse_hex(offset), declaration.starts_with("//")) {
                    if let Some(member) = member_name(declaration) {
                        fields.push(HeaderField {
                            class: class.name.clone(),
                            member,
                            offset,
                            base: class.base,
                            location: format!("{file}:{}", index + 1),
                        });
                    }
                }
            }
        }

        let code = line.split("//").next().unwrap_or("");
        depth += code.matches('{').count() as i32 - code.matches('}').count() as i32;
        if let Some(top) = stack.last_mut() {
            if depth > top.depth {
                top.opened = true;
            }
        }
        while stack.last().is_some_and(|top| top.opened && depth <= top.depth) {
            stack.pop();
        }
    }
    fields
}

/// `constexpr size_t Foo_size = 0x1f08;` → ("Foo", 0x1f08)
fn parse_sizes(text: &str) -> Vec<(String, u64)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("constexpr size_t ")?;
            let (name, value) = rest.split_once('=')?;
            let class = name.trim().strip_suffix("_size")?;
            Some((class.to_owned(), parse_hex(value.trim().trim_end_matches(';'))?))
        })
        .collect()
}

#[derive(Debug, Default)]
struct Report {
    matched: usize,
    mismatches: Vec<String>,
}

/// Compares header offsets with generated fields declared in the same class.
fn compare(header: &[HeaderField], generated: &[(&str, &str, u64)]) -> Report {
    let mut by_member: HashMap<(&str, &str), Vec<u64>> = HashMap::new();
    for (declared_in, field, decl_offset) in generated {
        by_member.entry((declared_in, field)).or_default().push(*decl_offset);
    }

    let mut report = Report::default();
    for h in header {
        let Some(offsets) = by_member.get(&(h.class.as_str(), h.member.as_str())) else { continue };
        report.matched += 1;
        for decl_offset in offsets {
            let expected = decl_offset.wrapping_add(h.base);
            if expected != h.offset {
                report.mismatches.push(format!(
                    "{}::{} at {}: header says {:#x}, PDB gives {:#x} (+ base {:#x})",
                    h.class, h.member, h.location, h.offset, decl_offset, h.base
                ));
            }
        }
    }
    report
}

/// Every `.h` under eqlib's public include directory, recursively, as
/// (path relative to that directory, contents).
fn eqlib_headers() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("MQ_ROOT")).join("src/eqlib/include/eqlib");
    let mut headers = Vec::new();
    let mut dirs = vec![root.clone()];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("failed to list {}: {e}", dir.display())) {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|ext| ext == "h") {
                let relative = path.strip_prefix(&root).unwrap().to_string_lossy().replace('\\', "/");
                headers.push((relative, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    headers.sort();
    assert!(!headers.is_empty(), "no headers found under {}", root.display());
    headers
}

fn generated_fields() -> Vec<(&'static str, &'static str, u64)> {
    FIELDS.iter().map(|f| (f.declared_in, f.field, u64::from(f.decl_offset))).collect()
}

#[test]
fn generated_offsets_match_header_comments() {
    let header: Vec<HeaderField> =
        eqlib_headers().iter().flat_map(|(file, text)| parse_header(text, file)).collect();
    let report = compare(&header, &generated_fields());
    eprintln!("{} header members matched generated fields ({} generated fields total)", report.matched, FIELDS.len());

    assert!(report.mismatches.is_empty(), "{} offset mismatches:\n{}", report.mismatches.len(), report.mismatches.join("\n"));
    // Guard against the check silently matching nothing (e.g. a parser regression).
    assert!(report.matched >= 950, "only {} header members matched generated fields", report.matched);
}

#[test]
fn generated_sizes_match_header_constants() {
    let sizes: HashMap<String, u64> = eqlib_headers().iter().flat_map(|(_, text)| parse_sizes(text)).collect();
    let mut checked = 0;
    for record in RECORDS {
        if let Some(size) = sizes.get(record.name) {
            assert_eq!(u64::from(record.size), *size, "{} size", record.name);
            checked += 1;
        }
    }
    // PlayerClient_size and PcClient_size at least.
    assert!(checked >= 2, "only {checked} records had a header size to compare");
}

#[test]
fn virtual_base_lands_where_the_header_says() {
    // CharacterBase is a virtual base of PcClient, tagged [[offsetcomments(0x31ec)]].
    let x = FIELDS.iter().find(|f| f.record == "PcClient" && f.declared_in == "CharacterBase" && f.field == "X").unwrap();
    assert_eq!(x.offset - x.decl_offset, 0x31ec);
}

// Negative tests: the parser and comparison must reject bad input.

const SAMPLE: &str = r#"
class [[offsetcomments]] Plain : public Base
{
public:
/*0x10*/ float             JumpStrength;
/*0x14*/ eqlib::Foo        Bits : 3;            // bitfield
/*0x18*/ char              Name[64] = {};       // array with initializer
/*0x58*/ // void*          commentedOut;
/*0x5c*/ void              (*Callback)(int);
/*0x60*/ int               a, b;
	struct
	{
/*0x64*/ int               InsideAnonymous;
	};
	EQLIB_OBJECT void Method();
};

class [[offsetcomments(0x31ec)]] Shifted
{
/*0x31f0*/ int             Moved;
};

/*0x99*/ int              OutsideAnyClass;
"#;

fn sample() -> Vec<HeaderField> {
    parse_header(SAMPLE, "sample.h")
}

#[test]
fn parser_reads_members_bases_and_anonymous_structs() {
    let fields = sample();
    let found: Vec<(&str, &str, u64, u64)> =
        fields.iter().map(|f| (f.class.as_str(), f.member.as_str(), f.offset, f.base)).collect();
    assert_eq!(
        found,
        [
            ("Plain", "JumpStrength", 0x10, 0),
            ("Plain", "Bits", 0x14, 0),
            ("Plain", "Name", 0x18, 0),
            ("Plain", "InsideAnonymous", 0x64, 0),
            ("Shifted", "Moved", 0x31f0, 0x31ec),
        ]
    );
}

#[test]
fn wrong_offset_is_a_mismatch() {
    let report = compare(&sample(), &[("Plain", "JumpStrength", 0x14)]);
    assert_eq!(report.matched, 1);
    assert_eq!(report.mismatches.len(), 1, "{:?}", report.mismatches);
    assert!(report.mismatches[0].contains("header says 0x10, PDB gives 0x14"));
}

#[test]
fn base_offset_is_applied() {
    assert!(compare(&sample(), &[("Shifted", "Moved", 0x4)]).mismatches.is_empty());
    assert_eq!(compare(&sample(), &[("Shifted", "Moved", 0x31f0)]).mismatches.len(), 1);
}

#[test]
fn members_of_other_classes_are_not_compared() {
    let report = compare(&sample(), &[("Other", "JumpStrength", 0x99)]);
    assert_eq!(report.matched, 0);
    assert!(report.mismatches.is_empty());
}

#[test]
fn sizes_parse_hex_and_decimal() {
    let sizes = parse_sizes("constexpr size_t PlayerClient_size = 0x1f08;\nconstexpr size_t Small_size = 12;\nconstexpr int Other = 1;");
    assert_eq!(sizes, [("PlayerClient".to_owned(), 0x1f08), ("Small".to_owned(), 12)]);
}
