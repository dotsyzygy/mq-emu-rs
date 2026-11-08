//! Imports from `eqlib.dll` (`EQLIB_API` / `EQLIB_VAR`).

use core::ffi::{c_char, c_int, c_uchar, c_uint, c_ulong, c_void};

use crate::constants::N_EQ_MAPPABLE_COMMANDS;
use crate::layouts::{
    Achievement, AchievementManager, ClientAuraManager, AggroMeterManagerClient, AltAdvManager, CAltAbilityData, CBuffWindow, CCastSpellWnd, CCastingWnd, CComboWnd, CContainerMgr, CHotButtonWnd, CharacterBase, ItemGlobalIndex, VeBaseReferenceCount, VePointer_ItemClient,CSpellBookWnd, CDisplay, CContextMenuManager, CEverQuest, CMDLIST, CTaskElement, CHotButton, CListWnd, CSidlManager, CSliderWnd, CTabWnd, CXMLData, CXRect, HotButtonData, DatabaseStringTable, StringTable, ExtendedTargetList, CXStr, CXWnd, CharacterZoneClient, CInvSlotMgr, CItemDisplayManager, CLootWnd, CMercenaryManager, CMerchantWnd, CPetInfoWnd, CRaid,
    CSpellDisplayManager, CTargetWnd, CTaskManager, CTradeWnd, PCTaskStatus, TaskSystemType, CTribute, CXWndManager, ClientSpellManager, EQGroundItemListManager, EQWorldData, EverQuestinfo, KeyCombo, KeypressHandler, PcClient, PcZoneClient, PlayerManagerClient, PlayerZoneClient, SharedTaskPlayerInfo, SkillManager,
    UniversalChatProxy, zoneHeader,
};
use crate::types::{ComputedPointer, ForeignPointer, PlayerClient, StringView};

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

        // eqlib/game/ItemLinks.h
        /// Writes a chat link to `achievement` as `player` has it into
        /// `buffer`; leaves `buffer` as it was when the achievement manager
        /// doesn't know the achievement.
        pub fn FormatAchievementLink(buffer: *mut c_char, size: usize, achievement: *const Achievement, player: StringView);
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
        /// The bag windows: which are open, and on what (`pContainerWnds`), and
        /// the world container (`WorldContainer`).
        pub static mut pContainerMgr: ForeignPointer<CContainerMgr>;
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
        /// The game's string table.
        pub static mut pDBStr: ForeignPointer<DatabaseStringTable>;
        /// The game's top object, which names classes and races.
        pub static mut pEverQuest: ForeignPointer<CEverQuest>;
        /// The game's token strings, which name skills.
        pub static mut pStringTable: ForeignPointer<StringTable>;
        /// Your auras, as the aura window lists them; null without them.
        pub static mut pAuraMgr: ComputedPointer<ClientAuraManager>;
        /// The game's `CharacterZoneClient::GetBaseSkill`, which takes two
        /// arguments (`ret 8`) where eqlib declares one.
        pub static mut CharacterZoneClient__GetBaseSkill: usize;
        /// The game's `PcZoneClient::GetPcSkillLimit`, which takes one argument
        /// (`ret 4`) where eqlib declares two.
        pub static mut PcZoneClient__GetPcSkillLimit: usize;
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
        /// The UI's XML manager; windows have no XML data before it exists.
        pub static mut pSidlMgr: ForeignPointer<CSidlManager>;
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
        /// Your skill `skill` with its modifiers (`${Me.Skill}`).
        pub fn CharacterZoneClient_GetAdjustedSkill(this: *mut CharacterZoneClient, skill: c_int) -> c_int = "?GetAdjustedSkill@CharacterZoneClient@eqlib@@QAEHH@Z";
        /// eqlib's `GetBaseSkill`, declared with one argument: never call it,
        /// as the game's pops two and leaves the stack 4 bytes off. Here for
        /// the selftest's stack check (`mqemu_skill_call_stack_shift`).
        pub fn CharacterZoneClient_GetBaseSkill(this: *mut CharacterZoneClient, skill: c_int) -> c_int = "?GetBaseSkill@CharacterZoneClient@eqlib@@QAEHH@Z";
        /// eqlib's `GetPcSkillLimit`, declared with two arguments: never call
        /// it, as the game's pops one. Here for the selftest's stack check.
        pub fn PcZoneClient_GetPcSkillLimit(this: *mut PcZoneClient, skill: c_int, unused: bool) -> c_int = "?GetPcSkillLimit@PcZoneClient@eqlib@@QAEHH_N@Z";
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
        /// The string table token of skill `skill`'s name.
        pub fn SkillManager_GetNameToken(this: *mut SkillManager, skill: c_int) -> c_int = "?GetNameToken@SkillManager@eqlib@@QAEHH@Z";
        /// The string with token `id`; `found` (may be null) says whether there is one.
        pub fn StringTable_getString(this: *mut StringTable, id: c_uint, found: *mut bool) -> *const c_char = "?getString@StringTable@eqlib@@QAEPBDIPA_N@Z";
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

        /// What `objective` asks, as the task window says it, into `out`
        /// (the game writes up to `MAX_STRING` bytes).
        pub fn CTaskManager_GetElementDescription(this: *mut CTaskManager, objective: *const CTaskElement, out: *mut c_char)
            = "?GetElementDescription@CTaskManager@eqlib@@QAEXPBUCTaskElement@2@PAD@Z";

        // eqlib/game/EQClasses.h, eqlib/game/EQData.h, eqlib/game/PcClient.h
        /// String `id` of `kind` (`eDatabaseStringType`); `found` says whether
        /// the table has it.
        pub fn DatabaseStringTable_GetString(this: *mut DatabaseStringTable, id: c_int, kind: c_int, found: *mut bool) -> *const c_char
            = "?GetString@DatabaseStringTable@eqlib@@QAEPBDHW4eDatabaseStringType@2@PA_N@Z";
        /// The name the game shows for race `race`.
        pub fn CEverQuest_GetClassDesc(this: *mut CEverQuest, class: c_int) -> *const c_char = "?GetClassDesc@CEverQuest@eqlib@@QAEPBDH@Z";
        /// The name the game shows for race `race`.
        pub fn CEverQuest_GetRaceDesc(this: *mut CEverQuest, race: c_int) -> *const c_char = "?GetRaceDesc@CEverQuest@eqlib@@QAEPBDH@Z";
        /// Your friend `index` (0-based, below `GetNumberOfFriends`).
        pub fn UniversalChatProxy_GetFriendName(this: *mut UniversalChatProxy, index: c_int) -> *const c_char = "?GetFriendName@UniversalChatProxy@eqlib@@QAEPBDH@Z";
        /// The extended target window's name for slot role `role` (`xTargetType`).
        pub fn ExtendedTargetList_ExtendedTargetRoleName(this: *mut ExtendedTargetList, role: c_uint) -> *const c_char
            = "?ExtendedTargetRoleName@ExtendedTargetList@eqlib@@QAEPBDI@Z";

        // eqlib/game/CXWnd.h
        /// What kind of control `this` is (`UI_*`).
        pub fn CXWnd_GetType(this: *const CXWnd) -> c_int = "?GetType@CXWnd@eqlib@@QBE?AW4UIType@2@XZ";
        /// The window's definition in the UI's XML, or null for a window it didn't create.
        pub fn CXWnd_GetXMLData(this: *const CXWnd) -> *mut CXMLData = "?GetXMLData@CXWnd@eqlib@@QBEPAVCXMLData@2@XZ";
        /// Where the window is on screen, constructed into `out` (C++ returns
        /// the CXRect by value), which it returns.
        pub fn CXWnd_GetScreenRect(this: *const CXWnd, out: *mut CXRect) -> *mut CXRect = "?GetScreenRect@CXWnd@eqlib@@QBE?AVCXRect@2@XZ";
        /// Redraws the window's background and fade after its colors and alpha.
        pub fn CXWnd_Refade(this: *mut CXWnd) = "?Refade@CXWnd@eqlib@@QAEXXZ";

        // eqlib/game/UI.h
        /// The text of row `row`, column `column`, constructed into `out`
        /// (C++ returns the CXStr by value), which it returns.
        pub fn CListWnd_GetItemText(this: *const CListWnd, out: *mut CXStr, row: c_int, column: c_int) -> *mut CXStr = "?GetItemText@CListWnd@eqlib@@QBE?AVCXStr@2@HH@Z";
        /// Shows page `page`, telling the tab box's parent when `notify_parent`.
        pub fn CTabWnd_SetPage(this: *mut CTabWnd, page: c_int, notify_parent: bool) = "?SetPage@CTabWnd@eqlib@@QAEXH_N@Z";
        /// Closes every open context menu.
        pub fn CContextMenuManager_Flush(this: *mut CContextMenuManager) = "?Flush@CContextMenuManager@eqlib@@QAEXXZ";
        /// What the hot button holds (an item, a spell, a social...), or null.
        pub fn CHotButton_GetHotButtonData(this: *const CHotButton) -> *const HotButtonData = "?GetHotButtonData@CHotButton@eqlib@@QBEPBUHotButtonData@2@XZ";
        /// Moves the slider to `value`.
        pub fn CSliderWnd_SetValue(this: *mut CSliderWnd, value: c_int) = "?SetValue@CSliderWnd@eqlib@@QAEXH@Z";
        /// Selects row `row` (without telling anyone).
        pub fn CListWnd_SetCurSel(this: *mut CListWnd, row: c_int) = "?SetCurSel@CListWnd@eqlib@@QAEXH@Z";
        /// The selected row, or -1.
        pub fn CListWnd_GetCurSel(this: *const CListWnd) -> c_int = "?GetCurSel@CListWnd@eqlib@@QBEHXZ";
        /// Scrolls the list so row `row` shows.
        pub fn CListWnd_EnsureVisible(this: *mut CListWnd, row: c_int) = "?EnsureVisible@CListWnd@eqlib@@QAEXH@Z";
        /// Picks row `row` of the combo box's list and shows it as the choice.
        pub fn CComboWnd_SetChoice(this: *mut CComboWnd, row: c_int) = "?SetChoice@CComboWnd@eqlib@@QAEXH@Z";
        /// Presses the bar's button `button` (0-based), as its key does;
        /// `allow_auto_right_click` is a BOOL.
        pub fn CHotButtonWnd_DoHotButton(this: *mut CHotButtonWnd, button: c_int, allow_auto_right_click: c_int) = "?DoHotButton@CHotButtonWnd@eqlib@@QAEXHH@Z";
        /// The item at `location`, constructed into `out` (C++ returns the
        /// `VePointer` by value, holding a reference), which it returns.
        pub fn CharacterBase_GetItemByGlobalIndex(this: *const CharacterBase, out: *mut VePointer_ItemClient, location: *const ItemGlobalIndex) -> *mut VePointer_ItemClient
            = "?GetItemByGlobalIndex@CharacterBase@eqlib@@QBE?AV?$VePointer@VItemClient@eqlib@@@2@ABVItemGlobalIndex@2@@Z";
        /// Drops a reference, deleting the object with the last (what a
        /// `VePointer`'s destructor does).
        pub fn VeBaseReferenceCount_DecrementRefCount(this: *mut VeBaseReferenceCount) = "?DecrementRefCount@VeBaseReferenceCount@eqlib@@QAEXXZ";

        // eqlib/game/CXStr.h
        /// Constructs a CXStr in `this` from NUL-terminated `text`.
        pub fn CXStr_from_c_str(this: *mut CXStr, text: *const c_char) -> *mut CXStr = "??0CXStr@eqlib@@QAE@PBD@Z";
        /// Destroys the CXStr in `this`, releasing its text.
        pub fn CXStr_drop(this: *mut CXStr) = "??1CXStr@eqlib@@QAE@XZ";
        /// The CXStr's text, NUL-terminated.
        pub fn CXStr_c_str(this: *const CXStr) -> *const c_char = "?c_str@CXStr@eqlib@@QBEPBDXZ";

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

        // eqlib/game/ItemLinks.h
        /// The game's `CleanItemTags`: `text` with each chat link replaced by
        /// its text, constructed into `out` (C++ returns the CXStr by value),
        /// which it returns. MacroQuest passes `flag` false.
        pub fn CleanItemTags(out: *mut CXStr, text: *const CXStr, flag: bool) -> *mut CXStr = "?CleanItemTags@eqlib@@YA?AVCXStr@1@ABV21@_N@Z";
    }
}
