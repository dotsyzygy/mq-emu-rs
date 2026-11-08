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

use crate::layouts::{Achievement, AchievementManager, CAltAbilityData, CCastSpellWnd, CTaskElement, CTaskEntry, PCTaskStatus, PlayerClient};

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
    /// The alternate ability with this index (one rank of one ability), as
    /// `AltAdvManager::GetAAById` finds it; null if none.
    pub fn mqemu_alt_ability(index: i32) -> *mut CAltAbilityData;
    /// Your auras as the aura window lists them (`ClientAuraManager`, what
    /// `${Me.Aura}` reads): up to `cap` into `out`. Returns how many there are.
    pub fn mqemu_auras(out: *mut mqemu_aura, cap: usize) -> usize;
    /// Whether you can use `ability` now; `reuse_seconds` gets its reuse time
    /// left (`${Me.AltAbilityTimer}` in seconds).
    pub fn mqemu_alt_ability_ready(ability: *mut CAltAbilityData, reuse_seconds: *mut i32) -> bool;
    /// The game's string `id` of `kind` (`DB_STRING_*`); false if there's none.
    pub fn mqemu_db_string(id: i32, kind: i32, out: *mut c_char, cap: usize) -> bool;
    /// Your skill `skill`: `which` 0 with modifiers (`${Me.Skill}`), 1
    /// trained (`${Me.SkillBase}`), 2 the cap at your level (`${Me.SkillCap}`).
    pub fn mqemu_skill_value(skill: i32, which: i32) -> i32;
    /// [`SKILL_HAS`], [`SKILL_ACTIVATABLE`] and [`SKILL_AVAILABLE`] bits.
    pub fn mqemu_skill_flags(skill: i32) -> u32;
    /// Milliseconds until you can use the skill again (`${Me.AbilityTimer}`);
    /// `total` gets its full reuse time (`${Me.AbilityTimerTotal}`).
    pub fn mqemu_skill_timer(skill: i32, total: *mut i32) -> i32;
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
    /// Milliseconds left on your cast as `${Me.CastTimeLeft}` counts them
    /// (`CastingData.SpellETA - pDisplay->TimeStamp`, 0 once due), or -1 when
    /// you aren't casting.
    pub fn mqemu_cast_time_left_ms() -> i32;
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
    /// The achievement manager (`AchievementManager::Instance`).
    pub fn mqemu_achievement_manager() -> *mut AchievementManager;
    /// A chat link to `achievement` as `player` has it
    /// (`${Achievement[...].Link}`).
    pub fn mqemu_achievement_link(achievement: *const Achievement, player: *const c_char, out: *mut c_char, cap: usize) -> bool;

    rust {
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
