use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-env-changed=MQ_ROOT");
    println!("cargo::rerun-if-env-changed=MQ_BUILD_CONFIG");
    println!("cargo::rerun-if-env-changed=DOCS_RS");

    // docs.rs only runs rustdoc, which never links, and has no MacroQuest build.
    if env::var_os("DOCS_RS").is_some() {
        return;
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

    // Exposed to this crate's tests, and to dependents as DEP_MQ2MAIN_ROOT / DEP_MQ2MAIN_BIN_DIR.
    println!("cargo::rustc-env=MQ_ROOT={}", mq_root.display());
    println!("cargo::rustc-env=MQ_BIN_DIR={}", bin_dir.display());
    println!("cargo::metadata=root={}", mq_root.display());
    println!("cargo::metadata=bin_dir={}", bin_dir.display());
}

fn fail(message: &str) -> ! {
    panic!("mq-emu-sys: {message}");
}
