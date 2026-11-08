//! Game data only the game's (or MacroQuest's) own functions can answer, from
//! `shim/game.cpp`: MacroQuest's cache of other spawns' buffs, alternate
//! abilities, skill values and caps, the game's string table and extended
//! target role names, task progress and objective text, the friends list,
//! tribute's cost and achievements; chat lines cleaned for event matching; and
//! acting on spells (casting, memorizing, forgetting) and through the game's
//! own slash commands.
//!
//! These import `MQ2Main.dll` and `eqlib.dll`, so they only run inside the
//! game; call them through [`API`] (see [`function_table!`]). Text comes back
//! NUL-terminated in the caller's buffer, cut to fit; functions return
//! "nothing" (null, 0, false) when the game objects they need are missing.

use core::ffi::c_char;

use crate::constants::{MAX_MEMORIZED_SPELLS, NUM_SKILLS, NUM_SPELL_GEMS, TOTAL_SPELL_COUNT};
use crate::eqlib;
use crate::layouts::{Achievement, AchievementManager, CAltAbilityData, CBuffWindow, CCastSpellWnd, CDisplay, CTaskElement, CTaskEntry, ClientSpellManager, EQ_Spell, PCTaskStatus, PcClient, PcProfile, PlayerClient, SkillManager};
use crate::types::foreign;

/// One of MacroQuest's cached buffs (`mq::CachedBuff`).
/// One of your auras, copied out of the game's aura manager.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct mqemu_aura {
    pub name: [c_char; 64],
    pub spawn_id: u32,
    pub cost: i32,
    pub icon_id: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct mqemu_cached_buff {
    pub slot: i32,
    pub spell_id: i32,
    /// Ticks left when the server sent it; -1 for a buff that doesn't wear off.
    pub duration: i32,
    /// Hits left (counters).
    pub count: i32,
    /// What's left now, counting down from when it was sent.
    pub remaining_ms: u32,
    /// How long ago the server sent it.
    pub age_ms: u32,
    pub caster: [c_char; 64],
}

/// [`mqemu_skill_flags`]: you have the skill.
pub const SKILL_HAS: u32 = 1;
/// [`mqemu_skill_flags`]: an ability you activate (a button on the abilities page).
pub const SKILL_ACTIVATABLE: u32 = 2;
/// [`mqemu_skill_flags`]: ready to use now.
pub const SKILL_AVAILABLE: u32 = 4;

function_table! {
    /// MacroQuest's cached buffs for `spawn`, in `${Spawn.Buff[n]}` order, up
    /// to `cap` into `out`. Returns how many there are. The cache holds what
    /// the server last sent for each spawn you targeted, without expired buffs.
    pub fn mqemu_cached_buffs(spawn: *mut PlayerClient, out: *mut mqemu_cached_buff, cap: usize) -> usize;
    /// Your auras as the aura window lists them (`ClientAuraManager`, what
    /// `${Me.Aura}` reads): up to `cap` into `out`. Returns how many there are.
    pub fn mqemu_auras(out: *mut mqemu_aura, cap: usize) -> usize;
    /// The game's string `id` of `kind` (`DB_STRING_*`); false if there's none.
    pub fn mqemu_db_string(id: i32, kind: i32, out: *mut c_char, cap: usize) -> bool;
    /// Your skill `skill`: `which` 0 with modifiers (`${Me.Skill}`), 1
    /// trained (`${Me.SkillBase}`), 2 the cap at your level (`${Me.SkillCap}`).
    pub fn mqemu_skill_value(skill: i32, which: i32) -> i32;
    /// The skill's name as the game shows it; false for none.
    pub fn mqemu_skill_name(skill: i32, out: *mut c_char, cap: usize) -> bool;
    /// The extended target window's name for a slot role (`xTargetType`).
    pub fn mqemu_xtarget_role_name(role: u32, out: *mut c_char, cap: usize) -> bool;
    /// `line` as MQ2Lua matches events against it: item links replaced by
    /// their text (the game's `CleanItemTags`) unless `keep_links`, then
    /// MacroQuest's color codes and line breaks removed (`StripMQChat`). `out`
    /// needs `strlen(line) + 1` bytes; neither step lengthens the line.
    pub fn mqemu_clean_chat_line(line: *const c_char, keep_links: bool, out: *mut c_char, cap: usize) -> bool;
    /// The name `${Spawn.Class}` shows for a class ID; false for none.
    pub fn mqemu_class_name(class: i32, out: *mut c_char, cap: usize) -> bool;
    /// The name `${Spawn.Race}` shows for a race ID; false for none.
    pub fn mqemu_race_name(race: i32, out: *mut c_char, cap: usize) -> bool;
    /// Runs the game's own handler for slash command `name` (`"/cast"`) with
    /// `args`, as you, from the game's command table (as MacroQuest finds
    /// `cmdCast`), or the first it abbreviates, as typing it does: not
    /// MacroQuest's command of that name, nor an alias. False
    /// if the game has no such command or there's no character.
    pub fn mqemu_eq_command(name: *const c_char, args: *const c_char) -> bool;
    /// How close `attacker` must be to hit `target` in melee, as
    /// `${Spawn.MaxRangeTo}` works it out (`mq::GetMeleeRange`); 14 to 75.
    /// Bytes the stack pointer moves across one call of skill function
    /// `which` (1 `GetBaseSkill`, 2 `GetPcSkillLimit`) for skill `skill`,
    /// through eqlib's declaration or as [`mqemu_skill_value`] calls it; 0 is
    /// right. For the selftest; the stack pointer is put back.
    pub fn mqemu_skill_call_stack_shift(which: i32, through_eqlib: bool, skill: i32) -> i32;
    pub fn mqemu_melee_range(attacker: *mut PlayerClient, target: *mut PlayerClient) -> f32;
    /// Your progress on `task`, one of `pTaskManager`'s `QuestEntries` or
    /// `SharedTaskEntries` (`CTaskManager::GetTaskStatus`). Null for an entry
    /// in neither, or without a character.
    pub fn mqemu_task_status(task: *mut CTaskEntry) -> *mut PCTaskStatus;
    /// What a task objective asks, as the task window says it
    /// (`${Task[n].Objective[m]}`); false for none.
    pub fn mqemu_task_objective_text(objective: *const CTaskElement, out: *mut c_char, cap: usize) -> bool;
    /// Your friend `index` (0-based); false past the end.
    pub fn mqemu_friend_name(index: i32, out: *mut c_char, cap: usize) -> bool;
    /// A chat link to `achievement` as `player` has it
    /// (`${Achievement[...].Link}`).
    pub fn mqemu_achievement_link(achievement: *const Achievement, player: *const c_char, out: *mut c_char, cap: usize) -> bool;

    rust {
        /// [`SKILL_HAS`], [`SKILL_ACTIVATABLE`] and [`SKILL_AVAILABLE`] bits.
        pub fn mqemu_skill_flags(skill: i32) -> u32;
        /// Milliseconds until you can use the skill again (`${Me.AbilityTimer}`);
        /// `total` gets its full reuse time (`${Me.AbilityTimerTotal}`).
        pub fn mqemu_skill_timer(skill: i32, total: *mut i32) -> i32;
        /// Memorizes spells as `/memspell` does (`CSpellBookWnd::MemorizeSet`):
        /// `spells` has a spell ID for each of the `count` gems, which must be
        /// [`NUM_SPELL_GEMS`](crate::constants::NUM_SPELL_GEMS); -1 leaves a gem
        /// as it is. False without the spell book window.
        pub fn mqemu_memorize_spells(spells: *const i32, count: usize) -> bool;
        /// Forgets the spell in gem `gem` (0-based), as right-clicking it does
        /// (`CCastSpellWnd::ForgetMemorizedSpell`). False for an empty gem or one
        /// past [`NUM_SPELL_GEMS`](crate::constants::NUM_SPELL_GEMS) (the client
        /// stops on a breakpoint from gem 16 on).
        pub fn mqemu_forget_gem(gem: i32) -> bool;
        /// Whether gem `gem`'s spell can be cast now, as `${Me.SpellReady[n]}`
        /// says. False for an empty gem.
        pub fn mqemu_gem_ready(gem: i32) -> bool;
        /// Uses skill `skill` on your target as `/doability <name>` does
        /// (`PcClient::UseSkill`), if you have it and it's an ability you
        /// activate. False otherwise, or without a character.
        pub fn mqemu_use_skill(skill: i32) -> bool;
        /// Uses combat ability `spell_id` as `/doability` does
        /// (`PcZoneClient::DoCombatAbility`). False without a character or when
        /// the game refuses.
        pub fn mqemu_do_combat_ability(spell_id: i32) -> bool;
        /// Milliseconds left on your buff (or with `song`, song) from spell
        /// `spell_id`, as its window counts it down each frame; -1 if it isn't
        /// in that window.
        pub fn mqemu_buff_timer(spell_id: i32, song: bool) -> i32;
        /// Your maximum HP with its modifiers (`${Me.MaxHPs}`); 0 without a
        /// character.
        pub fn mqemu_max_hp() -> i32;
        /// Your maximum endurance with its modifiers (`${Me.MaxEndurance}`); 0
        /// without a character.
        pub fn mqemu_max_endurance() -> i32;
        /// Your current HP (`${Me.CurrentHPs}`); 0 without a character.
        pub fn mqemu_hp_current() -> i32;
        /// Your maximum mana with its modifiers (`${Me.MaxMana}`); 0 without a
        /// character.
        pub fn mqemu_max_mana() -> i32;
        /// The achievement manager (`AchievementManager::Instance`).
        pub fn mqemu_achievement_manager() -> *mut AchievementManager;
        /// The alternate ability with this index (one rank of one ability), as
        /// `AltAdvManager::GetAAById` finds it; null if none.
        pub fn mqemu_alt_ability(index: i32) -> *mut CAltAbilityData;
        /// Whether you can use `ability` now; `reuse_seconds` gets its reuse time
        /// left (`${Me.AltAbilityTimer}` in seconds).
        pub fn mqemu_alt_ability_ready(ability: *mut CAltAbilityData, reuse_seconds: *mut i32) -> bool;
        /// Milliseconds left on your cast as `${Me.CastTimeLeft}` counts them
        /// (`CastingData.SpellETA - pDisplay->TimeStamp`, 0 once due), or -1 when
        /// you aren't casting.
        pub fn mqemu_cast_time_left_ms() -> i32;
        /// What your active tribute costs in favor (`${Me.ActiveFavorCost}`); 0
        /// without the tribute manager.
        pub fn mqemu_active_favor_cost() -> i32;
        /// How many friends your friends list has (`${Friends}`); -1 without the
        /// chat service that keeps it.
        pub fn mqemu_friend_count() -> i32;
        /// The game's clock in seconds (`GetFastTime`), which task timers count by.
        pub fn mqemu_fast_time() -> u32;
        /// The spell gems you have, as `${Me.NumGems}` counts them: 8, plus each
        /// of gems 9 to 12 the cast spell window shows. 8 without the window.
        pub fn mqemu_gem_count() -> i32;
    }
}

/// [`Api::mqemu_gem_count`], from the cast spell window's layout.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_gem_count() -> i32 {
    unsafe { gem_count(crate::types::foreign(&raw const crate::eqlib::pCastSpellWnd).as_ref()) }
}

/// # Safety
/// `window`'s non-null `SpellSlots` must point to live gems.
unsafe fn gem_count(window: Option<&CCastSpellWnd>) -> i32 {
    let Some(window) = window else { return 8 };
    let shown = window.SpellSlots()[8..].iter().filter(|gem| unsafe { gem.as_ref() }.is_some_and(|gem| gem.dShow())).count();
    8 + shown as i32
}

/// [`Api::mqemu_max_hp`]: `pLocalPC->Max_HP(0)`, calling eqlib directly.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_max_hp() -> i32 {
    unsafe { with_character(|pc| crate::eqlib::CharacterZoneClient_Max_HP(pc, 0, true)) }
}

/// Calls `f` with your character's `CharacterZoneClient`; 0 without one.
///
/// # Safety
/// In game, on the game thread.
unsafe fn with_character(f: impl FnOnce(*mut crate::layouts::CharacterZoneClient) -> i32) -> i32 {
    match unsafe { crate::types::foreign(&raw const crate::eqlib::pLocalPC).as_ref() } {
        Some(pc) => f(pc.as_CharacterZoneClient()),
        None => 0,
    }
}

/// [`Api::mqemu_max_endurance`]: `pLocalPC->Max_Endurance()`.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_max_endurance() -> i32 {
    unsafe { with_character(|pc| crate::eqlib::CharacterZoneClient_Max_Endurance(pc, true)) }
}

/// [`Api::mqemu_hp_current`]: `pLocalPC->Cur_HP(0)`.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_hp_current() -> i32 {
    unsafe { with_character(|pc| crate::eqlib::CharacterZoneClient_Cur_HP(pc, 0, true)) }
}

/// [`Api::mqemu_max_mana`]: `pLocalPC->Max_Mana()`.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_max_mana() -> i32 {
    unsafe { with_character(|pc| crate::eqlib::CharacterZoneClient_Max_Mana(pc, true)) }
}

/// [`Api::mqemu_fast_time`]: `GetFastTime()`.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_fast_time() -> u32 {
    unsafe { crate::eqlib::GetFastTime() }
}

/// [`Api::mqemu_friend_count`]: `pChatService->GetNumberOfFriends()`.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_friend_count() -> i32 {
    unsafe {
        let chat = crate::types::computed(&raw const crate::eqlib::pChatService);
        if chat.is_null() { -1 } else { crate::eqlib::UniversalChatProxy_GetNumberOfFriends(chat) }
    }
}

/// [`Api::mqemu_active_favor_cost`]: `pTribute->GetActiveFavorCost()`.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_active_favor_cost() -> i32 {
    unsafe {
        let tribute = crate::eqlib::pTribute;
        if tribute.is_null() { 0 } else { crate::eqlib::CTribute_GetActiveFavorCost(tribute) }
    }
}

/// [`Api::mqemu_achievement_manager`]: `&AchievementManager::Instance()`.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_achievement_manager() -> *mut AchievementManager {
    unsafe { crate::eqlib::AchievementManager_Instance() }
}

/// [`Api::mqemu_alt_ability`]: `pAltAdvManager->GetAAById(index)`.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_alt_ability(index: i32) -> *mut CAltAbilityData {
    unsafe {
        let manager = crate::types::foreign(&raw const crate::eqlib::pAltAdvManager);
        if manager.is_null() { core::ptr::null_mut() } else { crate::eqlib::AltAdvManager_GetAAById(manager, index) }
    }
}

/// [`Api::mqemu_alt_ability_ready`]: `pAltAdvManager->IsAbilityReady(pLocalPC, ability, &refresh)`.
///
/// # Safety
/// In game, on the game thread; `reuse_seconds` must be writable.
pub unsafe extern "C" fn mqemu_alt_ability_ready(ability: *mut CAltAbilityData, reuse_seconds: *mut i32) -> bool {
    unsafe {
        *reuse_seconds = 0;
        let manager = crate::types::foreign(&raw const crate::eqlib::pAltAdvManager);
        let pc = crate::types::foreign(&raw const crate::eqlib::pLocalPC);
        if manager.is_null() || pc.is_null() || ability.is_null() {
            return false;
        }
        let mut refresh = 0;
        let ready = crate::eqlib::AltAdvManager_IsAbilityReady(manager, pc, ability, &mut refresh, core::ptr::null_mut());
        *reuse_seconds = refresh.max(0);
        ready
    }
}

/// [`Api::mqemu_cast_time_left_ms`].
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_cast_time_left_ms() -> i32 {
    unsafe { cast_time_left_ms(crate::types::foreign(&raw const crate::eqlib::pDisplay).as_ref(), crate::types::foreign(&raw const crate::eqlib::pLocalPlayer).as_ref()) }
}

/// Milliseconds from the display's clock to `me`'s cast ending, 0 once due;
/// -1 without either, or when `me` isn't casting (no `SpellETA`).
fn cast_time_left_ms(display: Option<&CDisplay>, me: Option<&PlayerClient>) -> i32 {
    let (Some(display), Some(me)) = (display, me) else { return -1 };
    let eta = me.CastingData().SpellETA();
    if eta == 0 {
        return -1;
    }
    (i64::from(eta) - i64::from(display.TimeStamp())).clamp(0, i32::MAX.into()) as i32
}

/// The most profile lists [`current_profile`] walks; the client has a
/// handful. Guards against a damaged list.
const MAX_PROFILE_LISTS: usize = 64;

/// `PcClient::GetCurrentPcProfile`: the profile list the manager marks current.
pub fn current_profile(pc: &PcClient) -> Option<&PcProfile> {
    let manager = pc.ProfileManager();
    let mut list = manager.pFirst();
    for _ in 0..MAX_PROFILE_LISTS {
        // SAFETY: the lists and their profiles live as long as the PcClient.
        let current = unsafe { list.as_ref() }?;
        if current.ListType() == manager.CurProfileList() {
            return unsafe { current.pFirst().as_ref() };
        }
        list = current.pNext();
    }
    None
}

/// `mq::GetMemorizedSpell`: the spell ID in gem `gem` (0-based, below
/// [`MAX_MEMORIZED_SPELLS`]) of your current profile; -1 for an empty gem,
/// one out of range, or without a character or profile.
fn memorized_spell(pc: Option<&PcClient>, gem: i32) -> i32 {
    let Ok(gem) = usize::try_from(gem) else { return -1 };
    match pc.and_then(current_profile) {
        Some(profile) if gem < MAX_MEMORIZED_SPELLS => profile.MemorizedSpells_at(gem),
        _ => -1,
    }
}

/// `mq::GetSpellByID`: the spell with ID `id`, 1 to `TOTAL_SPELL_COUNT - 2`
/// (MQ stops below `GetMaxSpellID()`); null otherwise, for an unused ID, or
/// without the spell table. Reads `Spells`, where MQ calls the game's virtual
/// `GetSpellByID` for the same entry.
fn spell_by_id(spells: Option<&ClientSpellManager>, id: i32) -> *mut EQ_Spell {
    match (spells, usize::try_from(id)) {
        (Some(spells), Ok(id)) if id > 0 && id < TOTAL_SPELL_COUNT - 1 => spells.Spells_at(id),
        _ => core::ptr::null_mut(),
    }
}

/// `mq::SkillIsActivatable`: whether skill `skill` (0 to `NUM_SKILLS - 1`) is
/// one you activate, a button on the abilities page (`EQ_Skill::Activated`).
fn skill_activatable(skills: Option<&SkillManager>, skill: i32) -> bool {
    let Ok(skill) = usize::try_from(skill) else { return false };
    skill < NUM_SKILLS && skills.and_then(|skills| unsafe { skills.pSkill_at(skill).as_ref() }).is_some_and(|skill| skill.Activated())
}

/// The longest a buff window's lists are (`MAX_BUFF_ICONS`).
const MAX_BUFF_ICONS: i32 = 42;

/// `CBuffWindow::GetBuffInfoBySpellID(spell_id).GetBuffTimer()`: the timer
/// of the first of `window`'s buffs from spell `spell_id`, among its
/// `lastEffectSlot - firstEffectSlot` slots (at most `MAX_BUFF_ICONS`); -1
/// if it has none, or without the window.
fn buff_timer(window: Option<&CBuffWindow>, spell_id: i32) -> i32 {
    let Some(window) = window else { return -1 };
    if spell_id <= 0 {
        return -1;
    }
    let slots = (window.lastEffectSlot() - window.firstEffectSlot()).clamp(0, MAX_BUFF_ICONS) as usize;
    (0..slots).find(|&i| window.spellIds_at(i) == spell_id).map_or(-1, |i| window.buffTimers_at(i))
}

/// Whether gem `gem` (0-based, below [`NUM_SPELL_GEMS`]) can be cast now, as
/// `${Me.SpellReady[n]}` says: never with no spell in it (`spell` null) or
/// while `casting` (the casting window shows), always while
/// `bard_song_playing()`, otherwise once the display's clock is past the
/// gem's refresh (`me.SpellGemETA`) and the global cooldown.
fn gem_ready(display: Option<&CDisplay>, me: Option<&PlayerClient>, gem: i32, spell: *const EQ_Spell, casting: bool, bard_song_playing: impl FnOnce() -> bool) -> bool {
    let (Some(display), Some(me), Ok(gem)) = (display, me, usize::try_from(gem)) else { return false };
    if gem >= NUM_SPELL_GEMS || spell.is_null() || casting {
        return false;
    }
    if bard_song_playing() {
        return true;
    }
    let now = display.TimeStamp();
    now > me.SpellGemETA_at(gem) && now > me.SpellCooldownETA()
}

/// [`Api::mqemu_gem_ready`].
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_gem_ready(gem: i32) -> bool {
    unsafe {
        let spell = spell_by_id(foreign(&raw const eqlib::pSpellMgr).as_ref(), memorized_spell(foreign(&raw const eqlib::pLocalPC).as_ref(), gem));
        let casting = foreign(&raw const eqlib::pCastingWnd).as_ref().is_some_and(|window| window.dShow());
        let gems = foreign(&raw const eqlib::pCastSpellWnd);
        let bard_song_playing = || !gems.is_null() && eqlib::CCastSpellWnd_IsBardSongPlaying(gems);
        gem_ready(foreign(&raw const eqlib::pDisplay).as_ref(), foreign(&raw const eqlib::pLocalPlayer).as_ref(), gem, spell, casting, bard_song_playing)
    }
}

/// [`Api::mqemu_forget_gem`]: `pCastSpellWnd->ForgetMemorizedSpell(gem)` for
/// a gem with a spell in it.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_forget_gem(gem: i32) -> bool {
    unsafe {
        let window = foreign(&raw const eqlib::pCastSpellWnd);
        let in_range = usize::try_from(gem).is_ok_and(|gem| gem < NUM_SPELL_GEMS);
        if window.is_null() || !in_range || memorized_spell(foreign(&raw const eqlib::pLocalPC).as_ref(), gem) == -1 {
            return false;
        }
        eqlib::CCastSpellWnd_ForgetMemorizedSpell(window, gem);
        true
    }
}

/// [`Api::mqemu_use_skill`]: `pLocalPC->UseSkill(skill, pLocalPlayer)` for a
/// skill you have (`HasSkill`) and activate (`EQ_Skill::Activated`).
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_use_skill(skill: i32) -> bool {
    unsafe {
        let (Some(pc), Some(me)) = (foreign(&raw const eqlib::pLocalPC).as_ref(), foreign(&raw const eqlib::pLocalPlayer).as_ref()) else { return false };
        if !usize::try_from(skill).is_ok_and(|skill| skill < NUM_SKILLS) {
            return false;
        }
        let character = pc.as_CharacterZoneClient();
        if !eqlib::CharacterZoneClient_HasSkill(character, skill) || !skill_activatable(foreign(&raw const eqlib::pSkillMgr).as_ref(), skill) {
            return false;
        }
        eqlib::CharacterZoneClient_UseSkill(character, skill as u8, me.as_PlayerZoneClient());
        true
    }
}

/// [`Api::mqemu_do_combat_ability`]: `pLocalPC->DoCombatAbility(spell_id)`,
/// allowing a lower rank.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_do_combat_ability(spell_id: i32) -> bool {
    unsafe {
        match foreign(&raw const eqlib::pLocalPC).as_ref() {
            Some(pc) if spell_id > 0 => eqlib::PcZoneClient_DoCombatAbility(pc.as_PcZoneClient(), spell_id, true),
            _ => false,
        }
    }
}

/// [`Api::mqemu_buff_timer`], from the buff or song window.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_buff_timer(spell_id: i32, song: bool) -> i32 {
    unsafe {
        let window = if song { foreign(&raw const eqlib::pSongWnd) } else { foreign(&raw const eqlib::pBuffWnd) };
        buff_timer(window.as_ref(), spell_id)
    }
}

/// [`Api::mqemu_skill_flags`]: `HasSkill`, [`skill_activatable`] and
/// `pSkillMgr->IsAvailable`, as bits.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_skill_flags(skill: i32) -> u32 {
    unsafe {
        let pc = foreign(&raw const eqlib::pLocalPC);
        let skills = foreign(&raw const eqlib::pSkillMgr);
        let Some(pc) = pc.as_ref() else { return 0 };
        if skills.is_null() || !usize::try_from(skill).is_ok_and(|skill| skill < NUM_SKILLS) {
            return 0;
        }
        let mut flags = 0;
        if eqlib::CharacterZoneClient_HasSkill(pc.as_CharacterZoneClient(), skill) {
            flags |= SKILL_HAS;
        }
        if skill_activatable(skills.as_ref(), skill) {
            flags |= SKILL_ACTIVATABLE;
        }
        if eqlib::SkillManager_IsAvailable(skills, skill) {
            flags |= SKILL_AVAILABLE;
        }
        flags
    }
}

/// Milliseconds left on a skill timer of `total` milliseconds last started at
/// `last_used`, at `now` (the game's millisecond clock); 0 once it's over.
fn skill_time_left(total: u32, last_used: u32, now: u32) -> i32 {
    (total as i32).wrapping_sub(now.wrapping_sub(last_used) as i32).max(0)
}

/// [`Api::mqemu_skill_timer`]: `pSkillMgr->GetSkillTimerDuration` and
/// `GetSkillLastUsed`, against `EQGetTime()`.
///
/// # Safety
/// In game, on the game thread; `total` must be writable.
pub unsafe extern "C" fn mqemu_skill_timer(skill: i32, total: *mut i32) -> i32 {
    unsafe {
        *total = 0;
        let skills = foreign(&raw const eqlib::pSkillMgr);
        if skills.is_null() || !usize::try_from(skill).is_ok_and(|skill| skill < NUM_SKILLS) {
            return 0;
        }
        let duration = eqlib::SkillManager_GetSkillTimerDuration(skills, skill);
        *total = duration as i32;
        skill_time_left(duration, eqlib::SkillManager_GetSkillLastUsed(skills, skill), eqlib::EQGetTime())
    }
}

/// [`Api::mqemu_memorize_spells`]: `pSpellBookWnd->MemorizeSet` with a copy
/// of `spells`.
///
/// # Safety
/// In game, on the game thread; `spells` must hold `count` IDs.
pub unsafe extern "C" fn mqemu_memorize_spells(spells: *const i32, count: usize) -> bool {
    unsafe {
        let window = foreign(&raw const eqlib::pSpellBookWnd);
        if window.is_null() || foreign(&raw const eqlib::pLocalPC).is_null() || count != NUM_SPELL_GEMS {
            return false;
        }
        let mut loadout = [0; NUM_SPELL_GEMS];
        loadout.copy_from_slice(core::slice::from_raw_parts(spells, NUM_SPELL_GEMS));
        eqlib::CSpellBookWnd_MemorizeSet(window, loadout.as_mut_ptr(), NUM_SPELL_GEMS as i32);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layouts::CSpellGemWnd;

    extern crate std;
    use std::{vec, vec::Vec};

    /// Zeroed storage for a game object of type `T`.
    fn zeroed<T>() -> (Vec<u64>, *mut T) {
        let mut storage = vec![0u64; size_of::<T>().div_ceil(8)];
        let ptr = storage.as_mut_ptr().cast();
        (storage, ptr)
    }

    #[test]
    fn cast_time_left_counts_from_the_display_clock_to_the_spell_eta() {
        let (_display_storage, display) = zeroed::<CDisplay>();
        let (_me_storage, me) = zeroed::<PlayerClient>();
        let (display, me) = unsafe { (&*display, &*me) };
        display.set_TimeStamp(10_000);
        assert_eq!(cast_time_left_ms(Some(display), Some(me)), -1, "not casting");
        me.CastingData().set_SpellETA(12_500);
        assert_eq!(cast_time_left_ms(Some(display), Some(me)), 2_500);
        me.CastingData().set_SpellETA(9_000);
        assert_eq!(cast_time_left_ms(Some(display), Some(me)), 0, "due");
        assert_eq!(cast_time_left_ms(None, Some(me)), -1);
        assert_eq!(cast_time_left_ms(Some(display), None), -1);
    }

    #[test]
    fn memorized_spell_reads_the_current_profiles_gems() {
        let (_pc_storage, pc) = zeroed::<PcClient>();
        let (_list_storage, list) = zeroed::<crate::layouts::ProfileList>();
        let (_profile_storage, profile) = zeroed::<PcProfile>();
        let (pc, list_ref, profile_ref) = unsafe { (&*pc, &*list, &*profile) };
        assert_eq!(memorized_spell(None, 0), -1, "no character");
        assert_eq!(memorized_spell(Some(pc), 0), -1, "no profile");
        list_ref.set_pFirst(profile);
        pc.ProfileManager().set_pFirst(list);
        profile_ref.set_MemorizedSpells_at(0, 202);
        profile_ref.set_MemorizedSpells_at(15, 4_000);
        profile_ref.set_MemorizedSpells_at(3, -1);
        assert_eq!(memorized_spell(Some(pc), 0), 202);
        assert_eq!(memorized_spell(Some(pc), 15), 4_000);
        assert_eq!(memorized_spell(Some(pc), 3), -1, "empty gem");
        assert_eq!(memorized_spell(Some(pc), 16), -1, "past the last gem");
        assert_eq!(memorized_spell(Some(pc), -1), -1);
    }

    #[test]
    fn spell_by_id_takes_ids_1_to_44999() {
        let (_spells_storage, spells) = zeroed::<ClientSpellManager>();
        let (_spell_storage, spell) = zeroed::<EQ_Spell>();
        let spells = unsafe { &*spells };
        for id in [0, 1, 44_999, 45_000] {
            spells.set_Spells_at(id, spell);
        }
        assert_eq!(spell_by_id(Some(spells), 1), spell);
        assert_eq!(spell_by_id(Some(spells), 44_999), spell);
        assert!(spell_by_id(Some(spells), 2).is_null(), "unused ID");
        for id in [0, -1, 45_000, 45_001] {
            assert!(spell_by_id(Some(spells), id).is_null(), "ID {id}");
        }
        assert!(spell_by_id(None, 1).is_null(), "no spell table");
    }

    #[test]
    fn skill_activatable_reads_the_skills_activated_flag() {
        let (_skills_storage, skills) = zeroed::<SkillManager>();
        let (_skill_storage, skill) = zeroed::<crate::layouts::EQ_Skill>();
        let skills = unsafe { &*skills };
        assert!(!skill_activatable(Some(skills), 5), "no skill");
        skills.set_pSkill_at(5, skill);
        assert!(!skill_activatable(Some(skills), 5), "not activated");
        unsafe { &*skill }.set_Activated(true);
        assert!(skill_activatable(Some(skills), 5));
        assert!(!skill_activatable(None, 5), "no skill manager");
        assert!(!skill_activatable(Some(skills), -1));
        assert!(!skill_activatable(Some(skills), NUM_SKILLS as i32));
    }

    #[test]
    fn buff_timer_finds_the_spell_among_the_windows_slots() {
        let (_window_storage, window) = zeroed::<CBuffWindow>();
        let window = unsafe { &*window };
        window.set_lastEffectSlot(42);
        window.set_spellIds_at(3, 1_234);
        window.set_buffTimers_at(3, 60_000);
        window.set_spellIds_at(41, 99);
        window.set_buffTimers_at(41, 6_000);
        assert_eq!(buff_timer(Some(window), 1_234), 60_000);
        assert_eq!(buff_timer(Some(window), 99), 6_000, "the last slot");
        assert_eq!(buff_timer(Some(window), 7), -1, "not there");
        assert_eq!(buff_timer(Some(window), 0), -1, "empty slots don't match spell 0");
        assert_eq!(buff_timer(None, 1_234), -1);

        // A song window: effect slots 42 on, 30 of them.
        window.set_firstEffectSlot(42);
        window.set_lastEffectSlot(72);
        assert_eq!(buff_timer(Some(window), 1_234), 60_000);
        assert_eq!(buff_timer(Some(window), 99), -1, "past the song window's 30 slots");
        window.set_lastEffectSlot(200);
        assert_eq!(buff_timer(Some(window), 99), 6_000, "MAX_BUFF_ICONS at most");
        window.set_lastEffectSlot(0);
        assert_eq!(buff_timer(Some(window), 1_234), -1, "no slots");
    }

    #[test]
    fn gem_ready_waits_for_the_gem_refresh_and_the_global_cooldown() {
        let (_display_storage, display) = zeroed::<CDisplay>();
        let (_me_storage, me) = zeroed::<PlayerClient>();
        let (_spell_storage, spell) = zeroed::<EQ_Spell>();
        let (display, me) = unsafe { (&*display, &*me) };
        let spell = spell.cast_const();
        let ready = |gem, spell, casting, bard| gem_ready(Some(display), Some(me), gem, spell, casting, || bard);
        display.set_TimeStamp(10_000);
        me.set_SpellGemETA_at(2, 12_000);
        me.set_SpellCooldownETA(9_000);
        assert!(ready(0, spell, false, false));
        assert!(!ready(2, spell, false, false), "gem refreshing");
        assert!(ready(2, spell, false, true), "a bard song plays");
        me.set_SpellCooldownETA(11_000);
        assert!(!ready(0, spell, false, false), "global cooldown");
        me.set_SpellCooldownETA(10_000);
        assert!(!ready(0, spell, false, false), "due isn't past");
        me.set_SpellCooldownETA(0);
        assert!(!ready(0, spell, true, true), "casting");
        assert!(!ready(0, core::ptr::null(), false, true), "empty gem");
        assert!(!ready(NUM_SPELL_GEMS as i32, spell, false, true));
        assert!(!ready(-1, spell, false, true));
        assert!(!gem_ready(None, Some(me), 0, spell, false, || true));
        assert!(!gem_ready(Some(display), None, 0, spell, false, || true));
    }

    #[test]
    fn skill_time_left_counts_down_from_the_last_use() {
        assert_eq!(skill_time_left(10_000, 50_000, 53_000), 7_000);
        assert_eq!(skill_time_left(10_000, 50_000, 60_000), 0, "due");
        assert_eq!(skill_time_left(10_000, 50_000, 90_000), 0, "long over");
        assert_eq!(skill_time_left(0, 0, 123), 0, "no timer");
        assert_eq!(skill_time_left(10_000, u32::MAX - 999, 2_000), 7_000, "across the clock's wrap");
    }

    #[test]
    fn gem_count_is_8_plus_the_shown_gems_past_the_eighth() {
        let (_window_storage, window) = zeroed::<CCastSpellWnd>();
        let window = unsafe { &*window };
        let gems: Vec<_> = (0..12).map(|_| zeroed::<CSpellGemWnd>()).collect();
        assert_eq!(unsafe { gem_count(None) }, 8, "no window");
        assert_eq!(unsafe { gem_count(Some(window)) }, 8, "no gems");

        for (slot, (_, gem)) in gems.iter().enumerate() {
            window.set_SpellSlots_at(slot, *gem);
            unsafe { &**gem }.set_dShow(true);
        }
        assert_eq!(unsafe { gem_count(Some(window)) }, 12, "all 12 shown");

        unsafe { &*gems[9].1 }.set_dShow(false);
        window.set_SpellSlots_at(11, core::ptr::null_mut());
        assert_eq!(unsafe { gem_count(Some(window)) }, 10, "gem 10 hidden, gem 12 missing");

        unsafe { &*gems[0].1 }.set_dShow(false);
        assert_eq!(unsafe { gem_count(Some(window)) }, 10, "the first 8 always count");
    }
}
