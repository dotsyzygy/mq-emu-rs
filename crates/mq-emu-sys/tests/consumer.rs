//! Builds mq-emu-sys the way a downstream crate would: from a separate crate that
//! depends on it by path, with nothing but the settings each test passes in.
//! Each requirement in README.md gets a positive control and a negative test.
//!
//! Isolation: cargo reads `.cargo/config.toml` from `CARGO_HOME` *and* from
//! every parent of the working directory. So the consumer gets an empty
//! `CARGO_HOME`, and lives under this repo's `target/tmp` rather than `%TEMP%`
//! (which sits inside the user profile, next to `~/.cargo/config.toml`). The
//! only parent config left is the repo's, which sets just `[build] target`,
//! and every build here passes `--target` explicitly. If someone adds `[env]`
//! to that config later, `missing_mq_root_fails_with_clear_message` fails.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::OnceLock;

const TARGET_32: &str = "i686-pc-windows-msvc";
const TARGET_64: &str = "x86_64-pc-windows-msvc";

struct Consumer {
    dir: PathBuf,
    cargo_home: PathBuf,
    target_dir: PathBuf,
}

fn scratch_root() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("consumer-tests")
}

/// Creates (once) a minimal standalone crate that depends on mq-emu-sys by path.
fn consumer() -> &'static Consumer {
    static CONSUMER: OnceLock<Consumer> = OnceLock::new();
    CONSUMER.get_or_init(|| {
        let root = scratch_root();
        let dir = root.join("consumer");
        let cargo_home = root.join("cargo-home");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::create_dir_all(&cargo_home).unwrap();

        let mq_emu_sys = Path::new(env!("CARGO_MANIFEST_DIR")).display().to_string().replace('\\', "/");
        std::fs::write(
            dir.join("Cargo.toml"),
            format!(
                "[package]\nname = \"consumer\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
                 [lib]\npath = \"lib.rs\"\n\n\
                 [dependencies]\nmq-emu-sys = {{ path = \"{mq_emu_sys}\" }}\n\n\
                 # Standalone, not part of any enclosing workspace.\n[workspace]\n"
            ),
        )
        .unwrap();
        std::fs::write(dir.join("lib.rs"), "pub use mq_emu_sys::EVERQUEST_VERSION;\n").unwrap();

        Consumer { target_dir: root.join("target"), dir, cargo_home }
    })
}

/// Runs `cargo build` in the consumer crate with only the given MQ settings.
fn build(target: &str, mq_root: Option<&str>) -> Output {
    run_cargo("build", target, mq_root, false)
}

/// Runs `cargo <subcommand>` in the consumer crate. `docs_rs` sets `DOCS_RS=1`,
/// as docs.rs does for its builds.
fn run_cargo(subcommand: &str, target: &str, mq_root: Option<&str>, docs_rs: bool) -> Output {
    let consumer = consumer();
    let cargo = PathBuf::from(env!("CARGO"));
    let mut cmd = Command::new(&cargo);
    cmd.args([subcommand, "--offline", "--target", target])
        .current_dir(&consumer.dir)
        .env("CARGO_HOME", &consumer.cargo_home)
        .env("CARGO_TARGET_DIR", &consumer.target_dir)
        // Use the toolchain's binaries directly; the rustup proxies aren't in the empty CARGO_HOME.
        .env("RUSTC", cargo.with_file_name("rustc.exe"))
        .env("RUSTDOC", cargo.with_file_name("rustdoc.exe"))
        .env_remove("MQ_ROOT")
        .env_remove("MQ_BUILD_CONFIG")
        .env_remove("DOCS_RS");
    if let Some(root) = mq_root {
        cmd.env("MQ_ROOT", root);
    }
    if docs_rs {
        cmd.env("DOCS_RS", "1");
    }
    cmd.output().expect("failed to run cargo")
}

fn assert_fails_with(out: &Output, message: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "build unexpectedly succeeded:\n{stderr}");
    assert!(stderr.contains(message), "expected {message:?} in:\n{stderr}");
}

#[test]
fn builds_with_mq_root_and_32_bit_target() {
    let out = build(TARGET_32, Some(env!("MQ_ROOT")));
    assert!(out.status.success(), "consumer build failed:\n{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn missing_mq_root_fails_with_clear_message() {
    assert_fails_with(&build(TARGET_32, None), "MQ_ROOT is not set");
}

#[test]
fn mq_root_without_build_output_fails_with_clear_message() {
    let empty = scratch_root().join("not-a-macroquest-checkout");
    std::fs::create_dir_all(&empty).unwrap();
    assert_fails_with(&build(TARGET_32, Some(empty.to_str().unwrap())), "MQ2Main.lib not found");
}

#[test]
fn non_32_bit_target_fails_with_clear_message() {
    assert_fails_with(&build(TARGET_64, Some(env!("MQ_ROOT"))), "MacroQuest emu (RoF2), which is 32-bit only");
}

// docs.rs has no MacroQuest build, so build.rs must not require one there.

#[test]
fn docs_build_succeeds_on_docs_rs_without_mq_root() {
    let out = run_cargo("doc", TARGET_32, None, true);
    assert!(out.status.success(), "docs.rs-style doc build failed:\n{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn docs_build_still_needs_mq_root_outside_docs_rs() {
    assert_fails_with(&run_cargo("doc", TARGET_32, None, false), "MQ_ROOT is not set");
}
