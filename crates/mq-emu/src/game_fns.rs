//! Calling the game's own functions (`mq_emu_sys::game`) through
//! [`Host::game_fns`](crate::host::Host), your character's HP, mana,
//! endurance and state, reading the game's arrays, and a fake of the
//! functions for tests.

use std::ffi::{CString, c_char};

use mq_emu_sys::constants::{COMBAT_STATE_COMBAT, COMBAT_STATE_DEBUFF, COMBAT_STATE_REGEN, COMBAT_STATE_TIMER};
use mq_emu_sys::layouts::eCharacterStatus;

use crate::game::Spawn;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::from_c_array;

impl<P: Plugin> Mq<P> {
    /// Your current HP (`${Me.CurrentHPs}`), if in game. Your spawn's
    /// [`Spawn::hp_current`](crate::game::Spawn::hp_current) may lag behind it.
    pub fn hp_current(&self) -> Option<i32> {
        self.local_pc()?;
        Some(unsafe { (self.host().game_fns.mqemu_hp_current)() })
    }

    /// Your maximum HP, with its modifiers (`${Me.MaxHPs}`), if in game.
    pub fn hp_max(&self) -> Option<i32> {
        self.local_pc()?;
        Some(unsafe { (self.host().game_fns.mqemu_max_hp)() })
    }

    /// Your HP as 0..=100, from [`Mq::hp_current`] and [`Mq::hp_max`]
    /// (`${Me.PctHPs}`), if in game.
    pub fn hp_percent(&self) -> Option<i32> {
        Some(percent(self.hp_current()?, self.hp_max()?))
    }

    /// Your current mana (`${Me.CurrentMana}`), if in game. Not your spawn's
    /// [`Spawn::mana_current`](crate::game::Spawn::mana_current): in game
    /// that read 96 where this read 10277.
    pub fn mana_current(&self) -> Option<i32> {
        Some(self.profile()?.Mana())
    }

    /// Your maximum mana, with its modifiers (`${Me.MaxMana}`), if in game.
    pub fn mana_max(&self) -> Option<i32> {
        self.local_pc()?;
        Some(unsafe { (self.host().game_fns.mqemu_max_mana)() })
    }

    /// Whether auto attack is on (`${Me.Combat}`). Not whether you're in
    /// combat: see [`Mq::combat_state`].
    pub fn is_auto_attacking(&self) -> bool {
        unsafe { (self.host().everquest_info)().as_ref() }.is_some_and(|info| info.bAutoAttack())
    }

    /// Turns auto attack on or off with the game's `/attack on|off`, which
    /// attacks your target. False out of game.
    pub fn set_auto_attack(&self, on: bool) -> bool {
        self.local_pc().is_some() && self.eq_command("/attack", if on { "on" } else { "off" })
    }

    /// Sits down, or stands up for `false`, through the game's `/sit on` and
    /// `/stand on` as `${Me.Sit}` and `${Me.Stand}` do (plain `/sit`
    /// toggles). Whether it's done shows in `Spawn::is_sitting` once the
    /// game has it. False with no character in game.
    pub fn set_sitting(&self, sit: bool) -> bool {
        self.local_pc().is_some() && self.eq_command(if sit { "/sit" } else { "/stand" }, "on")
    }

    /// How close `attacker` must be to hit `target` in melee, from both
    /// spawns' sizes and heights: 14 to 75. Yours to your target is
    /// `${Target.MaxRangeTo}`.
    pub fn melee_range(&self, attacker: &Spawn<'_>, target: &Spawn<'_>) -> f32 {
        let raw = |spawn: &Spawn<'_>| std::ptr::from_ref(spawn.raw()).cast_mut();
        unsafe { (self.host().game_fns.mqemu_melee_range)(raw(attacker), raw(target)) }
    }

    /// Whether auto attack is on, despite the name.
    #[deprecated(since = "0.1.6", note = "this is auto attack: use `is_auto_attacking`, or `combat_state` for combat")]
    pub fn in_combat(&self) -> bool {
        self.is_auto_attacking()
    }

    /// Your combat state, as the player window's icon shows it
    /// (`${Me.CombatState}`), if in game. MacroQuest works it out each pulse.
    pub fn combat_state(&self) -> Option<CombatState> {
        self.local_pc()?;
        Some(CombatState::from((self.host().combat_state)()))
    }

    /// Whether you're stunned (`${Me.Stunned}`, your character's status).
    pub fn is_stunned(&self) -> bool {
        self.local_pc().is_some_and(|pc| pc.Status() == eCharacterStatus::eCharStatusStunned)
    }

    /// The short name of the server you're on (`${EverQuest.Server}`); `None`
    /// before you've picked one.
    pub fn server_name(&self) -> Option<String> {
        let info = unsafe { (self.host().everquest_info)().as_ref() }?;
        Some(from_c_array(&info.WorldServerShortname())).filter(|name| !name.is_empty())
    }

    /// Your current endurance (`${Me.CurrentEndurance}`), if in game.
    pub fn endurance_current(&self) -> Option<i32> {
        Some(self.profile()?.Endurance())
    }

    /// Your maximum endurance, with its modifiers (`${Me.MaxEndurance}`), if
    /// in game.
    pub fn endurance_max(&self) -> Option<i32> {
        self.local_pc()?;
        Some(unsafe { (self.host().game_fns.mqemu_max_endurance)() })
    }

    /// Your endurance as 0..=100, from [`Mq::endurance_current`] and
    /// [`Mq::endurance_max`] (`${Me.PctEndurance}`), if in game.
    pub fn endurance_percent(&self) -> Option<i32> {
        Some(percent(self.endurance_current()?, self.endurance_max()?))
    }

    /// The name of a class ID ([`Spawn::class`](crate::game::Spawn::class)),
    /// as `${Spawn.Class}` shows it: "Warrior".
    pub fn class_name(&self, class: u8) -> Option<String> {
        let api = self.host().game_fns;
        text(|out, cap| unsafe { (api.mqemu_class_name)(i32::from(class), out, cap) })
    }

    /// The name of a race ID ([`Spawn::race`](crate::game::Spawn::race)), as
    /// `${Spawn.Race}` shows it: "Human".
    pub fn race_name(&self, race: i32) -> Option<String> {
        let api = self.host().game_fns;
        text(|out, cap| unsafe { (api.mqemu_race_name)(race, out, cap) })
    }

    /// Runs the game's own handler for slash command `name` with `args`, as
    /// MacroQuest's `/cast` runs the game's: not MacroQuest's command of that
    /// name, nor an alias. `name` may abbreviate the game's command, as when
    /// typed ("/alt", which the game names longer). False if the game has no
    /// such command.
    pub(crate) fn eq_command(&self, name: &str, args: &str) -> bool {
        let (Ok(name), Ok(args)) = (CString::new(name), CString::new(args)) else { return false };
        unsafe { (self.host().game_fns.mqemu_eq_command)(name.as_ptr(), args.as_ptr()) }
    }
}

/// `current` as a percentage of `max`, 0..=100; 0 without a maximum.
fn percent(current: i32, max: i32) -> i32 {
    if max <= 0 { 0 } else { (i64::from(current) * 100 / i64::from(max)).clamp(0, 100) as i32 }
}

/// Your combat state (`${Me.CombatState}`), the icon on the player window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CombatState {
    /// In combat ("COMBAT").
    Combat,
    /// Out of combat with a detrimental effect on you ("DEBUFFED").
    Debuffed,
    /// Out of combat, waiting for the timer before resting ("COOLDOWN").
    Cooldown,
    /// Out of combat, standing ("ACTIVE").
    Active,
    /// Out of combat and regenerating faster: sitting, or on a mount that
    /// allows it ("RESTING").
    Resting,
}

impl CombatState {
    /// The name `${Me.CombatState}` gives it ("COMBAT", "ACTIVE"...).
    pub fn name(self) -> &'static str {
        match self {
            CombatState::Combat => "COMBAT",
            CombatState::Debuffed => "DEBUFFED",
            CombatState::Cooldown => "COOLDOWN",
            CombatState::Active => "ACTIVE",
            CombatState::Resting => "RESTING",
        }
    }
}

impl From<i32> for CombatState {
    /// Like `${Me.CombatState}`, an unknown value is [`CombatState::Active`].
    fn from(value: i32) -> CombatState {
        match value {
            COMBAT_STATE_COMBAT => CombatState::Combat,
            COMBAT_STATE_DEBUFF => CombatState::Debuffed,
            COMBAT_STATE_TIMER => CombatState::Cooldown,
            COMBAT_STATE_REGEN => CombatState::Resting,
            _ => CombatState::Active,
        }
    }
}

/// Runs a shim function that writes NUL-terminated text into a buffer and
/// says whether there was any.
pub(crate) fn text(f: impl FnOnce(*mut c_char, usize) -> bool) -> Option<String> {
    let mut buffer = [0 as c_char; 512];
    f(buffer.as_mut_ptr(), buffer.len()).then(|| from_c_array(&buffer))
}

/// The most elements [`game_array`] reads, so a damaged length can't send a
/// walk through all of memory. Far above any real list it's used on.
pub(crate) const MAX_ARRAY_LEN: usize = 4096;

/// The `len` elements at `data` (an `ArrayClass`'s `m_array` and
/// `m_length`): empty if `data` is null or `len` isn't positive, and at most
/// [`MAX_ARRAY_LEN`].
///
/// # Safety
/// Unless null, `data` must point to `len` (or [`MAX_ARRAY_LEN`], if fewer)
/// live elements for `'a`.
pub(crate) unsafe fn game_array<'a, T>(data: *const T, len: i32) -> &'a [T] {
    let len = usize::try_from(len).unwrap_or(0).min(MAX_ARRAY_LEN);
    if data.is_null() || len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(data, len) } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_arrays_are_bounded_and_null_safe() {
        let values = [1, 2, 3];
        assert_eq!(unsafe { game_array(values.as_ptr(), 2) }, [1, 2]);
        assert!(unsafe { game_array(values.as_ptr(), 0) }.is_empty());
        assert!(unsafe { game_array(values.as_ptr(), -1) }.is_empty());
        assert!(unsafe { game_array(std::ptr::null::<i32>(), 3) }.is_empty());
        let many = vec![0u8; MAX_ARRAY_LEN + 10];
        assert_eq!(unsafe { game_array(many.as_ptr(), i32::MAX) }.len(), MAX_ARRAY_LEN);
    }

    #[test]
    fn text_is_none_unless_the_function_says_there_is_some() {
        assert_eq!(text(|_, _| false), None);
        assert_eq!(text(|out, _| unsafe { *out = b'A' as c_char; true }), Some("A".to_owned()));
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! Stand-ins for the shim's game functions, answering from thread-locals
    //! a test fills in.

    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::c_char;

    use mq_emu_sys::game::{Api, SKILL_ACTIVATABLE, SKILL_HAS, mqemu_cached_buff};
    use mq_emu_sys::layouts::{Achievement, AchievementManager, CAltAbilityData, CTaskElement, CTaskEntry, PCTaskStatus, PlayerClient};

    /// A skill as the fake game knows it.
    #[derive(Debug, Clone, Default)]
    pub(crate) struct FakeSkill {
        pub name: String,
        /// With modifiers, trained, cap.
        pub values: [i32; 3],
        pub flags: u32,
        /// Reuse left and total, in ms.
        pub timer: (i32, i32),
    }

    pub(crate) struct FakeGame {
        /// Cached buffs by spawn.
        pub cached_buffs: HashMap<usize, Vec<mqemu_cached_buff>>,
        /// Alternate abilities by index.
        pub alt_abilities: HashMap<i32, *mut CAltAbilityData>,
        /// Ready and reuse seconds, by ability.
        pub alt_ability_ready: HashMap<usize, (bool, i32)>,
        /// The string table, by (id, kind).
        pub strings: HashMap<(i32, i32), String>,
        pub skills: HashMap<i32, FakeSkill>,
        pub xtarget_roles: HashMap<u32, String>,
        pub max_endurance: i32,
        /// Current and maximum HP.
        pub hp: (i32, i32),
        pub max_mana: i32,
        pub class_names: HashMap<i32, String>,
        pub race_names: HashMap<i32, String>,
        /// Every action taken (commands, memorizing, forgetting), in order.
        pub log: Vec<String>,
        /// The game's slash commands, by lowercase name.
        pub commands: Vec<String>,
        /// Whether memorizing and forgetting work.
        pub spells_work: bool,
        /// The gems that are ready to cast.
        pub ready_gems: Vec<i32>,
        /// What `mqemu_gem_count` says: 8.
        pub gem_count: i32,
        /// What `mqemu_cast_time_left_ms` says: -1, not casting.
        pub cast_time_left_ms: i32,
        /// Your auras.
        pub auras: Vec<mq_emu_sys::game::mqemu_aura>,
        /// Buff and song window timers (ms), by spell ID and whether a song.
        pub buff_timers: HashMap<(i32, bool), i32>,
        /// Melee ranges, by attacker and target.
        pub melee_ranges: HashMap<(usize, usize), f32>,
        /// Task progress, by task entry.
        pub task_statuses: HashMap<usize, *mut PCTaskStatus>,
        /// Task objective text, by objective.
        pub objective_texts: HashMap<usize, String>,
        /// The game's clock, in seconds.
        pub fast_time: u32,
        /// The friends list; `None` without the chat service.
        pub friends: Option<Vec<String>>,
        pub active_favor_cost: i32,
        pub achievement_manager: *mut AchievementManager,
    }

    impl Default for FakeGame {
        fn default() -> FakeGame {
            FakeGame {
                cached_buffs: HashMap::new(),
                alt_abilities: HashMap::new(),
                alt_ability_ready: HashMap::new(),
                strings: HashMap::new(),
                skills: HashMap::new(),
                xtarget_roles: HashMap::new(),
                max_endurance: 0,
                hp: (0, 0),
                max_mana: 0,
                class_names: HashMap::new(),
                race_names: HashMap::new(),
                log: Vec::new(),
                commands: Vec::new(),
                spells_work: false,
                ready_gems: Vec::new(),
                gem_count: 8,
                cast_time_left_ms: -1,
                auras: Vec::new(),
                buff_timers: HashMap::new(),
                melee_ranges: HashMap::new(),
                task_statuses: HashMap::new(),
                objective_texts: HashMap::new(),
                fast_time: 0,
                friends: None,
                active_favor_cost: 0,
                achievement_manager: std::ptr::null_mut(),
            }
        }
    }

    thread_local! {
        pub(crate) static GAME: RefCell<FakeGame> = RefCell::new(FakeGame::default());
    }

    /// Changes the fake game for this thread.
    pub(crate) fn with<R>(f: impl FnOnce(&mut FakeGame) -> R) -> R {
        GAME.with(|game| f(&mut game.borrow_mut()))
    }

    fn write(text: Option<&str>, out: *mut c_char, cap: usize) -> bool {
        let Some(text) = text else { return false };
        let len = text.len().min(cap - 1);
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr().cast(), out, len);
            *out.add(len) = 0;
        }
        true
    }

    unsafe extern "C" fn cached_buffs(spawn: *mut PlayerClient, out: *mut mqemu_cached_buff, cap: usize) -> usize {
        with(|game| {
            let buffs = game.cached_buffs.get(&(spawn as usize)).cloned().unwrap_or_default();
            for (i, buff) in buffs.iter().take(cap).enumerate() {
                unsafe { *out.add(i) = *buff };
            }
            buffs.len()
        })
    }

    unsafe extern "C" fn alt_ability(index: i32) -> *mut CAltAbilityData {
        with(|game| game.alt_abilities.get(&index).copied().unwrap_or(std::ptr::null_mut()))
    }

    unsafe extern "C" fn alt_ability_ready(ability: *mut CAltAbilityData, reuse_seconds: *mut i32) -> bool {
        let (ready, reuse) = with(|game| game.alt_ability_ready.get(&(ability as usize)).copied().unwrap_or((false, 0)));
        unsafe { *reuse_seconds = reuse };
        ready
    }

    unsafe extern "C" fn db_string(id: i32, kind: i32, out: *mut c_char, cap: usize) -> bool {
        with(|game| write(game.strings.get(&(id, kind)).map(String::as_str), out, cap))
    }

    unsafe extern "C" fn max_endurance() -> i32 {
        with(|game| game.max_endurance)
    }

    unsafe extern "C" fn hp_current() -> i32 {
        with(|game| game.hp.0)
    }

    unsafe extern "C" fn max_hp() -> i32 {
        with(|game| game.hp.1)
    }

    unsafe extern "C" fn max_mana() -> i32 {
        with(|game| game.max_mana)
    }

    unsafe extern "C" fn class_name(class: i32, out: *mut c_char, cap: usize) -> bool {
        with(|game| write(game.class_names.get(&class).map(String::as_str), out, cap))
    }

    unsafe extern "C" fn race_name(race: i32, out: *mut c_char, cap: usize) -> bool {
        with(|game| write(game.race_names.get(&race).map(String::as_str), out, cap))
    }

    unsafe extern "C" fn skill_value(skill: i32, which: i32) -> i32 {
        with(|game| game.skills.get(&skill).map_or(0, |s| s.values[which as usize]))
    }

    unsafe extern "C" fn skill_flags(skill: i32) -> u32 {
        with(|game| game.skills.get(&skill).map_or(0, |s| s.flags))
    }

    unsafe extern "C" fn skill_timer(skill: i32, total: *mut i32) -> i32 {
        let (left, all) = with(|game| game.skills.get(&skill).map_or((0, 0), |s| s.timer));
        unsafe { *total = all };
        left
    }

    unsafe extern "C" fn skill_name(skill: i32, out: *mut c_char, cap: usize) -> bool {
        with(|game| write(game.skills.get(&skill).map(|s| s.name.as_str()), out, cap))
    }

    unsafe extern "C" fn xtarget_role_name(role: u32, out: *mut c_char, cap: usize) -> bool {
        with(|game| write(game.xtarget_roles.get(&role).map(String::as_str), out, cap))
    }

    /// RoF2's item link: `\x12`, a 56-character body, the item's name, `\x12`.
    pub(crate) const LINK_BODY: usize = 56;

    /// A stand-in for the game's `CleanItemTags` and MacroQuest's
    /// `StripMQChat`: links become their names (unless kept), then color
    /// codes (`\a` and a letter, or `\a#` and six hex digits) and line breaks go.
    pub(crate) fn clean_chat_line(line: &[u8], keep_links: bool) -> Vec<u8> {
        let mut text = Vec::new();
        let mut rest = line;
        while !keep_links && let Some(start) = rest.iter().position(|&b| b == 0x12) {
            text.extend_from_slice(&rest[..start]);
            let link = &rest[start + 1..];
            let end = link.iter().position(|&b| b == 0x12).unwrap_or(link.len());
            text.extend_from_slice(link[..end].get(LINK_BODY..).unwrap_or_default());
            rest = link.get(end + 1..).unwrap_or_default();
        }
        text.extend_from_slice(rest);
        let mut out = Vec::new();
        let mut i = 0;
        while i < text.len() {
            match text[i] {
                0x07 => i += if text.get(i + 1) == Some(&b'#') { 8 } else { 2 },
                b'\n' => i += 1,
                b => {
                    out.push(b);
                    i += 1;
                }
            }
        }
        out
    }

    unsafe extern "C" fn clean_chat(line: *const c_char, keep_links: bool, out: *mut c_char, cap: usize) -> bool {
        let line = unsafe { std::ffi::CStr::from_ptr(line) }.to_bytes();
        let cleaned = String::from_utf8_lossy(&clean_chat_line(line, keep_links)).into_owned();
        write(Some(&cleaned), out, cap)
    }

    /// What the fake game did, taken out.
    pub(crate) fn log() -> Vec<String> {
        with(|game| std::mem::take(&mut game.log))
    }

    unsafe extern "C" fn eq_command(name: *const c_char, args: *const c_char) -> bool {
        let name = unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().to_lowercase();
        let args = unsafe { std::ffi::CStr::from_ptr(args) }.to_string_lossy();
        with(|game| {
            // As the shim does: the exact name, or else the first it abbreviates.
            let found = game.commands.iter().find(|c| **c == name).or_else(|| game.commands.iter().find(|c| c.starts_with(&name))).cloned();
            if let Some(command) = &found {
                game.log.push(format!("{command} {args}").trim_end().to_owned());
            }
            found.is_some()
        })
    }

    unsafe extern "C" fn memorize_spells(spells: *const i32, count: usize) -> bool {
        let spells = unsafe { std::slice::from_raw_parts(spells, count) };
        with(|game| {
            game.log.push(format!("memorize {spells:?}"));
            game.spells_work
        })
    }

    unsafe extern "C" fn forget_gem(gem: i32) -> bool {
        with(|game| {
            game.log.push(format!("forget {gem}"));
            game.spells_work
        })
    }

    unsafe extern "C" fn gem_ready(gem: i32) -> bool {
        with(|game| game.ready_gems.contains(&gem))
    }

    unsafe extern "C" fn gem_count() -> i32 {
        with(|game| game.gem_count)
    }

    unsafe extern "C" fn cast_time_left_ms() -> i32 {
        with(|game| game.cast_time_left_ms)
    }

    /// Uses skills the fake game has as abilities.
    unsafe extern "C" fn use_skill(skill: i32) -> bool {
        with(|game| {
            let usable = game.skills.get(&skill).is_some_and(|s| s.flags & (SKILL_HAS | SKILL_ACTIVATABLE) == SKILL_HAS | SKILL_ACTIVATABLE);
            if usable {
                game.log.push(format!("skill {skill}"));
            }
            usable
        })
    }

    unsafe extern "C" fn do_combat_ability(spell_id: i32) -> bool {
        with(|game| {
            game.log.push(format!("disc {spell_id}"));
            game.spells_work
        })
    }

    /// As the game's functions move it: eqlib's calls 4 bytes off (up for
    /// GetBaseSkill, down for GetPcSkillLimit), mq-emu's not at all.
    unsafe extern "C" fn skill_call_stack_shift(which: i32, through_eqlib: bool, _skill: i32) -> i32 {
        match (through_eqlib, which) {
            (true, 1) => 4,
            (true, _) => -4,
            (false, _) => 0,
        }
    }

    unsafe extern "C" fn auras(out: *mut mq_emu_sys::game::mqemu_aura, cap: usize) -> usize {
        with(|game| {
            for (i, aura) in game.auras.iter().take(cap).enumerate() {
                unsafe { *out.add(i) = *aura };
            }
            game.auras.len()
        })
    }

    unsafe extern "C" fn buff_timer(spell_id: i32, song: bool) -> i32 {
        with(|game| game.buff_timers.get(&(spell_id, song)).copied().unwrap_or(-1))
    }

    /// The range set for the pair, or MacroQuest's least.
    unsafe extern "C" fn melee_range(attacker: *mut PlayerClient, target: *mut PlayerClient) -> f32 {
        with(|game| game.melee_ranges.get(&(attacker as usize, target as usize)).copied().unwrap_or(14.0))
    }

    unsafe extern "C" fn task_status(task: *mut CTaskEntry) -> *mut PCTaskStatus {
        with(|game| game.task_statuses.get(&(task as usize)).copied().unwrap_or(std::ptr::null_mut()))
    }

    unsafe extern "C" fn task_objective_text(objective: *const CTaskElement, out: *mut c_char, cap: usize) -> bool {
        with(|game| write(game.objective_texts.get(&(objective as usize)).map(String::as_str), out, cap))
    }

    unsafe extern "C" fn fast_time() -> u32 {
        with(|game| game.fast_time)
    }

    unsafe extern "C" fn friend_count() -> i32 {
        with(|game| game.friends.as_ref().map_or(-1, |friends| friends.len() as i32))
    }

    unsafe extern "C" fn friend_name(index: i32, out: *mut c_char, cap: usize) -> bool {
        with(|game| {
            let name = game.friends.as_ref().and_then(|friends| friends.get(usize::try_from(index).ok()?));
            write(name.map(String::as_str), out, cap)
        })
    }

    unsafe extern "C" fn active_favor_cost() -> i32 {
        with(|game| game.active_favor_cost)
    }

    unsafe extern "C" fn achievement_manager() -> *mut AchievementManager {
        with(|game| game.achievement_manager)
    }

    /// Links as "achievement <id> of <player>".
    unsafe extern "C" fn achievement_link(achievement: *const Achievement, player: *const c_char, out: *mut c_char, cap: usize) -> bool {
        let Some(achievement) = (unsafe { achievement.as_ref() }) else { return false };
        let player = unsafe { std::ffi::CStr::from_ptr(player) }.to_string_lossy();
        write(Some(&format!("achievement {} of {player}", achievement.id())), out, cap)
    }

    pub(crate) static API: Api = Api {
        mqemu_cached_buffs: cached_buffs,
        mqemu_alt_ability: alt_ability,
        mqemu_alt_ability_ready: alt_ability_ready,
        mqemu_db_string: db_string,
        mqemu_skill_value: skill_value,
        mqemu_skill_flags: skill_flags,
        mqemu_skill_timer: skill_timer,
        mqemu_skill_name: skill_name,
        mqemu_xtarget_role_name: xtarget_role_name,
        mqemu_max_endurance: max_endurance,
        mqemu_hp_current: hp_current,
        mqemu_max_hp: max_hp,
        mqemu_max_mana: max_mana,
        mqemu_class_name: class_name,
        mqemu_race_name: race_name,
        mqemu_clean_chat_line: clean_chat,
        mqemu_eq_command: eq_command,
        mqemu_memorize_spells: memorize_spells,
        mqemu_forget_gem: forget_gem,
        mqemu_gem_ready: gem_ready,
        mqemu_gem_count: gem_count,
        mqemu_cast_time_left_ms: cast_time_left_ms,
        mqemu_use_skill: use_skill,
        mqemu_do_combat_ability: do_combat_ability,
        mqemu_melee_range: melee_range,
        mqemu_buff_timer: buff_timer,
        mqemu_auras: auras,
        mqemu_skill_call_stack_shift: skill_call_stack_shift,
        mqemu_task_status: task_status,
        mqemu_task_objective_text: task_objective_text,
        mqemu_fast_time: fast_time,
        mqemu_friend_count: friend_count,
        mqemu_friend_name: friend_name,
        mqemu_active_favor_cost: active_favor_cost,
        mqemu_achievement_manager: achievement_manager,
        mqemu_achievement_link: achievement_link,
    };
}
