//! Imports from `eqlib.dll` (`EQLIB_API` / `EQLIB_VAR`).

use core::ffi::{c_char, c_int, c_uchar, c_uint, c_ulong, c_void};

use crate::constants::N_EQ_MAPPABLE_COMMANDS;
use crate::layouts::{
    AchievementManager, AggroMeterManagerClient, AltAdvManager, CAltAbilityData, CBuffWindow, CCastSpellWnd, CCastingWnd, CSpellBookWnd, CDisplay, CContextMenuManager, CMDLIST, CXStr, CXWnd, CharacterZoneClient, CInvSlotMgr, CItemDisplayManager, CLootWnd, CMercenaryManager, CMerchantWnd, CPetInfoWnd, CRaid,
    CSpellDisplayManager, CTargetWnd, CTaskManager, CTradeWnd, PCTaskStatus, TaskSystemType, CTribute, CXWndManager, ClientSpellManager, EQGroundItemListManager, EQWorldData, EverQuestinfo, KeyCombo, KeypressHandler, PcClient, PcZoneClient, PlayerManagerClient, PlayerZoneClient, SharedTaskPlayerInfo, SkillManager,
    UniversalChatProxy, zoneHeader,
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
        /// The game's clock in seconds, which task timers count by.
        pub fn GetFastTime() -> c_ulong;
        /// The game's clock in milliseconds, which skill timers count by.
        pub fn EQGetTime() -> c_ulong;
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
        /// The item display windows (`m_windows`), made as items are shown;
        /// the window manager's list doesn't have them.
        pub static mut pItemDisplayManager: ForeignPointer<CItemDisplayManager>;
        /// The spell display windows, likewise.
        pub static mut pSpellDisplayManager: ForeignPointer<CSpellDisplayManager>;
        /// Your tasks: `QuestEntries` (solo) and `SharedTaskEntries`, used
        /// where `TaskID` is set.
        pub static mut pTaskManager: *mut CTaskManager;
        /// The first member of your shared task, then each `pNext`.
        pub static mut pTaskMember: ForeignPointer<SharedTaskPlayerInfo>;
        /// The game's slash commands, ended by an entry with no handler.
        pub static mut EQADDR_CMDLIST: *mut CMDLIST;
        /// Points to a byte that's nonzero while your tribute is active.
        pub static mut pTributeActive: *mut u8;
        /// The spell gem bar; `SpellSlots` past the eighth are shown only if
        /// your character has them.
        pub static mut pCastSpellWnd: ForeignPointer<CCastSpellWnd>;
        /// Tribute state; null before the game makes it.
        pub static mut pTribute: *mut CTribute;
        /// The chat service, which keeps your friends list; its getter
        /// returns null without one.
        pub static mut pChatService: ComputedPointer<UniversalChatProxy>;
        /// Every alternate ability, and whether you can use one now.
        pub static mut pAltAdvManager: ForeignPointer<AltAdvManager>;
        /// The display; `TimeStamp` is the game's clock in milliseconds, which
        /// cast and gem timers count by.
        pub static mut pDisplay: ForeignPointer<CDisplay>;
        /// The buff window: your long buffs' spell IDs and timers.
        pub static mut pBuffWnd: ForeignPointer<CBuffWindow>;
        /// The song window (short-duration buffs), likewise.
        pub static mut pSongWnd: ForeignPointer<CBuffWindow>;
        /// The casting window, shown while you cast.
        pub static mut pCastingWnd: ForeignPointer<CCastingWnd>;
        /// The spell book window, open or not; it memorizes spell sets.
        pub static mut pSpellBookWnd: ForeignPointer<CSpellBookWnd>;
    }
    methods(EQLIB_METHODS) {
        // eqlib/game/PcClient.h
        /// Your maximum HP with its modifiers, capped at the game's maximum when
        /// `cap_at_max` (C++'s default). `spawn_type` 0 is you.
        pub fn CharacterZoneClient_Max_HP(this: *mut CharacterZoneClient, spawn_type: c_int, cap_at_max: bool) -> c_int
            = "?Max_HP@CharacterZoneClient@eqlib@@QAEHH_N@Z";
        /// Your current HP, capped like [`CharacterZoneClient_Max_HP`].
        pub fn CharacterZoneClient_Cur_HP(this: *mut CharacterZoneClient, spawn_type: c_int, cap_at_max: bool) -> c_int
            = "?Cur_HP@CharacterZoneClient@eqlib@@QAEHH_N@Z";
        /// Your maximum endurance with its modifiers, capped when `cap_at_max`
        /// (C++'s default).
        pub fn CharacterZoneClient_Max_Endurance(this: *mut CharacterZoneClient, cap_at_max: bool) -> c_int
            = "?Max_Endurance@CharacterZoneClient@eqlib@@QAEH_N@Z";
        /// Your maximum mana with its modifiers, capped when `cap_at_max`
        /// (C++'s default).
        pub fn CharacterZoneClient_Max_Mana(this: *mut CharacterZoneClient, cap_at_max: bool) -> c_int
            = "?Max_Mana@CharacterZoneClient@eqlib@@QAEH_N@Z";
        /// Whether you have skill `skill` (0 to `NUM_SKILLS - 1`).
        pub fn CharacterZoneClient_HasSkill(this: *mut CharacterZoneClient, skill: c_int) -> bool = "?HasSkill@CharacterZoneClient@eqlib@@QAE_NH@Z";
        /// Uses skill `skill` on `target`, as `/doability` does; the game checks its timer.
        pub fn CharacterZoneClient_UseSkill(this: *mut CharacterZoneClient, skill: c_uchar, target: *mut PlayerZoneClient)
            = "?UseSkill@CharacterZoneClient@eqlib@@QAEXEPAVPlayerZoneClient@2@@Z";
        /// Uses combat ability `spell_id` (allowing a lower rank you have when
        /// `allow_lower_rank`, C++'s default); false when the game refuses.
        pub fn PcZoneClient_DoCombatAbility(this: *mut PcZoneClient, spell_id: c_int, allow_lower_rank: bool) -> bool
            = "?DoCombatAbility@PcZoneClient@eqlib@@QAE_NH_N@Z";

        // eqlib/game/EQClasses.h, eqlib/game/EQData.h
        /// What your active tribute costs in favor.
        pub fn CTribute_GetActiveFavorCost(this: *mut CTribute) -> c_int = "?GetActiveFavorCost@CTribute@eqlib@@QAEHXZ";
        /// How many friends your friends list has.
        pub fn UniversalChatProxy_GetNumberOfFriends(this: *mut UniversalChatProxy) -> c_int = "?GetNumberOfFriends@UniversalChatProxy@eqlib@@QAEHXZ";

        // eqlib/game/UI.h
        /// Forgets the spell in gem `gem` (0-based), as right-clicking it does.
        pub fn CCastSpellWnd_ForgetMemorizedSpell(this: *mut CCastSpellWnd, gem: c_int) = "?ForgetMemorizedSpell@CCastSpellWnd@eqlib@@QAEXH@Z";
        /// Whether a bard song is playing.
        pub fn CCastSpellWnd_IsBardSongPlaying(this: *mut CCastSpellWnd) -> bool = "?IsBardSongPlaying@CCastSpellWnd@eqlib@@QAE_NXZ";

        /// Memorizes `count` spells, one per gem (-1 leaves a gem as it is),
        /// one after another, as `/memspell` does.
        pub fn CSpellBookWnd_MemorizeSet(this: *mut CSpellBookWnd, spells: *mut c_int, count: c_int) = "?MemorizeSet@CSpellBookWnd@eqlib@@QAEXPAHH@Z";

        // eqlib/game/EQClasses.h
        /// Whether skill `skill` is ready to use now.
        pub fn SkillManager_IsAvailable(this: *mut SkillManager, skill: c_int) -> bool = "?IsAvailable@SkillManager@eqlib@@QAE_NH@Z";
        /// Skill `skill`'s full reuse time in milliseconds.
        pub fn SkillManager_GetSkillTimerDuration(this: *mut SkillManager, skill: c_int) -> c_uint = "?GetSkillTimerDuration@SkillManager@eqlib@@QAEIH@Z";
        /// When you last used skill `skill`, by [`EQGetTime`].
        pub fn SkillManager_GetSkillLastUsed(this: *mut SkillManager, skill: c_int) -> c_uint = "?GetSkillLastUsed@SkillManager@eqlib@@QAEIH@Z";

        // eqlib/game/TaskSystem.h
        /// `pc`'s progress on entry `index` of the list `system` names
        /// (`QuestEntries` for solo quests, `SharedTaskEntries`).
        pub fn CTaskManager_GetTaskStatus(this: *mut CTaskManager, pc: *mut PcClient, index: c_int, system: TaskSystemType) -> *mut PCTaskStatus
            = "?GetTaskStatus@CTaskManager@eqlib@@QAEPAUPCTaskStatus@2@PAVPcClient@2@HW4TaskSystemType@2@@Z";

        // eqlib/game/CXWnd.h
        /// What kind of control `this` is (`UI_*`).
        pub fn CXWnd_GetType(this: *const CXWnd) -> c_int = "?GetType@CXWnd@eqlib@@QBE?AW4UIType@2@XZ";

        // eqlib/game/CXStr.h
        /// Constructs a CXStr in `this` from NUL-terminated `text`.
        pub fn CXStr_from_c_str(this: *mut CXStr, text: *const c_char) -> *mut CXStr = "??0CXStr@eqlib@@QAE@PBD@Z";
        /// Destroys the CXStr in `this`, releasing its text.
        pub fn CXStr_drop(this: *mut CXStr) = "??1CXStr@eqlib@@QAE@XZ";

        // eqlib/game/AltAbilities.h
        /// The alternate ability with this index (one rank of one ability), or null.
        pub fn AltAdvManager_GetAAById(this: *mut AltAdvManager, index: c_int) -> *mut CAltAbilityData = "?GetAAById@AltAdvManager@eqlib@@QAEPAVCAltAbilityData@2@H@Z";
        /// Whether `pc` can use `ability` now. `refresh` (may be null) gets
        /// its reuse time left in seconds, `timer` (may be null) its timer.
        pub fn AltAdvManager_IsAbilityReady(this: *mut AltAdvManager, pc: *mut PcClient, ability: *mut CAltAbilityData, refresh: *mut c_int, timer: *mut c_int) -> bool
            = "?IsAbilityReady@AltAdvManager@eqlib@@QAE_NPAVPcClient@2@PAVCAltAbilityData@2@PAH2@Z";
    }
    cpp_functions(EQLIB_CPP_FUNCTIONS) {
        // eqlib/game/Achievements.h
        /// The achievement manager (C++ returns a reference): every achievement
        /// and category the client knows, and your progress.
        pub fn AchievementManager_Instance() -> *mut AchievementManager = "?Instance@AchievementManager@eqlib@@SAAAV12@XZ";
    }
}
