//! Hooks into the EverQuest RoF2 emu client (eqgame.exe, 32-bit), with no
//! MacroQuest in them: what the client is hooked at ([`rof2`]), how
//! ([`patch`]), and the shared eqclient.dll that owns the hooks ([`abi`],
//! [`loader`]). A plugin can't own a hook: MacroQuest refuses a second detour
//! on one address, and each plugin unloads on its own, taking its code with
//! it. So one DLL holds them, stays loaded once loaded, and every plugin reads
//! what they see through its versioned C ABI.
//!
//! Today it hooks `StringTable::getString`, so plugins learn which of the
//! client's messages (eqstr_us.txt's, by ID: [`eqstr`]) were shown, without
//! reading their English text.
//!
//! The client is chosen with a cargo feature, and exactly one is required:
//! `rof2` (the only one so far).

// Exactly one game client feature, matched by its target. Cargo merges features
// across a build, so when a second client is added, two enabled at once must
// fail here too.
#[cfg(not(feature = "rof2"))]
compile_error!("eq-client needs a game client feature: enable exactly one of `rof2`.");
#[cfg(all(feature = "rof2", not(all(target_os = "windows", target_env = "msvc", target_arch = "x86"))))]
compile_error!("eq-client: the rof2 client is 32-bit only. Build with --target i686-pc-windows-msvc.");

pub mod abi;
pub mod eqstr;
#[doc(hidden)]
pub mod host;
pub mod loader;
pub mod patch;
pub mod ring;
pub mod rof2;
mod win;

#[cfg(test)]
mod fake_code;
