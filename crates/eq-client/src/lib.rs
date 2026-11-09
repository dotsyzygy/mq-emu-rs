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
//! The client follows the target: 32-bit is RoF2, the only one so far. Its
//! cargo feature, `rof2`, is optional: it insists on that client.

// The client follows the target: 32-bit is RoF2, the only client so far. (Its
// `rof2` feature is optional: it insists on RoF2.)
#[cfg(not(all(target_os = "windows", target_env = "msvc", target_arch = "x86")))]
compile_error!("eq-client: only the 32-bit RoF2 client is supported so far. Build with --target i686-pc-windows-msvc.");

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
