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
