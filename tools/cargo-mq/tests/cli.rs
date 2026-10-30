//! Runs `cargo mq` against throwaway plugin crates and MacroQuest folders.
//!
//! Isolation: each test gets an empty `CARGO_HOME` and no MQ_* settings, so only
//! what it passes in counts. The crates live under this repo's `target/tmp`, so
//! the only parent config is the repo's, which has no `[env]`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CARGO: &str = env!("CARGO");
const BUILT: &str = "target/i686-pc-windows-msvc";

/// A plugin crate, a MacroQuest install and a `CARGO_HOME`, all empty to start.
struct Fixture {
    root: PathBuf,
    plugin: PathBuf,
    install: PathBuf,
    home: PathBuf,
}

fn write(path: impl AsRef<Path>, contents: &str) {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn plugin_manifest(package: &str, lib: &str) -> String {
    format!("[package]\nname = \"{package}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\nname = \"{lib}\"\npath = \"lib.rs\"\ncrate-type = [\"cdylib\"]\n\n[workspace]\n")
}

fn fixture(test: &str) -> Fixture {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cargo-mq").join(test);
    let _ = fs::remove_dir_all(&root);
    let fixture = Fixture { plugin: root.join("plugin"), install: root.join("install"), home: root.join("home"), root };
    fs::create_dir_all(&fixture.home).unwrap();
    write(fixture.plugin.join("Cargo.toml"), &plugin_manifest("mq2-fake", "MQ2Fake"));
    write(fixture.plugin.join("lib.rs"), "#![allow(non_snake_case)]\n");
    write(fixture.install.join("MQ2Main.dll"), "the install's build");
    fixture
}

impl Fixture {
    /// `cargo-mq` in the plugin's folder with nothing configured.
    fn command(&self) -> Command {
        let mut command = bare_command(&self.plugin);
        command.env("CARGO_HOME", &self.home);
        command
    }

    /// As `command`, with MQ_INSTALL pointing at the fixture's install.
    fn installed(&self) -> Command {
        let mut command = self.command();
        command.env("MQ_INSTALL", &self.install);
        command
    }

    /// Stands in for a build of the plugin, without running cargo.
    fn fake_build(&self, profile: &str) {
        write(self.plugin.join(BUILT).join(profile).join("MQ2Fake.dll"), "dll");
        write(self.plugin.join(BUILT).join(profile).join("MQ2Fake.pdb"), "pdb");
    }

    /// A MacroQuest checkout holding what plugins build against.
    fn fake_mq_root(&self, config: &str, main_dll: &str) -> PathBuf {
        let root = self.root.join("checkout");
        let bin = root.join("build/bin").join(config);
        write(bin.join("MQ2Main.dll"), main_dll);
        for lib in ["MQ2Main.lib", "eqlib.lib", "imgui.lib"] {
            write(bin.join(lib), "");
        }
        root
    }

    fn deployed(&self, file: &str) -> PathBuf {
        self.install.join(file)
    }
}

fn bare_command(dir: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-mq"));
    command
        .current_dir(dir)
        .env("CARGO", CARGO)
        // The toolchain's binaries directly; the rustup proxies aren't in an empty CARGO_HOME.
        .env("RUSTC", Path::new(CARGO).with_file_name("rustc.exe"))
        .env_remove("MQ_INSTALL")
        .env_remove("MQ_ROOT")
        .env_remove("MQ_BUILD_CONFIG")
        .env_remove("CARGO_TARGET_DIR");
    command
}

fn run(command: &mut Command, args: &[&str]) -> Output {
    command.args(args).output().expect("failed to run cargo-mq")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn assert_ok(out: &Output) {
    assert!(out.status.success(), "cargo-mq failed:\n{}\n{}", stdout(out), stderr(out));
}

fn assert_fails_with(out: &Output, message: &str) {
    assert!(!out.status.success(), "cargo-mq unexpectedly succeeded:\n{}", stdout(out));
    assert!(stderr(out).contains(message), "expected {message:?} in:\n{}", stderr(out));
}

const NO_BUILD: &[&str] = &["deploy", "--no-build"];

// What deploy copies.

#[test]
fn deploy_copies_the_dll_pdb_lua_and_resources() {
    let f = fixture("copies");
    f.fake_build("release");
    write(f.plugin.join("lua/peer.lua"), "-- lua");
    write(f.plugin.join("lua/notes.txt"), "not a script");
    write(f.plugin.join("resources/uifiles/default/Wnd.xml"), "<xml/>");

    let out = run(&mut f.installed(), NO_BUILD);

    assert_ok(&out);
    assert_eq!(fs::read_to_string(f.deployed("plugins/MQ2Fake.dll")).unwrap(), "dll");
    assert!(f.deployed("plugins/MQ2Fake.pdb").is_file());
    assert!(f.deployed("lua/peer.lua").is_file());
    assert!(!f.deployed("lua/notes.txt").exists());
    assert!(f.deployed("resources/uifiles/default/Wnd.xml").is_file());
    assert!(stdout(&out).contains("/plugin MQ2Fake"), "{}", stdout(&out));
}

#[test]
fn deploy_works_when_cargo_runs_it_as_a_subcommand() {
    let f = fixture("subcommand");
    f.fake_build("release");
    assert_ok(&run(&mut f.installed(), &["mq", "deploy", "--no-build"]));
    assert!(f.deployed("plugins/MQ2Fake.dll").is_file());
}

#[test]
fn deploy_debug_takes_the_debug_build() {
    let f = fixture("debug");
    write(f.plugin.join(BUILT).join("debug/MQ2Fake.dll"), "debug dll");
    assert_ok(&run(&mut f.installed(), &["deploy", "--no-build", "--debug"]));
    assert_eq!(fs::read_to_string(f.deployed("plugins/MQ2Fake.dll")).unwrap(), "debug dll");
}

#[test]
fn deploy_builds_the_plugin_first() {
    let f = fixture("builds");
    assert_ok(&run(&mut f.installed(), &["deploy"]));
    assert_eq!(&fs::read(f.deployed("plugins/MQ2Fake.dll")).unwrap()[..2], b"MZ");
}

#[test]
fn failed_build_stops_the_deploy() {
    let f = fixture("build-fails");
    write(f.plugin.join("lib.rs"), "this isn't rust\n");
    assert_fails_with(&run(&mut f.installed(), &["deploy"]), "cargo build failed");
    assert!(!f.deployed("plugins").exists());
}

#[test]
fn missing_build_fails_with_clear_message() {
    let f = fixture("not-built");
    assert_fails_with(&run(&mut f.installed(), NO_BUILD), "drop --no-build");
}

#[test]
fn locked_dll_fails_with_clear_message() {
    use std::os::windows::fs::OpenOptionsExt;

    let f = fixture("locked");
    f.fake_build("release");
    write(f.deployed("plugins/MQ2Fake.dll"), "old");
    let lock = fs::OpenOptions::new().read(true).share_mode(0).open(f.deployed("plugins/MQ2Fake.dll")).unwrap();

    let out = run(&mut f.installed(), NO_BUILD);

    assert_fails_with(&out, "'/plugin MQ2Fake unload'");
    drop(lock);
    assert_eq!(fs::read_to_string(f.deployed("plugins/MQ2Fake.dll")).unwrap(), "old");
}

// Finding the install.

#[test]
fn install_comes_from_the_user_cargo_config() {
    for entry in ["MQ_INSTALL = \"{}\"", "MQ_INSTALL = { value = \"{}\", force = true }"] {
        let f = fixture("user-config");
        f.fake_build("release");
        let install = f.install.display().to_string().replace('\\', "/");
        write(f.home.join("config.toml"), &format!("[env]\n{}\n", entry.replace("{}", &install)));
        assert_ok(&run(&mut f.command(), NO_BUILD));
        assert!(f.deployed("plugins/MQ2Fake.dll").is_file(), "{entry}");
    }
}

#[test]
fn environment_beats_the_user_cargo_config() {
    let f = fixture("env-wins");
    f.fake_build("release");
    write(f.home.join("config.toml"), "[env]\nMQ_INSTALL = \"C:/nowhere\"\n");
    assert_ok(&run(&mut f.installed(), NO_BUILD));
    assert!(f.deployed("plugins/MQ2Fake.dll").is_file());
}

#[test]
fn install_falls_back_to_the_mq_root_build() {
    let f = fixture("mq-root");
    f.fake_build("release");
    let root = f.fake_mq_root("release", "build");
    assert_ok(&run(f.command().env("MQ_ROOT", &root), NO_BUILD));
    assert!(root.join("build/bin/release/plugins/MQ2Fake.dll").is_file());
}

#[test]
fn mq_build_config_picks_the_fallback_folder() {
    let f = fixture("mq-build-config");
    f.fake_build("release");
    let root = f.fake_mq_root("custom", "build");
    assert_ok(&run(f.command().env("MQ_ROOT", &root).env("MQ_BUILD_CONFIG", "custom"), NO_BUILD));
    assert!(root.join("build/bin/custom/plugins/MQ2Fake.dll").is_file());
}

#[test]
fn no_install_setting_fails_with_clear_message() {
    let f = fixture("no-setting");
    f.fake_build("release");
    assert_fails_with(&run(&mut f.command(), NO_BUILD), "Neither MQ_INSTALL nor MQ_ROOT is set");
}

#[test]
fn folder_without_mq2main_fails_with_clear_message() {
    let f = fixture("not-an-install");
    f.fake_build("release");
    fs::remove_file(f.install.join("MQ2Main.dll")).unwrap();
    assert_fails_with(&run(&mut f.installed(), NO_BUILD), "has no MQ2Main.dll");
    assert!(!f.deployed("plugins").exists());
}

// Finding the plugin.

#[test]
fn crate_that_is_not_a_plugin_fails_with_clear_message() {
    let f = fixture("not-a-plugin");
    write(f.plugin.join("Cargo.toml"), "[package]\nname = \"mq2-fake\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"lib.rs\"\n\n[workspace]\n");
    assert_fails_with(&run(&mut f.installed(), NO_BUILD), "isn't a plugin");
}

/// A workspace with two plugins, MQ2One and MQ2Two.
fn two_plugins(f: &Fixture) {
    write(f.plugin.join("Cargo.toml"), "[workspace]\nresolver = \"3\"\nmembers = [\"one\", \"two\"]\n");
    fs::remove_file(f.plugin.join("lib.rs")).unwrap();
    for (dir, package, lib) in [("one", "mq2-one", "MQ2One"), ("two", "mq2-two", "MQ2Two")] {
        let manifest = plugin_manifest(package, lib).replace("\n[workspace]\n", "");
        write(f.plugin.join(dir).join("Cargo.toml"), &manifest);
        write(f.plugin.join(dir).join("lib.rs"), "");
        write(f.plugin.join(BUILT).join("release").join(format!("{lib}.dll")), lib);
    }
}

#[test]
fn package_flag_takes_a_package_or_dll_name() {
    let f = fixture("package-flag");
    two_plugins(&f);
    assert_ok(&run(&mut f.installed(), &["deploy", "--no-build", "-p", "mq2-one"]));
    assert_ok(&run(&mut f.installed(), &["deploy", "--no-build", "-p", "MQ2Two"]));
    assert!(f.deployed("plugins/MQ2One.dll").is_file());
    assert!(f.deployed("plugins/MQ2Two.dll").is_file());
}

#[test]
fn deploy_takes_the_plugin_whose_folder_it_runs_in() {
    let f = fixture("in-member");
    two_plugins(&f);
    let mut command = f.installed();
    command.current_dir(f.plugin.join("two"));
    assert_ok(&run(&mut command, NO_BUILD));
    assert!(f.deployed("plugins/MQ2Two.dll").is_file());
    assert!(!f.deployed("plugins/MQ2One.dll").exists());
}

#[test]
fn several_plugins_need_a_choice() {
    let f = fixture("several");
    two_plugins(&f);
    assert_fails_with(&run(&mut f.installed(), NO_BUILD), "pick one with -p: mq2-one, mq2-two");
}

#[test]
fn unknown_package_fails_with_clear_message() {
    let f = fixture("unknown-package");
    assert_fails_with(&run(&mut f.installed(), &["deploy", "--no-build", "-p", "nope"]), "No package nope");
}

#[test]
fn unknown_argument_prints_usage() {
    let f = fixture("unknown-argument");
    assert_fails_with(&run(&mut f.installed(), &["deploy", "--nope"]), "usage:");
    assert_fails_with(&run(&mut f.installed(), &["nope"]), "usage:");
}

#[test]
fn dll_name_macroquest_cannot_find_gets_a_warning() {
    let f = fixture("bad-name");
    write(f.plugin.join("Cargo.toml"), &plugin_manifest("fake", "Fake"));
    write(f.plugin.join(BUILT).join("release/Fake.dll"), "dll");
    let out = run(&mut f.installed(), NO_BUILD);
    assert_ok(&out);
    assert!(stderr(&out).contains("names start with MQ"), "{}", stderr(&out));

    let f = fixture("good-name");
    f.fake_build("release");
    let out = run(&mut f.installed(), NO_BUILD);
    assert!(!stderr(&out).contains("names start with MQ"), "{}", stderr(&out));
}

// Mismatched MacroQuest builds.

#[test]
fn deploy_warns_when_install_differs_from_the_build() {
    let f = fixture("different-build");
    f.fake_build("release");
    let root = f.fake_mq_root("release", "another build");
    let out = run(f.installed().env("MQ_ROOT", &root), NO_BUILD);
    assert_ok(&out);
    assert!(stderr(&out).contains("different MQ2Main.dll builds"), "{}", stderr(&out));
}

#[test]
fn deploy_stays_quiet_when_install_matches_the_build() {
    let f = fixture("same-build");
    f.fake_build("release");
    let root = f.fake_mq_root("release", "the install's build");
    let out = run(f.installed().env("MQ_ROOT", &root), NO_BUILD);
    assert_ok(&out);
    assert!(!stderr(&out).contains("different MQ2Main.dll builds"), "{}", stderr(&out));
}

// Doctor.

#[test]
fn doctor_passes_outside_an_mq_emu_project_when_settings_are_right() {
    let f = fixture("doctor-ok");
    let root = f.fake_mq_root("release", "the install's build");
    let out = run(f.installed().env("MQ_ROOT", &root), &["doctor"]);
    assert_ok(&out);
    assert!(stdout(&out).contains("weren't checked"), "{}", stdout(&out));
}

#[test]
fn doctor_reports_missing_settings() {
    let f = fixture("doctor-missing");
    let out = run(&mut f.command(), &["doctor"]);
    assert_fails_with(&out, "2 problem(s)");
    assert!(stdout(&out).contains("MQ_ROOT is not set"), "{}", stdout(&out));
    assert!(stdout(&out).contains("Neither MQ_INSTALL nor MQ_ROOT"), "{}", stdout(&out));
}

#[test]
fn doctor_reports_missing_libraries() {
    let f = fixture("doctor-no-libs");
    let root = f.fake_mq_root("release", "the install's build");
    fs::remove_file(root.join("build/bin/release/eqlib.lib")).unwrap();
    let out = run(f.installed().env("MQ_ROOT", &root), &["doctor"]);
    assert_fails_with(&out, "1 problem(s)");
    assert!(stdout(&out).contains("eqlib.lib not found"), "{}", stdout(&out));
}

#[test]
fn doctor_reports_an_install_from_another_build() {
    let f = fixture("doctor-different");
    let root = f.fake_mq_root("release", "another build");
    let out = run(f.installed().env("MQ_ROOT", &root), &["doctor"]);
    assert_fails_with(&out, "1 problem(s)");
    assert!(stdout(&out).contains("different MQ2Main.dll build"), "{}", stdout(&out));
}

/// The MacroQuest build this repo's tests run against.
fn real_mq_root() -> String {
    std::env::var("MQ_ROOT").expect("MQ_ROOT must be set to run the layout tests")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).unwrap().to_path_buf()
}

#[test]
fn doctor_passes_layouts_that_match_the_pdb() {
    // The real workspace needs the real package cache, so no empty CARGO_HOME.
    let out = run(bare_command(&workspace_root()).env("MQ_ROOT", real_mq_root()), &["doctor"]);
    assert_ok(&out);
    assert!(stdout(&out).contains("layouts match"), "{}", stdout(&out));
}

/// A plugin depending on an mq-emu-sys whose layouts are `generated`.
fn plugin_with_layouts(f: &Fixture, generated: &str) {
    let sys = f.root.join("mq-emu-sys");
    write(sys.join("Cargo.toml"), "[package]\nname = \"mq-emu-sys\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[lib]\npath = \"lib.rs\"\n\n[workspace]\n");
    write(sys.join("lib.rs"), "");
    write(sys.join("src/layouts/generated.rs"), generated);
    let roots = fs::read_to_string(workspace_root().join("crates/mq-emu-sys/layouts.txt")).unwrap();
    write(sys.join("layouts.txt"), &roots);
    let dependency = "[dependencies]\nmq-emu-sys = { path = \"../mq-emu-sys\" }\n\n[workspace]\n";
    write(f.plugin.join("Cargo.toml"), &plugin_manifest("mq2-fake", "MQ2Fake").replace("[workspace]\n", dependency));
}

#[test]
fn doctor_reports_layouts_from_another_build() {
    let f = fixture("doctor-stale-layouts");
    plugin_with_layouts(&f, "// generated from some other eqlib.pdb\n");
    let root = real_mq_root();
    let install = PathBuf::from(&root).join("build/bin/release");
    let out = run(f.command().env("MQ_ROOT", &root).env("MQ_INSTALL", install), &["doctor"]);
    assert_fails_with(&out, "1 problem(s)");
    assert!(stdout(&out).contains("layouts don't match"), "{}", stdout(&out));
}

#[test]
fn doctor_reports_a_missing_pdb() {
    let f = fixture("doctor-no-pdb");
    plugin_with_layouts(&f, "");
    let root = f.fake_mq_root("release", "the install's build");
    let out = run(f.installed().env("MQ_ROOT", &root), &["doctor"]);
    assert_fails_with(&out, "1 problem(s)");
    assert!(stdout(&out).contains("eqlib.pdb not found"), "{}", stdout(&out));
}
