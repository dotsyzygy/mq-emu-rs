//! Imports from `eqlib.dll` (`EQLIB_API` / `EQLIB_VAR`).

use core::ffi::{c_char, c_uint, c_void};

use crate::constants::N_EQ_MAPPABLE_COMMANDS;
use crate::layouts::{
    ClientSpellManager, EQGroundItemListManager, EQWorldData, EverQuestinfo, KeyCombo, KeypressHandler, PcClient,
    PlayerManagerClient, zoneHeader,
};
use crate::types::{ComputedPointer, ForeignPointer, PlayerClient};

imports! {
    dll: "eqlib",
    names: EQLIB_IMPORTS,
    functions {
        // eqlib/game/Globals.h
        /// Runs a key-bindable game command (`CMD_*`) as if its key went down
        /// (`key_down = true`) or up. Movement keys stay held until released.
        /// `data` and `combo` may be null.
        pub fn EQExecuteCmd(command: c_uint, key_down: bool, data: *mut c_void, combo: *const KeyCombo) -> bool;
    }
    statics {
        // eqlib/game/Globals.h
        /// The local player's spawn.
        pub static mut pLocalPlayer: ForeignPointer<PlayerClient>;
        /// The spawn whose movement you control (usually the local player; a mount when riding).
        pub static mut pControlledPlayer: ForeignPointer<PlayerClient>;
        /// The current target, if any.
        pub static mut pTarget: ForeignPointer<PlayerClient>;
        /// Every spawn in the zone, as a linked list (`PlayerList`, then each
        /// spawn's `m_pNext`).
        pub static mut pSpawnManager: ForeignPointer<PlayerManagerClient>;
        /// Key bindings, and which bound keys are physically held
        /// (`CommandState`; commands pressed by `EQExecuteCmd` don't show there,
        /// see `EverQuestinfo::keyDown`).
        pub static mut pKeypressHandler: ForeignPointer<KeypressHandler>;
        /// Client-wide game state, including which game commands are down
        /// (`keyDown`, indexed by `CMD_*`, set however the command was pressed).
        pub static mut pEverQuestInfo: *mut EverQuestinfo;
        /// Points to one byte per bindable command (`CMD_*`): nonzero if the
        /// game currently allows it. MacroQuest's `/keypress` refuses disabled ones.
        pub static mut EQbCommandStates: *mut u8;
        /// Your character's data: profile (inventory, buffs, spells), bank, group.
        pub static mut pLocalPC: ForeignPointer<PcClient>;
        /// The spell table; `Spells` is indexed by spell ID, null for unused IDs.
        pub static mut pSpellMgr: ForeignPointer<ClientSpellManager>;
        /// Every zone the client knows; `ZoneArray` is indexed by zone ID.
        pub static mut pWorldData: ForeignPointer<EQWorldData>;
        /// The zone you're in: names, type, safe point, clip planes.
        pub static mut pZoneInfo: *mut zoneHeader;
        /// The items on the ground, as a linked list (`Top`, then each item's `pNext`).
        pub static mut pItemList: ComputedPointer<EQGroundItemListManager>;
        /// The game's name for each bindable command, indexed by `CMD_*`
        /// (e.g. "forward"); entries may be null.
        pub static mut szEQMappableCommands: [*const c_char; N_EQ_MAPPABLE_COMMANDS];
    }
}
