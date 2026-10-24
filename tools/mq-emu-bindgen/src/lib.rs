//! Generates mq-emu-sys's game-object layouts from `eqlib.pdb`.
//!
//! The PDB is the compiler's own record of every class layout, so it is the
//! source of truth. The output is committed to mq-emu-sys, so neither its
//! users nor docs.rs need the PDB.

use std::path::{Path, PathBuf};

pub mod emit;
pub mod model;

pub use model::Error;

/// The workspace root (this crate lives in `tools/mq-emu-bindgen`).
pub fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().and_then(Path::parent).expect("tools/mq-emu-bindgen is two levels deep").to_path_buf()
}

/// The list of root types to generate.
pub fn roots_file() -> PathBuf {
    workspace_root().join("crates/mq-emu-sys/layouts.txt")
}

/// Where the generated layouts are written.
pub fn output_file() -> PathBuf {
    workspace_root().join("crates/mq-emu-sys/src/layouts/generated.rs")
}

/// `eqlib.pdb` from the MacroQuest build selected by `MQ_ROOT` / `MQ_BUILD_CONFIG`.
pub fn default_pdb() -> Result<PathBuf, Error> {
    let root = std::env::var_os("MQ_ROOT").ok_or_else(|| Error("MQ_ROOT is not set".into()))?;
    let config = std::env::var("MQ_BUILD_CONFIG").unwrap_or_else(|_| "release".into());
    Ok(PathBuf::from(root).join("build/bin").join(config).join("eqlib.pdb"))
}

/// Parses the roots file: one C++ type name per line; `#` starts a comment.
pub fn parse_roots(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Generates the layouts source for `roots` from the PDB at `pdb_path`.
pub fn generate(pdb_path: &Path, roots: &[String]) -> Result<String, Error> {
    let roots: Vec<&str> = roots.iter().map(String::as_str).collect();
    let model = model::build(&pdb_path.to_string_lossy(), &roots)?;
    Ok(emit::emit(&model))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roots_file_ignores_comments_and_blank_lines() {
        let text = "# header\nPlayerClient\n\n  PcClient   # the character\n#Disabled\n";
        assert_eq!(parse_roots(text), ["PlayerClient", "PcClient"]);
    }
}
