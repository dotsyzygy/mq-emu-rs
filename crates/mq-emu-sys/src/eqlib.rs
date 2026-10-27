//! Imports from `eqlib.dll` (`EQLIB_API` / `EQLIB_VAR`).

use core::ffi::{c_char, c_uint, c_void};

use crate::constants::N_EQ_MAPPABLE_COMMANDS;
use crate::layouts::{
    AggroMeterManagerClient, CContextMenuManager, CInvSlotMgr, CLootWnd, CMercenaryManager, CMerchantWnd, CPetInfoWnd, CRaid,
    CTargetWnd, CTradeWnd, CXWndManager, ClientSpellManager, EQGroundItemListManager, EQWorldData, EverQuestinfo, KeyCombo, KeypressHandler, PcClient, PlayerManagerClient, SkillManager,
    zoneHeader,
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
        /// The target window: your target's buffs as the game shows them.
        pub static mut pTargetWnd: ForeignPointer<CTargetWnd>;
        /// The pet window: your pet's buffs and commands.
        pub static mut pPetInfoWnd: ForeignPointer<CPetInfoWnd>;
        /// Your raid; `raidMembers[i]` is used where `locations[i]` is set.
        pub static mut pRaid: *mut CRaid;
        /// Your mercenary and the mercenaries you own.
        pub static mut pMercManager: ForeignPointer<CMercenaryManager>;
        /// Aggro percentages, indexed by `AggroDataTypes` (`AD_*`).
        pub static mut pAggroInfo: ComputedPointer<AggroMeterManagerClient>;
        /// Skill definitions (`pSkill`, indexed by skill number) and reuse timers.
        pub static mut pSkillMgr: ForeignPointer<SkillManager>;
        /// The merchant window, open or not; its first page lists what's for sale.
        pub static mut pMerchantWnd: ForeignPointer<CMerchantWnd>;
        /// The loot window, open or not.
        pub static mut pLootWnd: ForeignPointer<CLootWnd>;
        /// The merchant whose window is open, if any.
        pub static mut pActiveMerchant: ForeignPointer<PlayerClient>;
        /// The corpse being looted, if any.
        pub static mut pActiveCorpse: ForeignPointer<PlayerClient>;
        /// The window manager: every top-level window (`pWindows`) and which
        /// has the keyboard focus (`FocusWindow`).
        pub static mut pWndMgr: ForeignPointer<CXWndManager>;
        /// The context menus: which are open (`pCurrMenus`, `NumVisibleMenus`).
        pub static mut pContextMenuManager: ForeignPointer<CContextMenuManager>;
        /// Every item slot window (`SlotArray`), as `/itemnotify` finds them.
        pub static mut pInvSlotMgr: ForeignPointer<CInvSlotMgr>;
        /// The trade window: what's offered, and whether each side is ready.
        pub static mut pTradeWnd: ForeignPointer<CTradeWnd>;
    }
}
