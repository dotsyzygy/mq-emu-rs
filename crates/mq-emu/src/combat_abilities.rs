//! Your combat abilities (disciplines): the ones you know, their reuse
//! timers, using one, and the one running now.

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mq_emu_sys::layouts::PcProfile;

use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::spells::{Spell, SpellId, SpellTable};

/// Slots in the profile's list of combat abilities (`NUM_COMBAT_ABILITIES`).
const SLOTS: usize = 300;
/// Reuse timers the profile keeps (`NUM_COMBAT_ABILITY_TIMERS`).
const TIMERS: usize = 20;

/// A combat ability you know, read when asked.
#[derive(Clone, Copy)]
pub struct CombatAbility<'a> {
    slot: usize,
    spell: Spell<'a>,
    /// When its reuse timer runs out, in seconds since 1970; 0 for none.
    expires: u32,
}

impl<'a> CombatAbility<'a> {
    /// Its slot in your list (0-based; `${Me.CombatAbility[n]}` is slot n-1).
    pub fn slot(&self) -> usize {
        self.slot
    }

    pub fn spell(&self) -> Spell<'a> {
        self.spell
    }

    pub fn id(&self) -> SpellId {
        self.spell.id()
    }

    /// Its name, with any rank ("Ashenhand Discipline Rk. II").
    pub fn name(&self) -> String {
        self.spell.name()
    }

    /// Time until you can use it again (`${Me.CombatAbilityTimer}`, which
    /// counts in 6-second ticks). Abilities sharing a timer share this.
    pub fn reuse(&self) -> Duration {
        self.reuse_at(now())
    }

    /// Whether its reuse timer has run out (`${Me.CombatAbilityReady}`).
    /// Like MacroQuest, this checks only the timer: the game may still
    /// refuse it for endurance, a running discipline or your stance.
    pub fn is_ready(&self) -> bool {
        self.expires < now()
    }

    fn reuse_at(&self, now: u32) -> Duration {
        Duration::from_secs(u64::from(self.expires.saturating_sub(now)))
    }
}

impl fmt::Debug for CombatAbility<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CombatAbility").field("slot", &self.slot).field("id", &self.id()).field("name", &self.name()).finish()
    }
}

/// The wall clock in seconds since 1970, as the game keeps the timers.
fn now() -> u32 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as u32)
}

/// The combat ability in `slot`, unless the slot is empty, its spell is
/// unknown, or a higher rank of it is in another slot (the game's ability
/// window hides those: `CCombatSkillsSelectWnd::ShouldDisplayThisSkill`).
fn ability<'a>(profile: &PcProfile, spells: SpellTable<'a>, slot: usize) -> Option<CombatAbility<'a>> {
    let spell_in = |slot: usize| Some(profile.CombatAbilities_at(slot)).filter(|id| *id > 0).and_then(|id| spells.get(SpellId(id)));
    let spell = spell_in(slot)?;
    let outranked = (0..SLOTS)
        .filter(|other| *other != slot)
        .filter_map(spell_in)
        .any(|other| other.spell_group() == spell.spell_group() && other.rank() > spell.rank());
    if outranked {
        return None;
    }
    let timer = usize::try_from(spell.raw().ReuseTimerIndex()).ok().filter(|index| *index < TIMERS);
    let expires = timer.map_or(0, |index| profile.CombatAbilityTimers_at(index));
    Some(CombatAbility { slot, spell, expires })
}

impl<P: Plugin> Mq<P> {
    /// Your combat abilities, in slot order, as the game's ability window
    /// lists them (only the highest rank you have of each).
    pub fn combat_abilities(&self) -> impl Iterator<Item = CombatAbility<'_>> + '_ {
        let profile = self.profile();
        let spells = self.spell_table();
        (0..SLOTS).filter_map(move |slot| ability(profile?, spells, slot))
    }

    /// The combat ability in slot `slot` (0-based), if it's one
    /// [`combat_abilities`](Mq::combat_abilities) lists.
    pub fn combat_ability(&self, slot: usize) -> Option<CombatAbility<'_>> {
        (slot < SLOTS).then(|| ability(self.profile()?, self.spell_table(), slot)).flatten()
    }

    /// Your combat ability with this exact name, ignoring case
    /// (`${Me.CombatAbility[name]}`). The name includes any rank, as
    /// [`CombatAbility::name`] shows it.
    pub fn combat_ability_by_name(&self, name: &str) -> Option<CombatAbility<'_>> {
        self.combat_abilities().find(|ability| ability.name().eq_ignore_ascii_case(name))
    }

    /// Uses a combat ability as `/doability` does. True when the game
    /// accepted it; the game says why in chat when it doesn't.
    pub fn use_combat_ability(&self, ability: &CombatAbility<'_>) -> bool {
        self.local_pc().is_some() && unsafe { (self.host().game_fns.mqemu_do_combat_ability)(ability.id().0) }
    }

    /// The discipline running now (`${Me.ActiveDisc}`), from the name the
    /// combat abilities window shows for it.
    pub fn active_disc(&self) -> Option<Spell<'_>> {
        let name = self.window("CombatAbilityWnd/CAW_CombatEffectLabel")?.text();
        if name.is_empty() { None } else { self.spell_by_name(&name) }
    }
}
