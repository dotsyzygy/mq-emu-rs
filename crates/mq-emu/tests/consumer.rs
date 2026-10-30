//! Builds a plugin the way someone else's project would: a standalone crate
//! outside this workspace, using the setup in README.md, depending on mq-emu by
//! path. `tests/plugin_exports.rs` covers the macro's exports in detail; this
//! covers using it from outside.

use object::read::pe::PeFile32;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn standalone_plugin_crate_builds_a_loadable_dll() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("mq-emu-consumer");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let mq_emu = Path::new(env!("CARGO_MANIFEST_DIR")).display().to_string().replace('\\', "/");
    std::fs::write(
        dir.join("Cargo.toml"),
        format!(
            "[package]\nname = \"mq2-consumer\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
             [lib]\nname = \"MQ2Consumer\"\ncrate-type = [\"cdylib\"]\n\n\
             [dependencies]\nmq-emu = {{ path = \"{mq_emu}\" }}\n\n\
             # Standalone, not part of any enclosing workspace.\n[workspace]\n"
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("src/lib.rs"),
        "#![allow(non_snake_case)]\n\n\
         use mq_emu::{Mq, Plugin};\n\n\
         struct Consumer;\n\n\
         impl Plugin for Consumer {\n    fn new(mq: &Mq<Self>) -> Self {\n        mq.chat(\"loaded\");\n        Consumer\n    }\n}\n\n\
         mq_emu::plugin!(Consumer);\n",
    )
    .unwrap();

    let target_dir = dir.join("target");
    let out = Command::new(env!("CARGO"))
        .args(["build", "--offline", "--target", "i686-pc-windows-msvc"])
        .current_dir(&dir)
        .env("CARGO_TARGET_DIR", &target_dir)
        // Stands in for the .cargo/config.toml in README.md's setup.
        .env("CARGO_ENCODED_RUSTFLAGS", "-C\x1ftarget-feature=+crt-static")
        .output()
        .expect("failed to run cargo");
    assert!(out.status.success(), "plugin build failed:\n{}", String::from_utf8_lossy(&out.stderr));

    let dll = std::fs::read(target_dir.join("i686-pc-windows-msvc/debug/MQ2Consumer.dll")).unwrap();
    let pe = PeFile32::parse(&*dll).unwrap();
    let exports = pe.export_table().unwrap().unwrap().exports().unwrap();
    for name in [&b"InitializePlugin"[..], b"IsBuiltForNext", b"EverQuestVersion"] {
        assert!(exports.iter().any(|export| export.name == Some(name)), "{} not exported", String::from_utf8_lossy(name));
    }
}
