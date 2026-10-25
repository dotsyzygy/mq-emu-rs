use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=MQ_ROOT");
    println!("cargo::rerun-if-env-changed=MQ_BUILD_CONFIG");
    println!("cargo::rerun-if-env-changed=DOCS_RS");

    // docs.rs only runs rustdoc, which never links, and has no MacroQuest build.
    if env::var_os("DOCS_RS").is_some() {
        return;
    }

    // Checked first: the shim below would otherwise fail on MacroQuest's own
    // "Win32 Configuration is required" before the crate's compile_error! runs.
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("x86") {
        fail("mq-emu-sys targets MacroQuest emu (RoF2), which is 32-bit only. Build with --target i686-pc-windows-msvc.");
    }

    let mq_root = PathBuf::from(env::var("MQ_ROOT").unwrap_or_else(|_| {
        fail("MQ_ROOT is not set. Point it at your MacroQuest (emu branch) checkout; see .cargo/config.toml.")
    }));
    let build_config = env::var("MQ_BUILD_CONFIG").unwrap_or_else(|_| "release".to_owned());
    let bin_dir = mq_root.join("build").join("bin").join(&build_config);

    for lib in ["MQ2Main.lib", "eqlib.lib"] {
        let path = bin_dir.join(lib);
        if !path.is_file() {
            fail(&format!("{} not found. Build MacroQuest ({build_config}|Win32) first.", path.display()));
        }
        println!("cargo::rerun-if-changed={}", path.display());
    }

    // The extern blocks carry #[link(name = ...)]; we only provide the search path.
    println!("cargo::rustc-link-search=native={}", bin_dir.display());

    // MacroQuest links the C runtime statically. A plugin that doesn't would
    // need the Visual C++ redistributable on every player's machine and fail to
    // load without it, so this is an error rather than a warning (which Cargo
    // hides for crates.io dependencies anyway).
    println!("cargo::rerun-if-env-changed=MQ_EMU_DYNAMIC_CRT");
    let crt_static = env::var("CARGO_CFG_TARGET_FEATURE").is_ok_and(|features| features.split(',').any(|f| f == "crt-static"));
    if !crt_static && env::var_os("MQ_EMU_DYNAMIC_CRT").is_none() {
        fail(
            "plugins must link the C runtime statically, as MacroQuest does, or they need VCRUNTIME140.dll on every \
             player's machine. Add `rustflags = [\"-C\", \"target-feature=+crt-static\"]` under \
             [target.i686-pc-windows-msvc] in .cargo/config.toml (or set MQ_EMU_DYNAMIC_CRT=1 to accept the dependency).",
        );
    }

    build_shim(&mq_root);

    // Exposed to this crate's tests, and to dependents as DEP_MQ2MAIN_ROOT / DEP_MQ2MAIN_BIN_DIR.
    println!("cargo::rustc-env=MQ_ROOT={}", mq_root.display());
    println!("cargo::rustc-env=MQ_BIN_DIR={}", bin_dir.display());
    println!("cargo::metadata=root={}", mq_root.display());
    println!("cargo::metadata=bin_dir={}", bin_dir.display());
}

/// Compiles the C++ shim (shim/*.cpp) against MacroQuest's headers, with the
/// language settings MacroQuest uses for its own plugins (src/Common.props).
fn build_shim(mq_root: &Path) {
    let sources = ["shim/commands.cpp", "shim/actors.cpp", "shim/main_interface.cpp", "shim/test_fake.cpp"];
    println!("cargo::rerun-if-changed=shim/mq_api.h");
    for source in sources {
        println!("cargo::rerun-if-changed={source}");
    }

    let vcpkg_include = mq_root.join("contrib/vcpkg/installed/x86-windows-static/include");
    if !vcpkg_include.is_dir() {
        fail(&format!("{} not found. Build MacroQuest first so vcpkg installs its dependencies.", vcpkg_include.display()));
    }

    cc::Build::new()
        .cpp(true)
        .files(sources)
        .include(mq_root.join("include"))
        .include(mq_root.join("src/eqlib/include"))
        .include(mq_root.join("src"))
        .include(mq_root.join("contrib"))
        .include(vcpkg_include)
        .flag("/std:c++20")
        .flag("/utf-8")
        .flag("/EHsc")
        .flag("/permissive-")
        .define("WIN32_LEAN_AND_MEAN", None)
        .define("NOMINMAX", None)
        .define("_WINNT_WIN32", "0x0601")
        .define("WINVER", "0x0601")
        // MacroQuest's headers aren't warning-clean at /W4; only our own code matters here.
        .warnings(false)
        .compile("mq_emu_shim");
}

fn fail(message: &str) -> ! {
    panic!("mq-emu-sys: {message}");
}
