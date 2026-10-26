//! Your skills, and the abilities among them (the buttons on the abilities
//! page: sneak, taunt, kick...).

use std::fmt;
use std::time::Duration;

use mq_emu_sys::constants::NUM_SKILLS;
use mq_emu_sys::game::{Api, SKILL_ACTIVATABLE, SKILL_AVAILABLE, SKILL_HAS};

use crate::game_fns::text;
use crate::mq::Mq;
use crate::plugin::Plugin;

/// A skill's number, 0 to 99 (`${Me.Skill[n]}` is skill n-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SkillId(pub i32);

impl fmt::Display for SkillId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One of your skills, read when asked.
#[derive(Clone, Copy)]
pub struct Skill {
    id: SkillId,
    /// The profile's value; 0 if you haven't trained it.
    trained: i32,
    api: &'static Api,
}

impl Skill {
    pub fn id(&self) -> SkillId {
        self.id
    }

    /// The game's name for it ("1H Blunt", "Sneak", ...).
    pub fn name(&self) -> String {
        let api = self.api;
        let id = self.id.0;
        text(|out, cap| unsafe { (api.mqemu_skill_name)(id, out, cap) }).unwrap_or_default()
    }

    fn value_of(&self, which: i32) -> i32 {
        unsafe { (self.api.mqemu_skill_value)(self.id.0, which) }
    }

    /// With items, AAs and buffs (`${Me.Skill[n]}`); 0 if untrained.
    pub fn value(&self) -> i32 {
        if self.trained == 0 { 0 } else { self.value_of(0) }
    }

    /// As trained, without modifiers (`${Me.SkillBase[n]}`); 0 if untrained.
    pub fn base(&self) -> i32 {
        if self.trained == 0 { 0 } else { self.value_of(1) }
    }

    /// The most you can train it to at your level (`${Me.SkillCap[n]}`).
    pub fn cap(&self) -> i32 {
        self.value_of(2)
    }

    fn flags(&self) -> u32 {
        unsafe { (self.api.mqemu_skill_flags)(self.id.0) }
    }

    /// Your class has it (at your level), trained or not.
    pub fn is_known(&self) -> bool {
        self.flags() & SKILL_HAS != 0
    }

    /// An ability you activate (a button on the abilities page).
    pub fn is_ability(&self) -> bool {
        self.flags() & SKILL_ACTIVATABLE != 0
    }

    /// For an ability you have, whether you can use it now (`${Me.AbilityReady}`).
    pub fn is_ready(&self) -> bool {
        let flags = self.flags();
        flags & (SKILL_HAS | SKILL_ACTIVATABLE | SKILL_AVAILABLE) == SKILL_HAS | SKILL_ACTIVATABLE | SKILL_AVAILABLE
    }

    /// Time until you can use it again (`${Me.AbilityTimer}`), and its full
    /// reuse time (`${Me.AbilityTimerTotal}`).
    pub fn reuse(&self) -> (Duration, Duration) {
        let mut total = 0;
        let left = unsafe { (self.api.mqemu_skill_timer)(self.id.0, &mut total) };
        let ms = |value: i32| Duration::from_millis(value.max(0) as u64);
        (ms(left), ms(total))
    }
}

impl fmt::Debug for Skill {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Skill").field("id", &self.id).field("name", &self.name()).finish()
    }
}

impl<P: Plugin> Mq<P> {
    /// Skill `id`, known to your class or not; `None` out of game or for an
    /// ID past the last skill.
    pub fn skill(&self, id: SkillId) -> Option<Skill> {
        let index = usize::try_from(id.0).ok().filter(|i| *i < NUM_SKILLS)?;
        let trained = self.profile()?.Skill_at(index);
        Some(Skill { id, trained, api: self.host().game_fns })
    }

    /// Every skill your class has at your level, trained or not.
    pub fn skills(&self) -> impl Iterator<Item = Skill> + '_ {
        (0..NUM_SKILLS as i32).filter_map(|id| self.skill(SkillId(id))).filter(Skill::is_known)
    }

    /// The skill with this name, ignoring case.
    pub fn skill_by_name(&self, name: &str) -> Option<Skill> {
        (0..NUM_SKILLS as i32).filter_map(|id| self.skill(SkillId(id))).find(|skill| skill.name().eq_ignore_ascii_case(name))
    }

    /// Your abilities: skills you have that you activate.
    pub fn abilities(&self) -> impl Iterator<Item = Skill> + '_ {
        self.skills().filter(Skill::is_ability)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_fns::fake::{self, FakeSkill};

    fn skill(id: i32, trained: i32) -> Skill {
        Skill { id: SkillId(id), trained, api: &fake::API }
    }

    #[test]
    fn untrained_skills_read_zero_but_keep_their_cap() {
        fake::with(|game| game.skills.insert(40, FakeSkill { name: "Sneak".into(), values: [55, 50, 200], flags: SKILL_HAS | SKILL_ACTIVATABLE, timer: (0, 0) }));
        let sneak = skill(40, 50);
        assert_eq!((sneak.name().as_str(), sneak.value(), sneak.base(), sneak.cap()), ("Sneak", 55, 50, 200));
        let untrained = skill(40, 0);
        assert_eq!((untrained.value(), untrained.base(), untrained.cap()), (0, 0, 200), "as ${{Me.Skill}} reads it");
    }

    #[test]
    fn ready_needs_the_skill_an_ability_and_the_timer() {
        let all = SKILL_HAS | SKILL_ACTIVATABLE | SKILL_AVAILABLE;
        for (flags, ready) in [(all, true), (all & !SKILL_HAS, false), (all & !SKILL_ACTIVATABLE, false), (all & !SKILL_AVAILABLE, false)] {
            fake::with(|game| game.skills.insert(30, FakeSkill { flags, timer: (2_500, 10_000), ..FakeSkill::default() }));
            assert_eq!(skill(30, 1).is_ready(), ready, "{flags:#b}");
        }
        assert_eq!(skill(30, 1).reuse(), (Duration::from_millis(2_500), Duration::from_secs(10)));
        assert!(skill(31, 1).name().is_empty(), "no name");
    }
}
