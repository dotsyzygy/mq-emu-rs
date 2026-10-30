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

use crate::layouts::{Achievement, AchievementManager, CAltAbilityData, CTaskElement, CTaskEntry, PCTaskStatus, PlayerClient};

/// One of MacroQuest's cached buffs (`mq::CachedBuff`).
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
    /// Your maximum endurance with its modifiers (`${Me.MaxEndurance}`); 0
    /// without a character.
    pub fn mqemu_max_endurance() -> i32;
    /// The name `${Spawn.Class}` shows for a class ID; false for none.
    pub fn mqemu_class_name(class: i32, out: *mut c_char, cap: usize) -> bool;
    /// The name `${Spawn.Race}` shows for a race ID; false for none.
    pub fn mqemu_race_name(race: i32, out: *mut c_char, cap: usize) -> bool;
    /// Runs the game's own handler for slash command `name` (`"/cast"`) with
    /// `args`, as you, from the game's command table (as MacroQuest finds
    /// `cmdCast`): not MacroQuest's command of that name, nor an alias. False
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
    /// Your progress on `task`, one of `pTaskManager`'s `QuestEntries` or
    /// `SharedTaskEntries` (`CTaskManager::GetTaskStatus`). Null for an entry
    /// in neither, or without a character.
    pub fn mqemu_task_status(task: *mut CTaskEntry) -> *mut PCTaskStatus;
    /// What a task objective asks, as the task window says it
    /// (`${Task[n].Objective[m]}`); false for none.
    pub fn mqemu_task_objective_text(objective: *const CTaskElement, out: *mut c_char, cap: usize) -> bool;
    /// The game's clock in seconds (`GetFastTime`), which task timers count by.
    pub fn mqemu_fast_time() -> u32;
    /// How many friends your friends list has (`${Friends}`); -1 without the
    /// chat service that keeps it.
    pub fn mqemu_friend_count() -> i32;
    /// Your friend `index` (0-based); false past the end.
    pub fn mqemu_friend_name(index: i32, out: *mut c_char, cap: usize) -> bool;
    /// What your active tribute costs in favor (`${Me.ActiveFavorCost}`); 0
    /// without the tribute manager.
    pub fn mqemu_active_favor_cost() -> i32;
    /// The achievement manager (`AchievementManager::Instance`).
    pub fn mqemu_achievement_manager() -> *mut AchievementManager;
    /// A chat link to `achievement` as `player` has it
    /// (`${Achievement[...].Link}`).
    pub fn mqemu_achievement_link(achievement: *const Achievement, player: *const c_char, out: *mut c_char, cap: usize) -> bool;
}
