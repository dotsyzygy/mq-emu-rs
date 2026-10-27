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
//! - Game objects such as [`Spawn`], [`Item`] and [`Spell`] borrow from that
//!   `Mq`, so they can't be kept past the callback, when the game may free
//!   them. Keep an ID ([`SpawnId`], [`SpellId`], an item's ID) instead.
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

mod actors;
mod alt_abilities;
pub mod color;
mod data;
mod events;
mod game;
mod game_fns;
mod ground;
mod group;
mod host;
pub mod imgui;
mod ini;
mod inventory;
mod items;
mod key_binds;
mod keys;
mod mercenary;
mod merchant;
pub mod lua;
mod movement;
mod mq;
mod plugin;
mod raid;
mod runtime;
mod skills;
mod spawn_buffs;
mod spawns;
mod spells;
mod tasks;
mod stats;
mod text;
mod windows;
mod xtarget;
mod zone;

pub use mq_emu_sys as sys;

pub use actors::{ActorError, Address, DEFAULT_REQUEST_TIMEOUT, Message};
pub use alt_abilities::AltAbility;
pub use data::{Data, PathError, TloError, Value};
pub use events::{ChatSource, EventError, EventMatch};
pub use ground::{GroundItem, GroundItems, MAX_GROUND_ITEMS};
pub use group::{Group, GroupMember, GroupRole};
pub use imgui::Ui;
pub use ini::{Ini, IniError};
pub use inventory::{InvSlot, Inventory};
pub use items::{Item, ItemEffect, ItemLocation, ItemSlots, ItemSpell, MAX_CONTAINER_SLOTS};
pub use key_binds::{BindSlot, KeyBindError, KeyBinding};
pub use keys::{Key, KeyCombo, ParseKeyComboError};

pub use lua::{LuaTable, LuaValue};
pub use mercenary::{Mercenary, MercenaryStance, MercenaryState, OwnedMercenary};
pub use merchant::{LootWindow, Merchant};
pub use game::{
    GameState, Heading, Position, Spawn, SpawnId, SpawnType, angle_difference,
};
pub use movement::{MoveError, MoveKey};
pub use mq::{AliasError, CommandError, Mq};
pub use plugin::Plugin;
pub use raid::{Raid, RaidLootType, RaidMember};
pub use runtime::{MAX_NESTED_DELIVERIES, MAX_QUEUE};
pub use skills::{Skill, SkillId};
pub use spawn_buffs::{CachedBuff, WindowBuff};
pub use spawns::{MAX_SPAWNS, Spawns};
pub use spells::{Buff, Spell, SpellEffect, SpellId};
pub use tasks::{NextPulse, Task, TaskId};
pub use stats::{DEFAULT_SLOW_PULSE, Stats, WARNING_INTERVAL};
pub use windows::{
    Click, ContextMenu, ListWindow, MAX_DESCENDANTS, Notification, Scroll, TabWindow, Window, WindowKind, WindowRect,
};
pub use xtarget::{XTARGET_AUTO_HATER, XTarget, XTargetStatus};
pub use zone::{CurrentZone, ZoneId, ZoneInfo, ZoneType};

/// Used by [`plugin!`]; not part of the public API.
#[doc(hidden)]
pub mod __private {
    pub use crate::runtime::{NamedEvent, Runtime};
}
