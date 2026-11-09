//! Unofficial raw FFI bindings to **MacroQuest emu** (`MQ2Main.dll` and `eqlib.dll`).
//!
//! Part of mq-rs, an independent project not affiliated with, endorsed by, or
//! supported by the MacroQuest team.
//!
//! # Emulator only
//!
//! These bindings target the MacroQuest `emu` branch, which is built for the
//! Rain of Fear 2 (RoF2) client used by EverQuest emulator servers. That build
//! is 32-bit (`i686-pc-windows-msvc`) and its client version is fixed (see
//! [`EVERQUEST_VERSION`]). They are **not** intended for live EverQuest.
//!
//! The client is chosen with a cargo feature, and exactly one is required:
//! `rof2` (the only one so far). Building without one, or for another
//! architecture than the client's, is a compile error.
//!
//! # Scope
//!
//! This crate is a thin, `unsafe`, 1:1 mapping of MacroQuest's `extern "C"`
//! exports (`MQLIB_API`/`MQLIB_VAR` and `EQLIB_VAR`). Safe, idiomatic wrappers
//! belong in the `mq-emu` crate. C++-only exports (`MQLIB_OBJECT`, game classes)
//! are not bound here.
//!
//! # Linking
//!
//! `build.rs` locates `MQ2Main.lib`/`eqlib.lib` under
//! `$MQ_ROOT/build/bin/$MQ_BUILD_CONFIG` (default `release`). A plugin only
//! gains a load-time dependency on `MQ2Main.dll` for symbols it actually uses,
//! which is always satisfied when MacroQuest loads the plugin.

#![no_std]
#![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]

// Exactly one game client feature, matched by its target. Cargo merges features
// across a build, so when a second client is added, two enabled at once must
// fail here too. build.rs reports the same, before the C++ shim compiles.
#[cfg(not(feature = "rof2"))]
compile_error!("mq-emu-sys needs a game client feature: enable exactly one of `rof2`.");
#[cfg(all(feature = "rof2", not(all(target_os = "windows", target_env = "msvc", target_arch = "x86"))))]
compile_error!("mq-emu-sys: the rof2 client is 32-bit only. Build with --target i686-pc-windows-msvc.");

#[macro_use]
mod macros;

pub mod constants;
pub mod eqlib;
pub mod events;
pub mod game;
pub mod imgui;
pub mod imgui_addons;
pub mod imgui_mq;
pub mod implot;
pub mod layouts;
pub mod mq2main;
pub mod plugin;
pub mod shim;
pub mod types;
pub mod windows;

pub use constants::*;
pub use eqlib::*;
pub use mq2main::*;
pub use types::*;

/// EverQuest client build that MacroQuest emu targets
/// (`__ExpectedVersionDate " " __ExpectedVersionTime` in eqlib `offsets/eqgame.h`).
///
/// The plugin loader refuses any plugin whose exported `EverQuestVersion`
/// doesn't match this exactly.
pub const EVERQUEST_VERSION: &str = "May 10 2013 23:30:08";

/// [`EVERQUEST_VERSION`] as a NUL-terminated byte array, suitable for a
/// plugin's exported `EverQuestVersion` symbol.
pub const EVERQUEST_VERSION_NUL: [u8; EVERQUEST_VERSION.len() + 1] = {
    let src = EVERQUEST_VERSION.as_bytes();
    let mut out = [0u8; EVERQUEST_VERSION.len() + 1];
    let mut i = 0;
    while i < src.len() {
        out[i] = src[i];
        i += 1;
    }
    out
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_nul_matches_version() {
        let (last, body) = EVERQUEST_VERSION_NUL.split_last().unwrap();
        assert_eq!(*last, 0);
        assert_eq!(body, EVERQUEST_VERSION.as_bytes());
        assert!(!body.contains(&0));
    }
}
