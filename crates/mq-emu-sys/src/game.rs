//! Game data only the game's (or MacroQuest's) own functions can answer, from
//! `shim/game.cpp`: MacroQuest's cache of other spawns' buffs, alternate
//! abilities, skill values and caps, the game's string table and extended
//! target role names.
//!
//! These import `MQ2Main.dll` and `eqlib.dll`, so they only run inside the
//! game; call them through [`API`] (see [`function_table!`]). Text comes back
//! NUL-terminated in the caller's buffer, cut to fit; functions return
//! "nothing" (null, 0, false) when the game objects they need are missing.

use core::ffi::c_char;

use crate::layouts::{CAltAbilityData, PlayerClient};

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
}
