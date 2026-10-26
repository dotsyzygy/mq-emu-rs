//! Alternate abilities (AAs): the ones you own, and any the game knows.
//!
//! The game keeps one entry per rank of each ability. An ability's
//! [`group_id`](AltAbility::group_id) is the same for all its ranks (it's the
//! number `/alt activate` takes); its [`index`](AltAbility::index) names one rank.

use std::fmt;
use std::time::Duration;

use mq_emu_sys::constants::{
    AA_CHAR_MAX_REAL, DB_STRING_ALT_ABILITY_CATEGORY, DB_STRING_ALT_ABILITY_DESCRIPTION, DB_STRING_ALT_ABILITY_NAME,
    DB_STRING_EXPANSION_NAME, NUM_ALT_ABILITIES,
};
use mq_emu_sys::game::Api;
use mq_emu_sys::layouts::CAltAbilityData;

use crate::game_fns::text;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::spells::{Spell, SpellId, SpellTable};

/// One rank of an alternate ability, for the duration of the current callback.
#[derive(Clone, Copy)]
pub struct AltAbility<'a> {
    raw: &'a CAltAbilityData,
    api: &'static Api,
    spells: SpellTable<'a>,
}

impl<'a> AltAbility<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a CAltAbilityData {
        self.raw
    }

    /// This rank's index (`${AltAbility[...].Index}`).
    pub fn index(&self) -> i32 {
        self.raw.Index()
    }

    /// The ability's ID, shared by its ranks: what `/alt activate` takes and
    /// `${Me.AltAbility[n]}` looks up (`${...AltAbility.ID}`).
    pub fn group_id(&self) -> i32 {
        self.raw.GroupID()
    }

    fn string(&self, id: i32, kind: i32) -> String {
        let api = self.api;
        text(|out, cap| unsafe { (api.mqemu_db_string)(id, kind, out, cap) }).unwrap_or_default()
    }

    pub fn name(&self) -> String {
        self.string(self.raw.nName(), DB_STRING_ALT_ABILITY_NAME)
    }

    pub fn description(&self) -> String {
        self.string(self.raw.nDesc(), DB_STRING_ALT_ABILITY_DESCRIPTION)
    }

    /// The window's category for it, or its expansion's name when it has
    /// none (`${AltAbility[...].Category}`).
    pub fn category(&self) -> String {
        match self.raw.DisplayCategory() {
            -1 => self.string(self.raw.Expansion(), DB_STRING_EXPANSION_NAME),
            category => self.string(category, DB_STRING_ALT_ABILITY_CATEGORY),
        }
    }

    /// Which rank this is, from 1.
    pub fn rank(&self) -> i32 {
        self.raw.CurrentRank()
    }

    pub fn max_rank(&self) -> i32 {
        self.raw.MaxRank()
    }

    /// Points spent on this ability up to and including this rank
    /// (`${...PointsSpent}`).
    pub fn points_spent(&self) -> i32 {
        self.raw.TotalPoints()
    }

    /// What this rank costs.
    pub fn cost(&self) -> i32 {
        self.raw.Cost()
    }

    pub fn min_level(&self) -> i32 {
        self.raw.MinLevel()
    }

    /// The window tab: 1 general, 2 archetype, 3 class, 4 special, 5 focus.
    pub fn kind(&self) -> i32 {
        self.raw.Type()
    }

    /// The spell it casts when activated; `None` for passive abilities.
    pub fn spell_id(&self) -> Option<SpellId> {
        let id = self.raw.SpellID();
        (id > 0).then_some(SpellId(id))
    }

    pub fn spell(&self) -> Option<Spell<'a>> {
        self.spells.get(self.spell_id()?)
    }

    /// Always on rather than activated (`${...Passive}`).
    pub fn is_passive(&self) -> bool {
        self.raw.SpellID() == -1
    }

    /// Reuse time after activating, before any reductions.
    pub fn reuse_time(&self) -> Duration {
        Duration::from_secs(self.raw.ReuseTimer().max(0) as u64)
    }

    /// The next rank's index, if there is one.
    pub fn next_index(&self) -> Option<i32> {
        let next = self.raw.NextGroupAbilityId();
        (next > 0).then_some(next)
    }

    /// Whether you can activate it now (`${Me.AltAbilityReady}`), and how
    /// long until you can (`${Me.AltAbilityTimer}`, whole seconds). Passive
    /// abilities are never ready.
    pub fn readiness(&self) -> (bool, Duration) {
        let mut seconds = 0;
        let ready = unsafe { (self.api.mqemu_alt_ability_ready)(std::ptr::from_ref(self.raw).cast_mut(), &mut seconds) };
        (ready && !self.is_passive(), Duration::from_secs(seconds.max(0) as u64))
    }

    /// Whether you can activate it now.
    pub fn is_ready(&self) -> bool {
        self.readiness().0
    }
}

impl fmt::Debug for AltAbility<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AltAbility").field("group_id", &self.group_id()).field("index", &self.index()).field("name", &self.name()).finish()
    }
}

impl<P: Plugin> Mq<P> {
    fn alt_ability_at(&self, index: i32) -> Option<AltAbility<'_>> {
        let api = self.host().game_fns;
        let raw = unsafe { (api.mqemu_alt_ability)(index).as_ref() }?;
        Some(AltAbility { raw, api, spells: self.spell_table() })
    }

    /// The alternate abilities you own, each at the rank you have.
    pub fn alt_abilities(&self) -> impl Iterator<Item = AltAbility<'_>> + '_ {
        let profile = self.profile();
        (0..AA_CHAR_MAX_REAL).filter_map(move |slot| {
            let index = profile?.AAList(slot).AAIndex();
            if index > 0 { self.alt_ability_at(index) } else { None }
        })
    }

    /// The ability you own with this ID (`${Me.AltAbility[id]}`).
    pub fn alt_ability(&self, group_id: i32) -> Option<AltAbility<'_>> {
        self.alt_abilities().find(|ability| ability.group_id() == group_id)
    }

    /// The ability you own with this name, ignoring case (`${Me.AltAbility[name]}`).
    pub fn alt_ability_by_name(&self, name: &str) -> Option<AltAbility<'_>> {
        self.alt_abilities().find(|ability| ability.name().eq_ignore_ascii_case(name))
    }

    /// Any ability rank the game knows, owned or not, by its index.
    pub fn alt_ability_by_index(&self, index: i32) -> Option<AltAbility<'_>> {
        usize::try_from(index).ok().filter(|i| *i < NUM_ALT_ABILITIES)?;
        self.alt_ability_at(index)
    }

    /// Your unspent AA points (`${Me.AAPoints}`).
    pub fn alt_ability_points(&self) -> i32 {
        self.profile().map_or(0, |p| p.AAPoints())
    }

    /// The AA points you've spent (`${Me.AAPointsSpent}`).
    pub fn alt_ability_points_spent(&self) -> i32 {
        self.profile().map_or(0, |p| p.AAPointsSpent())
    }
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::layouts::ClientSpellManager;

    use super::*;
    use crate::game_fns::fake;
    use crate::items::tests::Fake;

    fn ability(raw: &CAltAbilityData) -> AltAbility<'_> {
        AltAbility { raw, api: &fake::API, spells: unsafe { SpellTable::new(std::ptr::null::<ClientSpellManager>()) } }
    }

    #[test]
    fn names_come_from_the_string_table() {
        let fake = Fake::<CAltAbilityData>::new();
        let raw = fake.get();
        raw.set_nName(10);
        raw.set_nDesc(11);
        raw.set_DisplayCategory(-1);
        raw.set_Expansion(3);
        fake::with(|game| {
            game.strings.insert((10, DB_STRING_ALT_ABILITY_NAME), "Mass Group Buff".into());
            game.strings.insert((11, DB_STRING_ALT_ABILITY_DESCRIPTION), "Buffs the group.".into());
            game.strings.insert((3, DB_STRING_EXPANSION_NAME), "The Planes of Power".into());
            game.strings.insert((7, DB_STRING_ALT_ABILITY_CATEGORY), "Utility".into());
        });
        let aa = ability(raw);
        assert_eq!((aa.name().as_str(), aa.description().as_str()), ("Mass Group Buff", "Buffs the group."));
        assert_eq!(aa.category(), "The Planes of Power", "no category: the expansion");
        raw.set_DisplayCategory(7);
        assert_eq!(aa.category(), "Utility");
        raw.set_nName(99);
        assert_eq!(aa.name(), "", "a string the table lacks");
    }

    #[test]
    fn passive_abilities_have_no_spell_and_are_never_ready() {
        let fake = Fake::<CAltAbilityData>::new();
        let raw = fake.get();
        raw.set_SpellID(-1);
        raw.set_NextGroupAbilityId(-1);
        fake::with(|game| game.alt_ability_ready.insert(fake.ptr() as usize, (true, 0)));
        let aa = ability(raw);
        assert!(aa.is_passive() && aa.spell_id().is_none() && aa.next_index().is_none());
        assert!(!aa.is_ready(), "the game says ready, but there's nothing to activate");

        raw.set_SpellID(2558);
        raw.set_NextGroupAbilityId(1001);
        assert_eq!((aa.is_passive(), aa.spell_id(), aa.next_index()), (false, Some(SpellId(2558)), Some(1001)));
        assert!(aa.is_ready());
        fake::with(|game| game.alt_ability_ready.insert(fake.ptr() as usize, (false, 75)));
        assert_eq!(aa.readiness(), (false, Duration::from_secs(75)));
    }
}
