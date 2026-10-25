//! Unofficial, safe Rust API for writing **MacroQuest emu** plugins.
//!
//! Part of mq-rs, an independent project not affiliated with, endorsed by, or
//! supported by the MacroQuest team.
//!
//! # Emulator only
//!
//! Targets the MacroQuest `emu` branch for the RoF2 client, which is 32-bit
//! (`i686-pc-windows-msvc`). Not intended for live EverQuest.
//!
//! # A plugin
//!
//! ```ignore
//! use mq_emu::{Mq, Plugin};
//!
//! struct Hello {
//!     pulses: u64,
//! }
//!
//! impl Plugin for Hello {
//!     fn new(mq: &Mq<Self>) -> Self {
//!         mq.add_command("/hello", |plugin: &mut Hello, mq, args| {
//!             mq.chat(format!("hello {args}! {} pulses so far", plugin.pulses));
//!         })
//!         .expect("register /hello");
//!         Hello { pulses: 0 }
//!     }
//!
//!     fn on_pulse(&mut self, _mq: &Mq<Self>) {
//!         self.pulses += 1;
//!     }
//! }
//!
//! mq_emu::plugin!(Hello);
//! ```
//!
//! Build it as a `cdylib` whose library name is the DLL name MacroQuest should
//! load, then `/plugin Hello`.
//!
//! # Safety model
//!
//! - All game access happens on the game thread: callbacks get an [`Mq`] that
//!   can't leave the callback or the thread.
//! - Game objects such as [`Spawn`] borrow from that `Mq`, so they can't be
//!   kept past the callback, when the game may free them. Keep a [`SpawnId`]
//!   instead.
//! - A panic in plugin code is caught at the boundary: reported once in chat,
//!   counted in [`Mq::stats`], and the game keeps running.
//! - Callbacks that would re-enter the plugin while it's busy are queued and
//!   delivered right after, with a cap against feedback loops (see [`Plugin`]).
//!
//! Raw bindings are available as [`sys`] for anything not wrapped yet.

#[cfg(not(all(target_os = "windows", target_env = "msvc", target_arch = "x86")))]
compile_error!(
    "mq-emu targets MacroQuest emu (RoF2), which is 32-bit only. Build with --target i686-pc-windows-msvc."
);

pub mod color;
mod game;
mod host;
mod movement;
mod mq;
mod plugin;
mod runtime;
mod spawns;
mod stats;
mod text;

pub use mq_emu_sys as sys;

pub use game::{
    GameState, Heading, Position, Spawn, SpawnId, SpawnType, angle_difference,
};
pub use movement::{MoveError, MoveKey};
pub use mq::{CommandError, Mq};
pub use plugin::Plugin;
pub use runtime::{MAX_NESTED_DELIVERIES, MAX_QUEUE};
pub use spawns::{MAX_SPAWNS, Spawns};
pub use stats::{DEFAULT_SLOW_PULSE, Stats, WARNING_INTERVAL};

/// Used by [`plugin!`]; not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use crate::runtime::{NamedEvent, Runtime};
}
