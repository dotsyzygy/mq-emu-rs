//! Builds mq-emu-sys the way a downstream crate would: from a separate crate that
//! depends on it by path, with nothing but the settings each test passes in.
//! Each requirement in README.md gets a positive control and a negative test.
//!
//! Isolation: cargo reads `.cargo/config.toml` from `CARGO_HOME` *and* from
//! every parent of the working directory. So the consumer gets an empty
//! `CARGO_HOME`, and lives under this repo's `target/tmp` rather than `%TEMP%`
//! (which sits inside the user profile, next to `~/.cargo/config.toml`). The
//! only parent config left is the repo's, which sets `[build] target` and
//! `crt-static` rustflags; every build here passes `--target` and
//! `CARGO_ENCODED_RUSTFLAGS` explicitly, which override both. If someone adds
//! `[env]` to that config later, `missing_mq_root_fails_with_clear_message`
//! fails.

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
        share_package_cache(&cargo_home);

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

/// The isolated CARGO_HOME has no user config, but builds still need the
/// crates.io packages mq-emu-sys depends on (e.g. `cc`). Link the real package
/// cache in with a directory junction, so `--offline` resolution works without
/// any of the user's config coming along.
fn share_package_cache(cargo_home: &Path) {
    let link = cargo_home.join("registry");
    match std::fs::symlink_metadata(&link) {
        // Already a junction (std reports junctions as symlinks).
        Ok(meta) if meta.file_type().is_symlink() => return,
        // A real directory Cargo created on an earlier run. Remove it file by
        // file; never recursively, in case it ever points somewhere real.
        Ok(_) => {
            let _ = std::fs::remove_file(link.join("CACHEDIR.TAG"));
            std::fs::remove_dir(&link).unwrap_or_else(|e| panic!("{} isn't empty ({e}); delete it by hand", link.display()));
        }
        Err(_) => {}
    }
    let real_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("USERPROFILE").expect("USERPROFILE")).join(".cargo"));
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(real_home.join("registry"))
        .stdout(std::process::Stdio::null())
        .status()
        .expect("failed to run mklink");
    assert!(status.success() || link.exists(), "could not link the package cache into {}", cargo_home.display());
}

/// The static C runtime flag a plugin project is expected to set
/// (space-separated; encoded for CARGO_ENCODED_RUSTFLAGS when used).
const CRT_STATIC: &str = "-C target-feature=+crt-static";

/// Runs `cargo build` in the consumer crate with only the given MQ settings
/// (and the static C runtime, as a correctly configured project would have).
fn build(target: &str, mq_root: Option<&str>) -> Output {
    run_cargo("build", target, mq_root, false, Some(CRT_STATIC), &[])
}

/// Runs `cargo <subcommand>` in the consumer crate. `docs_rs` sets `DOCS_RS=1`,
/// as docs.rs does for its builds; `rustflags` stands in for the project's
/// `.cargo/config.toml` rustflags; `env` adds variables.
fn run_cargo(subcommand: &str, target: &str, mq_root: Option<&str>, docs_rs: bool, rustflags: Option<&str>, env: &[(&str, &str)]) -> Output {
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
        .env_remove("DOCS_RS")
        .env_remove("RUSTFLAGS")
        .env_remove("MQ_EMU_DYNAMIC_CRT")
        // Always set, even to nothing: it takes precedence over any rustflags in
        // a parent directory's .cargo/config.toml (this repo's sets crt-static).
        .env("CARGO_ENCODED_RUSTFLAGS", rustflags.unwrap_or("").replace(' ', "\x1f"));
    for (key, value) in env {
        cmd.env(key, value);
    }
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
    let out = run_cargo("doc", TARGET_32, None, true, None, &[]);
    assert!(out.status.success(), "docs.rs-style doc build failed:\n{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn docs_build_still_needs_mq_root_outside_docs_rs() {
    assert_fails_with(&run_cargo("doc", TARGET_32, None, false, Some(CRT_STATIC), &[]), "MQ_ROOT is not set");
}

// Plugins must not depend on the Visual C++ redistributable.

#[test]
fn dynamic_c_runtime_fails_with_clear_message() {
    let out = run_cargo("build", TARGET_32, Some(env!("MQ_ROOT")), false, None, &[]);
    assert_fails_with(&out, "plugins must link the C runtime statically");
}

#[test]
fn dynamic_c_runtime_can_be_opted_into() {
    let out = run_cargo("build", TARGET_32, Some(env!("MQ_ROOT")), false, None, &[("MQ_EMU_DYNAMIC_CRT", "1")]);
    assert!(out.status.success(), "opt-out build failed:
{}", String::from_utf8_lossy(&out.stderr));
}
