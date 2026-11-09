//! `cargo mq deploy` builds a plugin and copies it into a MacroQuest install.
//! `cargo mq doctor` checks that the MacroQuest a plugin builds against, the one
//! it deploys to and mq-emu's committed layouts all agree.
//!
//! Settings come from the environment, then from `[env]` in Cargo's config
//! files, the same places the build finds `MQ_ROOT`.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// The target for a 32-bit MacroQuest (RoF2): the only client so far.
const TARGET_32: &str = "i686-pc-windows-msvc";
/// The target for a 64-bit MacroQuest (ToB, to come).
const TARGET_64: &str = "x86_64-pc-windows-msvc";

const USAGE: &str = "\
Build, deploy and check MacroQuest plugins made with mq-emu.

usage:
  cargo mq deploy [-p <package>] [--debug] [--no-build]
      Build a plugin and copy it into a MacroQuest install, for the target
      its MQ2Main.dll is (32-bit: RoF2).
  cargo mq doctor
      Check that the MacroQuest a plugin builds against, the one it deploys
      to and mq-emu's committed layouts all agree.

MQ_INSTALL is the folder holding MQ2Main.dll. Without it, deploy uses
$MQ_ROOT/build/bin/$MQ_BUILD_CONFIG (default release). Set either in your
environment or under [env] in ~/.cargo/config.toml.";

type Result<T> = std::result::Result<T, String>;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // Cargo runs a subcommand as `cargo-mq mq <args>`.
    if args.first().is_some_and(|arg| arg == "mq") {
        args.remove(0);
    }
    match run(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("cargo-mq: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<ExitCode> {
    match args.split_first() {
        Some((command, rest)) if command == "deploy" => deploy(&parse_deploy_args(rest)?).map(|()| ExitCode::SUCCESS),
        Some((command, [])) if command == "doctor" => doctor(),
        Some((command, [])) if command == "-h" || command == "--help" => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(format!("unknown arguments {args:?}\n{USAGE}")),
    }
}

#[derive(Default)]
struct DeployArgs {
    package: Option<String>,
    debug: bool,
    no_build: bool,
}

fn parse_deploy_args(args: &[String]) -> Result<DeployArgs> {
    let mut parsed = DeployArgs::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" | "--package" => parsed.package = Some(args.next().ok_or(format!("{arg} needs a package name"))?.clone()),
            "--debug" => parsed.debug = true,
            "--no-build" => parsed.no_build = true,
            other => return Err(format!("unknown argument {other:?}\n{USAGE}")),
        }
    }
    Ok(parsed)
}

// Settings.

/// A setting from the environment, or else from `[env]` in Cargo's config files:
/// the current folder and its parents, then `CARGO_HOME`.
fn setting(name: &str) -> Option<String> {
    if let Some(value) = std::env::var(name).ok().filter(|value| !value.is_empty()) {
        return Some(value);
    }
    config_files().iter().find_map(|path| {
        let table: toml::Table = std::fs::read_to_string(path).ok()?.parse().ok()?;
        let value = table.get("env")?.get(name)?;
        // Both `NAME = "..."` and `NAME = { value = "...", ... }`.
        value.as_str().or_else(|| value.get("value")?.as_str()).map(str::to_owned)
    })
}

fn config_files() -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = std::env::current_dir()
        .map(|cwd| cwd.ancestors().map(|dir| dir.join(".cargo")).collect())
        .unwrap_or_default();
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join(".cargo")));
    folders.extend(cargo_home);
    folders.iter().flat_map(|dir| [dir.join("config.toml"), dir.join("config")]).collect()
}

/// `$MQ_ROOT/build/bin/$MQ_BUILD_CONFIG`: where plugins get MacroQuest's libraries.
fn build_dir() -> Option<PathBuf> {
    let config = setting("MQ_BUILD_CONFIG").unwrap_or_else(|| "release".to_owned());
    Some(PathBuf::from(setting("MQ_ROOT")?).join("build/bin").join(config))
}

fn install_dir() -> Result<PathBuf> {
    let dir = setting("MQ_INSTALL").map(PathBuf::from).or_else(build_dir).ok_or(
        "Neither MQ_INSTALL nor MQ_ROOT is set. Set MQ_INSTALL to the folder holding MQ2Main.dll, in your \
         environment or under [env] in ~/.cargo/config.toml.",
    )?;
    if !dir.join("MQ2Main.dll").is_file() {
        return Err(format!("{} has no MQ2Main.dll, so it isn't a MacroQuest install. Check MQ_INSTALL and MQ_ROOT.", dir.display()));
    }
    Ok(dir)
}

/// Whether the install runs the build plugins compile against. `None` if the
/// build's DLL can't be read, so there's nothing to compare with.
/// The target plugins for the MacroQuest in `install` must be built for: its
/// MQ2Main.dll's. The game client follows from it (mq-emu-sys reads it from
/// MQ_ROOT's build), so a plugin never names one.
fn install_target(install: &Path) -> Result<&'static str> {
    let path = install.join("MQ2Main.dll");
    let image = std::fs::read(&path).map_err(|e| format!("couldn't read {}: {e}", path.display()))?;
    match object::FileKind::parse(&*image) {
        Ok(object::FileKind::Pe32) => Ok(TARGET_32),
        Ok(object::FileKind::Pe64) => Ok(TARGET_64),
        _ => Err(format!("{} isn't a Windows DLL, so the target to build for is unknown.", path.display())),
    }
}

fn same_build(install: &Path, build: &Path) -> Option<bool> {
    let built = std::fs::read(build.join("MQ2Main.dll")).ok()?;
    Some(std::fs::read(install.join("MQ2Main.dll")).ok()? == built)
}

const DIFFERENT_BUILD: &str = "plugins would be built against a different MacroQuest than the one they run in, \
     which can misread game memory. MacroQuest's plugin loader doesn't check for this.";

// Cargo.

fn cargo() -> Command {
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
}

fn metadata(no_deps: bool) -> Result<Value> {
    let mut command = cargo();
    command.args(["metadata", "--format-version", "1"]);
    if no_deps {
        command.arg("--no-deps");
    }
    let output = command.output().map_err(|e| format!("couldn't run cargo: {e}"))?;
    if !output.status.success() {
        return Err(format!("cargo metadata failed:\n{}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("couldn't read cargo metadata: {e}"))
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

fn manifest_dir(package: &Value) -> PathBuf {
    Path::new(text(package, "manifest_path")).parent().unwrap_or(Path::new("")).to_path_buf()
}

/// The DLL's name without `.dll`: the package's `cdylib` library, if it has
/// one. eq-client-host's (eqclient.dll) isn't a plugin; deploy ships it with them.
fn plugin_name(package: &Value) -> Option<String> {
    if text(package, "name") == "eq-client-host" {
        return None;
    }
    let targets = package["targets"].as_array()?;
    let library = targets.iter().find(|target| target["kind"].as_array().is_some_and(|kinds| kinds.iter().any(|kind| kind == "cdylib")))?;
    Some(text(library, "name").replace('-', "_"))
}

struct PluginCrate {
    name: String,
    dir: PathBuf,
    target_dir: PathBuf,
}

fn find_plugin(metadata: &Value, wanted: Option<&str>) -> Result<PluginCrate> {
    let packages = metadata["packages"].as_array().map(Vec::as_slice).unwrap_or_default();
    let package = match wanted {
        Some(wanted) => packages
            .iter()
            .find(|package| text(package, "name") == wanted || plugin_name(package).as_deref() == Some(wanted))
            .ok_or(format!("No package {wanted} in this workspace."))?,
        None => current_package(packages).map_or_else(|| only_plugin(packages), Ok)?,
    };
    let name = plugin_name(package).ok_or(format!(
        "{} isn't a plugin: it has no library with crate-type = [\"cdylib\"] (see the mq-emu README).",
        text(package, "name")
    ))?;
    Ok(PluginCrate { name, dir: manifest_dir(package), target_dir: PathBuf::from(text(metadata, "target_directory")) })
}

/// The package whose folder holds the current folder, innermost first.
fn current_package(packages: &[Value]) -> Option<&Value> {
    let cwd = std::env::current_dir().ok()?.canonicalize().ok()?;
    packages
        .iter()
        .filter_map(|package| Some((package, manifest_dir(package).canonicalize().ok()?)))
        .filter(|(_, dir)| cwd.starts_with(dir))
        .max_by_key(|(_, dir)| dir.components().count())
        .map(|(package, _)| package)
}

/// From a workspace's root, which isn't in any package.
fn only_plugin(packages: &[Value]) -> Result<&Value> {
    let plugins: Vec<&Value> = packages.iter().filter(|package| plugin_name(package).is_some()).collect();
    match plugins.as_slice() {
        [] => Err("No plugin in this workspace: no package has a library with crate-type = [\"cdylib\"].".to_owned()),
        [plugin] => Ok(plugin),
        several => {
            let names: Vec<&str> = several.iter().map(|package| text(package, "name")).collect();
            Err(format!("Several plugins here; pick one with -p: {}.", names.join(", ")))
        }
    }
}

// Deploy.

fn deploy(args: &DeployArgs) -> Result<()> {
    let install = install_dir()?;
    let plugin = find_plugin(&metadata(true)?, args.package.as_deref())?;
    let target = install_target(&install)?;
    let name = &plugin.name;
    let profile = if args.debug { "debug" } else { "release" };

    if !args.no_build {
        println!("Building {name} ({profile})...");
        let mut command = cargo();
        // From the plugin's folder, so its .cargo/config.toml applies.
        command.args(["build", "--target", target]).current_dir(&plugin.dir);
        if !args.debug {
            command.arg("--release");
        }
        let status = command.status().map_err(|e| format!("couldn't run cargo: {e}"))?;
        if !status.success() {
            return Err("cargo build failed.".to_owned());
        }
    }

    let built = plugin.target_dir.join(target).join(profile);
    let dll = built.join(format!("{name}.dll"));
    if !dll.is_file() {
        return Err(format!("{} not found. Build it first (or drop --no-build).", dll.display()));
    }

    let plugins_dir = install.join("plugins");
    create_dir(&plugins_dir)?;
    copy_plugin_dll(&dll, &plugins_dir, name)?;
    println!("Copied {name}.dll to {}", plugins_dir.display());
    let pdb = built.join(format!("{name}.pdb"));
    if pdb.is_file() {
        copy_plugin_dll(&pdb, &plugins_dir, name)?;
    }

    deploy_client_hooks(&plugin, target, profile, &plugins_dir, args.no_build)?;
    copy_lua(&plugin.dir.join("lua"), &install.join("lua"))?;
    copy_resources(&plugin.dir.join("resources"), &install.join("resources"))?;

    if !name.to_ascii_lowercase().starts_with("mq") {
        eprintln!("warning: MacroQuest only finds plugin DLLs whose names start with MQ, so it won't load {name}.dll.");
    }
    if let Some(build) = build_dir()
        && same_build(&install, &build) == Some(false)
    {
        eprintln!("warning: {} and {} hold different MQ2Main.dll builds: {DIFFERENT_BUILD}", install.display(), build.display());
    }
    println!("In game: /plugin {name}");
    Ok(())
}

/// eqclient.dll, the client hooks every mq-emu plugin shares, from the
/// eq-client the plugin was built with (its sibling eq-client-host package, in
/// this repo or a git checkout of it). Nothing for a plugin without eq-client.
fn deploy_client_hooks(plugin: &PluginCrate, target: &str, profile: &str, plugins_dir: &Path, no_build: bool) -> Result<()> {
    let metadata = metadata(false)?;
    let packages = metadata["packages"].as_array().map(Vec::as_slice).unwrap_or_default();
    let Some(eq_client) = packages.iter().find(|package| text(package, "name") == "eq-client") else { return Ok(()) };
    let host = manifest_dir(eq_client).parent().unwrap_or(Path::new("")).join("eq-client-host").join("Cargo.toml");
    if !host.is_file() {
        return Err(format!("{} isn't there, so eqclient.dll can't be built (eq-client from crates.io doesn't include it yet).", host.display()));
    }
    if !no_build {
        let mut command = cargo();
        command.args(["build", "--target", target, "--manifest-path"]).arg(&host).arg("--target-dir").arg(&plugin.target_dir).current_dir(&plugin.dir);
        if profile == "release" {
            command.arg("--release");
        }
        let status = command.status().map_err(|e| format!("couldn't run cargo: {e}"))?;
        if !status.success() {
            return Err("building eqclient.dll failed.".to_owned());
        }
    }
    let dll = plugin.target_dir.join(target).join(profile).join(CLIENT_DLL);
    let target = plugins_dir.join(CLIENT_DLL);
    let built = std::fs::read(&dll).map_err(|e| format!("couldn't read {}: {e}", dll.display()))?;
    let installed = std::fs::read(&target).ok();
    if installed.as_deref() == Some(&built[..]) {
        return Ok(());
    }
    // Every plugin's deploy ships one, built from the eq-client that plugin
    // uses; the newest serves them all, so never go back to an older one.
    let (built_version, installed_version) = (client_version(&built), installed.as_deref().and_then(client_version));
    if installed.is_some() && built_version < installed_version {
        println!("Kept the installed {CLIENT_DLL} ({}): it's newer than this plugin's ({}).", describe(installed_version), describe(built_version));
        return Ok(());
    }
    match std::fs::copy(&dll, &target) {
        Ok(_) => println!("Copied {CLIENT_DLL} to {}", plugins_dir.display()),
        // It stays loaded for the life of a game client, so it can't be replaced while one runs.
        Err(e) if target.is_file() => eprintln!(
            "warning: couldn't replace {} ({e}): a running client has it loaded, and keeps it until it exits. Plugins needing a newer one say so; close the game and deploy again.",
            target.display()
        ),
        Err(e) => return Err(format!("couldn't copy {} to {}: {e}", dll.display(), target.display())),
    }
    Ok(())
}

/// The DLL eq-client-host builds.
const CLIENT_DLL: &str = "eqclient.dll";

/// The data export holding eqclient.dll's version (`eq_client::abi::VERSION_EXPORT`).
const CLIENT_VERSION_EXPORT: &[u8] = b"eqclient_version";

/// eqclient.dll's version, read from the file (`eq_client::abi::DllVersion`:
/// its API version, then its eq-client's major, minor and patch). None for an
/// eqclient.dll from before the export, which is older than any with it.
fn client_version(image: &[u8]) -> Option<[u32; 4]> {
    use object::read::pe::{ExportTarget, PeFile32};
    let file = PeFile32::parse(image).ok()?;
    let exports = file.export_table().ok()??.exports().ok()?;
    let export = exports.iter().find(|export| export.name == Some(CLIENT_VERSION_EXPORT))?;
    let ExportTarget::Address(rva) = export.target else { return None };
    let bytes = file.section_table().pe_data_at(image, rva)?.get(..16)?;
    let field = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    Some([field(0), field(1), field(2), field(3)])
}

fn describe(version: Option<[u32; 4]>) -> String {
    match version {
        Some([api, major, minor, patch]) => format!("API {api}, eq-client {major}.{minor}.{patch}"),
        None => "unversioned, from an older eq-client".to_owned(),
    }
}

fn create_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir).map_err(|e| format!("couldn't create {}: {e}", dir.display()))
}

fn copy_into(file: &Path, target: &Path) -> Result<()> {
    std::fs::copy(file, target).map(drop).map_err(|e| format!("couldn't copy {} to {}: {e}", file.display(), target.display()))
}

/// The plugin's own files can't be replaced while a game client has it loaded.
fn copy_plugin_dll(file: &Path, plugins_dir: &Path, name: &str) -> Result<()> {
    let target = plugins_dir.join(file.file_name().unwrap_or_default());
    std::fs::copy(file, &target).map(drop).map_err(|e| {
        let holders = plugin_holders(name);
        if holders.is_empty() {
            format!("Couldn't overwrite {} ({e}); it's probably loaded. Run '/plugin {name} unload' in game, then try again.", target.display())
        } else {
            // A client started while the plugin was loaded auto-loads it too.
            format!(
                "Couldn't overwrite {}: still loaded in EverQuest client(s) {}. Run '/plugin {name} unload' in each, then try again.",
                target.display(),
                holders.join(", ")
            )
        }
    })
}

/// The game clients are 32-bit, and a 64-bit process can't list their DLLs, so
/// this asks 32-bit Windows PowerShell.
fn plugin_holders(name: &str) -> Vec<String> {
    let Some(system_root) = std::env::var_os("SystemRoot") else { return Vec::new() };
    let powershell = Path::new(&system_root).join(r"SysWOW64\WindowsPowerShell\v1.0\powershell.exe");
    let script = format!(
        "Get-Process -Name eqgame -ErrorAction SilentlyContinue | Where-Object {{ $_.Modules.ModuleName -contains '{name}.dll' }} | \
         ForEach-Object {{ '{{0}} (started {{1:t}})' -f $_.Id, $_.StartTime }}"
    );
    let Ok(output) = Command::new(powershell).args(["-NoProfile", "-Command", &script]).output() else { return Vec::new() };
    String::from_utf8_lossy(&output.stdout).lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_owned).collect()
}

/// Lua scripts that go with the plugin. A running script keeps its old code
/// until restarted with /lua run.
fn copy_lua(source: &Path, destination: &Path) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(source) else { return Ok(()) };
    for path in entries.flatten().map(|entry| entry.path()) {
        if path.is_file() && path.extension().is_some_and(|extension| extension == "lua") {
            create_dir(destination)?;
            let name = path.file_name().unwrap_or_default();
            copy_into(&path, &destination.join(name))?;
            println!("Copied {} to {}", name.to_string_lossy(), destination.display());
        }
    }
    Ok(())
}

/// Resources that go with the plugin, as a C++ plugin ships them (e.g. the UI
/// XML for its windows in uifiles/default), keeping their folders. The UI reads
/// XML only when it loads.
fn copy_resources(source: &Path, destination: &Path) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(source) else { return Ok(()) };
    for path in entries.flatten().map(|entry| entry.path()) {
        let target = destination.join(path.file_name().unwrap_or_default());
        if path.is_dir() {
            copy_resources(&path, &target)?;
        } else {
            create_dir(destination)?;
            copy_into(&path, &target)?;
            println!("Copied {} to {}", path.display(), target.display());
        }
    }
    Ok(())
}

// Doctor.

fn doctor() -> Result<ExitCode> {
    let mut problems = 0;
    let mut report = |result: Result<String>| match result {
        Ok(message) => println!("ok: {message}"),
        Err(message) => {
            problems += 1;
            println!("problem: {message}");
        }
    };

    report(check_build());
    report(check_install());
    match mq_emu_sys_package() {
        Some(package) => report(check_layouts(&package)),
        None => println!("note: not in a project using mq-emu, so its layouts weren't checked"),
    }

    if problems > 0 {
        return Err(format!("{problems} problem(s) found."));
    }
    Ok(ExitCode::SUCCESS)
}

fn check_build() -> Result<String> {
    let build = build_dir().ok_or("MQ_ROOT is not set, so plugins can't build.")?;
    for lib in ["MQ2Main.lib", "eqlib.lib", "imgui.lib"] {
        if !build.join(lib).is_file() {
            return Err(format!("{} not found. Build MacroQuest (Win32) first.", build.join(lib).display()));
        }
    }
    Ok(format!("plugins build against {}", build.display()))
}

fn check_install() -> Result<String> {
    let install = install_dir()?;
    match build_dir().and_then(|build| same_build(&install, &build)) {
        Some(false) => Err(format!("{} holds a different MQ2Main.dll build than MQ_ROOT: {DIFFERENT_BUILD}", install.display())),
        _ => Ok(format!("plugins deploy to {}", install.display())),
    }
}

fn mq_emu_sys_package() -> Option<Value> {
    let metadata = metadata(false).ok()?;
    metadata["packages"].as_array()?.iter().find(|package| text(package, "name") == "mq-emu-sys").cloned()
}

/// mq-emu-sys commits layouts generated from one eqlib.pdb. Regenerating them
/// from this machine's PDB shows whether they describe this MacroQuest.
fn check_layouts(package: &Value) -> Result<String> {
    let build = build_dir().ok_or("MQ_ROOT is not set, so the layouts can't be checked.")?;
    let pdb = build.join("eqlib.pdb");
    if !pdb.is_file() {
        return Err(format!("{} not found, so the layouts can't be checked. Build MacroQuest (Win32) first.", pdb.display()));
    }
    let dir = manifest_dir(package);
    let read = |path: PathBuf| std::fs::read_to_string(&path).map_err(|e| format!("couldn't read {}: {e}", path.display()));
    let roots = mq_emu_bindgen::parse_roots(&read(dir.join("layouts.txt"))?);
    let generated = mq_emu_bindgen::generate(&pdb, &roots).map_err(|e| e.to_string())?;
    let committed = read(dir.join("src/layouts/generated.rs"))?;
    // A git checkout may have turned the committed file's line endings into CRLF.
    if generated.replace("\r\n", "\n") != committed.replace("\r\n", "\n") {
        return Err(format!(
            "mq-emu's layouts don't match {}, so plugins can misread game memory. Use the mq-emu revision made for this \
             MacroQuest, or build MacroQuest at the commit this mq-emu was made for.",
            pdb.display()
        ));
    }
    Ok(format!("mq-emu's layouts match {}", pdb.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_is_not_a_versioned_dll_has_no_version() {
        assert_eq!(client_version(b"not a DLL"), None);
        // A real DLL without the export, as eqclient.dll was before it.
        let without = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        assert_eq!(client_version(&without), None);
    }

    #[test]
    fn versions_order_by_api_then_eq_client_and_unversioned_is_oldest() {
        assert!(Some([1, 0, 1, 12]) > Some([1, 0, 1, 11]));
        assert!(Some([2, 0, 1, 0]) > Some([1, 0, 9, 0]));
        assert!(Some([1, 0, 0, 0]) > None);
    }

    /// Builds eqclient.dll from this workspace and reads its version from the file.
    #[test]
    fn reads_the_version_of_a_built_eqclient_dll() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let target_dir = std::env::current_exe().unwrap().parent().unwrap().join("eqclient-version");
        let status = cargo()
            .args(["build", "-p", "eq-client-host", "--target", TARGET_32, "--target-dir"])
            .arg(&target_dir)
            .current_dir(&workspace)
            .status()
            .unwrap();
        assert!(status.success());
        let image = std::fs::read(target_dir.join(TARGET_32).join("debug").join(CLIENT_DLL)).unwrap();
        let package: Vec<u32> = env!("CARGO_PKG_VERSION").split('.').map(|n| n.parse().unwrap()).collect();
        let version = client_version(&image).expect("no version export");
        assert!(version[0] >= 1, "API version {}", version[0]);
        assert_eq!(version[1..], package[..], "eq-client-host's version is the workspace's, as cargo-mq's is");
    }
}
