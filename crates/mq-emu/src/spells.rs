//! Spells, your buffs and songs, spell gems and spell book.
//!
//! The client's spell table is indexed by [`SpellId`]. A [`Spell`] borrows
//! from the callback's [`Mq`] like every game object; keep its ID.

use std::fmt;

use mq_emu_sys::constants::{MAX_EFFECT_SLOTS, MAX_MEMORIZED_SPELLS, NUM_BOOK_SLOTS, NUM_LONG_BUFFS, NUM_TEMP_BUFFS, TOTAL_SPELL_COUNT};
use mq_emu_sys::layouts::{ClientSpellManager, EQ_Affect, EQ_Spell, PcProfile};

use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::from_c_array;

/// `SPA_NOSPELL`: an unused effect slot.
const SPA_NOSPELL: i32 = 254;
/// `EQ_Spell::ClassLevel` values for classes that can't use a spell
/// (MacroQuest's `IsSpellClassUsable`).
const CANT_USE: [u8; 3] = [255, 254, 127];
/// Player classes are 1 (warrior) to 16 (berserker).
const PLAYER_CLASSES: std::ops::RangeInclusive<u8> = 1..=16;

/// A spell's ID, stable across sessions (it's the spell's row in spells_us.txt).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpellId(pub i32);

impl fmt::Display for SpellId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One of a spell's effects: what it changes (`attrib`, a `SPA_*` number from
/// eqlib's `Spells.h`) and by how much.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpellEffect {
    /// Effect slot, 0-based (`${Spell.Attrib[n]}` is slot n-1).
    pub slot: usize,
    pub attrib: i32,
    pub base: i32,
    pub base2: i32,
    pub max: i32,
    /// How the value scales with level (spell "formula").
    pub calc: i32,
}

/// A spell for the duration of the current callback.
#[derive(Clone, Copy)]
pub struct Spell<'a> {
    raw: &'a EQ_Spell,
}

impl<'a> Spell<'a> {
    /// # Safety
    /// `raw` must be null or point to a live spell for `'a`, which must not
    /// extend past the current callback.
    pub unsafe fn from_raw(raw: *const EQ_Spell) -> Option<Spell<'a>> {
        unsafe { raw.as_ref() }.map(|raw| Spell { raw })
    }

    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a EQ_Spell {
        self.raw
    }

    pub fn id(&self) -> SpellId {
        SpellId(self.raw.ID())
    }

    pub fn name(&self) -> String {
        from_c_array(&self.raw.Name())
    }

    /// True if the name is `name`, ignoring ASCII case. Doesn't allocate.
    pub fn is_named(&self, name: &str) -> bool {
        let raw = self.raw.Name();
        let len = raw.iter().position(|c| *c == 0).unwrap_or(raw.len());
        len == name.len() && raw[..len].iter().zip(name.bytes()).all(|(a, b)| (*a as u8).eq_ignore_ascii_case(&b))
    }

    /// The level `class` (1 = warrior ... 16 = berserker) can use it at, or
    /// `None` if it can't.
    pub fn level_for(&self, class: u8) -> Option<u8> {
        let level = self.raw.ClassLevel().get(usize::from(class)).copied()?;
        (PLAYER_CLASSES.contains(&class) && !CANT_USE.contains(&level)).then_some(level)
    }

    /// True if any player class can use it.
    pub fn is_player_usable(&self) -> bool {
        PLAYER_CLASSES.into_iter().any(|class| self.level_for(class).is_some())
    }

    pub fn mana_cost(&self) -> i32 {
        self.raw.ManaCost()
    }

    pub fn endurance_cost(&self) -> i32 {
        self.raw.EnduranceCost()
    }

    /// Cast time in milliseconds, before focus effects.
    pub fn cast_time_ms(&self) -> u32 {
        self.raw.CastTime()
    }

    /// How long until any spell can be cast after this one, in milliseconds.
    pub fn recovery_time_ms(&self) -> u32 {
        self.raw.RecoveryTime()
    }

    /// How long until this spell can be cast again, in milliseconds.
    pub fn recast_time_ms(&self) -> u32 {
        self.raw.RecastTime()
    }

    pub fn range(&self) -> f32 {
        self.raw.Range()
    }

    pub fn ae_range(&self) -> f32 {
        self.raw.AERange()
    }

    /// How the duration scales with level (`DurationType`, the "formula").
    pub fn duration_formula(&self) -> u32 {
        self.raw.DurationType()
    }

    /// Duration cap in ticks (6 seconds each).
    pub fn duration_cap(&self) -> u32 {
        self.raw.DurationCap()
    }

    /// Lands in the short buff (song) window.
    pub fn is_short_duration(&self) -> bool {
        self.raw.DurationWindow()
    }

    /// Beneficial (including group-only), as opposed to detrimental.
    pub fn is_beneficial(&self) -> bool {
        self.raw.SpellType().0 != 0
    }

    /// The raw target type (`eSpellTargetType`).
    pub fn target_type(&self) -> u8 {
        self.raw.TargetType()
    }

    /// The raw resist type (`eResistType`).
    pub fn resist_type(&self) -> u8 {
        self.raw.Resist()
    }

    /// The casting skill.
    pub fn skill(&self) -> u8 {
        self.raw.Skill()
    }

    pub fn icon(&self) -> i32 {
        self.raw.SpellIcon()
    }

    pub fn gem_icon(&self) -> i32 {
        self.raw.GemIcon()
    }

    pub fn category(&self) -> i32 {
        self.raw.Category()
    }

    pub fn subcategory(&self) -> i32 {
        self.raw.Subcategory()
    }

    /// Spells that are ranks of one another share a group.
    pub fn spell_group(&self) -> i32 {
        self.raw.SpellGroup()
    }

    pub fn rank(&self) -> i32 {
        self.raw.SpellRank()
    }

    /// The message you see when it lands on you.
    pub fn cast_on_you(&self) -> String {
        from_c_array(&self.raw.CastOnYou())
    }

    /// The message others see when it lands on someone ("... is surrounded by ...").
    pub fn cast_on_another(&self) -> String {
        from_c_array(&self.raw.CastOnAnother())
    }

    pub fn wear_off(&self) -> String {
        from_c_array(&self.raw.WearOff())
    }

    /// The used effect slots.
    pub fn effects(self) -> impl Iterator<Item = SpellEffect> + 'a {
        let raw = self.raw;
        (0..MAX_EFFECT_SLOTS).map(move |slot| SpellEffect {
            slot,
            attrib: raw.Attrib_at(slot),
            base: raw.Base_at(slot),
            base2: raw.Base2_at(slot),
            max: raw.Max_at(slot),
            calc: raw.Calc_at(slot),
        })
        .filter(|effect| effect.attrib != SPA_NOSPELL)
    }

    /// True if any effect changes `attrib` (`SPA_*`).
    pub fn has_effect(&self, attrib: i32) -> bool {
        self.effects().any(|effect| effect.attrib == attrib)
    }
}

impl fmt::Debug for Spell<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Spell").field("id", &self.id()).field("name", &self.name()).finish()
    }
}

/// A buff or song on you.
#[derive(Clone, Copy)]
pub struct Buff<'a> {
    raw: &'a EQ_Affect,
    slot: usize,
    spell: Option<Spell<'a>>,
}

impl<'a> Buff<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a EQ_Affect {
        self.raw
    }

    /// The buff window slot, 0-based: `${Me.Buff[n]}` is slot n-1, and
    /// `${Me.Song[n]}` is song slot n-1.
    pub fn slot(&self) -> usize {
        self.slot
    }

    pub fn spell_id(&self) -> SpellId {
        SpellId(self.raw.SpellID())
    }

    /// The spell, if the client's spell table has it.
    pub fn spell(&self) -> Option<Spell<'a>> {
        self.spell
    }

    /// The spell's name, or "" if the client doesn't know the spell.
    pub fn name(&self) -> String {
        self.spell.map(|spell| spell.name()).unwrap_or_default()
    }

    /// The caster's level.
    pub fn level(&self) -> u8 {
        self.raw.Level()
    }

    /// Ticks (6 seconds each) left as the server last sent them, or `None`
    /// for a buff that doesn't wear off. The buff window counts down
    /// between updates; this doesn't.
    pub fn ticks(&self) -> Option<u32> {
        u32::try_from(self.raw.Duration()).ok()
    }

    /// Hits left for buffs that wear off after a number of hits (runes, procs).
    pub fn hit_count(&self) -> i32 {
        self.raw.HitCount()
    }

    /// The caster's ID as the server sent it.
    pub fn caster_id(&self) -> i32 {
        self.raw.CasterID()
    }
}

impl fmt::Debug for Buff<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Buff").field("slot", &self.slot).field("spell_id", &self.spell_id()).field("name", &self.name()).finish()
    }
}

/// Looks spells up by ID in the client's spell table.
#[derive(Clone, Copy)]
pub(crate) struct SpellTable<'a> {
    raw: Option<&'a ClientSpellManager>,
}

impl<'a> SpellTable<'a> {
    /// # Safety
    /// `raw` must be null or the game's spell manager, alive for `'a`.
    pub(crate) unsafe fn new(raw: *const ClientSpellManager) -> SpellTable<'a> {
        SpellTable { raw: unsafe { raw.as_ref() } }
    }

    pub(crate) fn get(&self, id: SpellId) -> Option<Spell<'a>> {
        let index = usize::try_from(id.0).ok().filter(|i| (1..TOTAL_SPELL_COUNT).contains(i))?;
        // SAFETY: entries are null or spells loaded for the whole session.
        unsafe { Spell::from_raw(self.raw?.Spells_at(index)) }
    }

    pub(crate) fn all(&self) -> impl Iterator<Item = Spell<'a>> + 'a {
        let table = *self;
        (1..TOTAL_SPELL_COUNT).filter_map(move |id| table.get(SpellId(id as i32)))
    }
}

/// MacroQuest's `GetSpellFromMap` choice among same-named spells, for a
/// character of `(class, level)`.
fn prefer_spell<'a>(matches: &[Spell<'a>], me: Option<(i32, i32)>) -> Option<Spell<'a>> {
    let first = *matches.first()?;
    // Of the spells `usable` accepts, the first with a category, else the first.
    let pick = |usable: &dyn Fn(&Spell<'a>) -> bool| {
        let mut chosen: Option<Spell<'a>> = None;
        for spell in matches.iter().filter(|spell| usable(spell)) {
            match chosen {
                None => chosen = Some(*spell),
                Some(current) if current.category() == 0 && spell.category() != 0 => chosen = Some(*spell),
                Some(_) => {}
            }
        }
        chosen
    };
    if matches.len() == 1 {
        return Some(first);
    }
    let mine = me.and_then(|(class, level)| {
        let class = u8::try_from(class).ok().filter(|class| PLAYER_CLASSES.contains(class))?;
        pick(&|spell| i32::from(spell.raw.ClassLevel_at(usize::from(class))) <= level)
    });
    mine.or_else(|| pick(&|spell| spell.is_player_usable())).or(Some(first))
}

/// A buff slot's contents, if it holds a spell.
fn buff<'a>(raw: &'a EQ_Affect, slot: usize, spells: SpellTable<'a>) -> Option<Buff<'a>> {
    let id = SpellId(raw.SpellID());
    (id.0 > 0).then(|| Buff { raw, slot, spell: spells.get(id) })
}

impl<P: Plugin> Mq<P> {
    pub(crate) fn spell_table(&self) -> SpellTable<'_> {
        unsafe { SpellTable::new((self.host().spell_manager)()) }
    }

    /// The spell with this ID, if the client has it (`${Spell[id]}`).
    pub fn spell(&self, id: SpellId) -> Option<Spell<'_>> {
        self.spell_table().get(id)
    }

    /// The spell with this exact name, ignoring case, chosen as
    /// `${Spell[name]}` chooses among spells that share a name: one your
    /// class can cast at your level, else one any class can use, preferring
    /// spells with a category (the learnable ones), else the lowest ID.
    ///
    /// This walks the whole spell table, so prefer [`spell`](Mq::spell) and
    /// keep the ID.
    pub fn spell_by_name(&self, name: &str) -> Option<Spell<'_>> {
        let matches: Vec<Spell<'_>> = self.spell_table().all().filter(|spell| spell.is_named(name)).collect();
        let me = self.profile().map(|p| (p.Class(), p.Level()));
        prefer_spell(&matches, me)
    }

    /// Your buffs (long duration), in buff window order.
    pub fn buffs(&self) -> impl Iterator<Item = Buff<'_>> + '_ {
        self.buff_slots(0, NUM_LONG_BUFFS)
    }

    /// Your songs (short duration buffs), in song window order.
    pub fn songs(&self) -> impl Iterator<Item = Buff<'_>> + '_ {
        self.buff_slots(NUM_LONG_BUFFS, NUM_TEMP_BUFFS)
    }

    /// The first buff or song from this spell.
    pub fn buff(&self, id: SpellId) -> Option<Buff<'_>> {
        self.buffs().chain(self.songs()).find(|buff| buff.spell_id() == id)
    }

    fn buff_slots(&self, first: usize, count: usize) -> impl Iterator<Item = Buff<'_>> + '_ {
        let profile = self.profile();
        let spells = self.spell_table();
        (0..count).filter_map(move |slot| buff(profile?.Buffs(first + slot), slot, spells))
    }

    /// The spell memorized in gem `gem` (0-based: `${Me.Gem[n]}` is gem n-1).
    pub fn gem(&self, gem: usize) -> Option<Spell<'_>> {
        let profile = self.profile().filter(|_| gem < MAX_MEMORIZED_SPELLS)?;
        self.spell(SpellId(profile.MemorizedSpells_at(gem)))
    }

    /// Your memorized spells, with their gems (0-based).
    pub fn gems(&self) -> impl Iterator<Item = (usize, Spell<'_>)> + '_ {
        (0..MAX_MEMORIZED_SPELLS).filter_map(|gem| self.gem(gem).map(|spell| (gem, spell)))
    }

    /// Your spell book, with each spell's book slot (0-based; 8 per page in
    /// RoF2, so slot 9 is the second spell on page 2).
    pub fn spell_book(&self) -> impl Iterator<Item = (usize, Spell<'_>)> + '_ {
        let profile = self.profile();
        let spells = self.spell_table();
        (0..NUM_BOOK_SLOTS).filter_map(move |slot| spells.get(SpellId(profile?.SpellBook_at(slot))).map(|spell| (slot, spell)))
    }

    /// True if this spell is in your spell book.
    pub fn knows_spell(&self, id: SpellId) -> bool {
        self.profile().is_some_and(|profile| (0..NUM_BOOK_SLOTS).any(|slot| profile.SpellBook_at(slot) == id.0))
    }

    /// The level your class can cast this spell at, if it can.
    pub fn spell_level(&self, spell: &Spell<'_>) -> Option<u8> {
        let profile: &PcProfile = self.profile()?;
        spell.level_for(u8::try_from(profile.Class()).ok()?)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::items::tests::{Fake, set_name};

    pub(crate) fn fake_spell(id: i32, name: &str) -> Fake<EQ_Spell> {
        let spell = Fake::<EQ_Spell>::new();
        spell.get().set_ID(id);
        spell.get().set_Name(set_name(name));
        for slot in 0..MAX_EFFECT_SLOTS {
            spell.get().set_Attrib_at(slot, SPA_NOSPELL);
        }
        spell
    }

    #[test]
    fn spells_read_their_fields_and_used_effects() {
        let fake = fake_spell(1200, "Courage");
        let raw = fake.get();
        raw.set_ManaCost(10);
        raw.set_SpellType(mq_emu_sys::layouts::eSpellType(1));
        raw.set_Attrib_at(0, 1); // SPA_AC
        raw.set_Base_at(0, 4);
        raw.set_Attrib_at(2, 0); // SPA_HP
        raw.set_Base_at(2, 10);
        raw.set_ClassLevel_at(2, 1); // cleric
        raw.set_ClassLevel_at(1, 255); // warrior
        let spell = unsafe { Spell::from_raw(fake.ptr()) }.unwrap();
        assert_eq!((spell.id(), spell.name().as_str(), spell.mana_cost()), (SpellId(1200), "Courage", 10));
        assert!(spell.is_named("COURAGE") && !spell.is_named("Courag") && !spell.is_named("Courage2") && !spell.is_named(""));
        assert!(spell.is_beneficial());
        let effects: Vec<(usize, i32, i32)> = spell.effects().map(|e| (e.slot, e.attrib, e.base)).collect();
        assert_eq!(effects, [(0, 1, 4), (2, 0, 10)]);
        assert!(spell.has_effect(0) && !spell.has_effect(SPA_NOSPELL));
        assert_eq!((spell.level_for(2), spell.level_for(1), spell.level_for(0), spell.level_for(200)), (Some(1), None, None, None));
    }

    #[test]
    fn same_named_spells_are_chosen_like_macroquest() {
        let fakes: Vec<Fake<EQ_Spell>> = (1..=4).map(|id| fake_spell(id, "Gate")).collect();
        for fake in &fakes {
            for class in 0..36 {
                fake.get().set_ClassLevel_at(class, 255);
            }
        }
        let spells: Vec<Spell<'_>> = fakes.iter().map(|f| unsafe { Spell::from_raw(f.ptr()) }.unwrap()).collect();
        let ids = |me| prefer_spell(&spells, me).map(|s| s.id().0);
        // Nobody can use any: the first.
        assert_eq!(ids(Some((2, 50))), Some(1));
        // Spell 3 is a wizard spell (class 12) at 4; spell 4 too, with a category.
        fakes[2].get().set_ClassLevel_at(12, 4);
        fakes[3].get().set_ClassLevel_at(12, 4);
        fakes[3].get().set_Category(5);
        assert_eq!(ids(Some((12, 10))), Some(4), "usable by me, with a category");
        assert_eq!(ids(Some((12, 3))), Some(4), "too low for either: any class's, with a category");
        fakes[3].get().set_ClassLevel_at(12, 20);
        assert_eq!(ids(Some((12, 10))), Some(3), "only spell 3 is usable at my level");
        assert_eq!(ids(None), Some(4), "no character: any class's, with a category");
        assert_eq!(prefer_spell(&spells[..1], Some((12, 10))).map(|s| s.id().0), Some(1), "a single match is it");
        assert!(prefer_spell(&[], None).is_none());
    }

    #[test]
    fn the_spell_table_rejects_ids_out_of_range() {
        let table = Fake::<ClientSpellManager>::new();
        let courage = fake_spell(1200, "Courage");
        table.get().set_Spells_at(1200, courage.ptr());
        let spells = unsafe { SpellTable::new(table.ptr()) };
        assert_eq!(spells.get(SpellId(1200)).map(|s| s.name()), Some("Courage".to_owned()));
        for id in [0, -1, 1201, TOTAL_SPELL_COUNT as i32, i32::MAX] {
            assert!(spells.get(SpellId(id)).is_none(), "{id}");
        }
        assert_eq!(spells.all().count(), 1);
        assert!(unsafe { SpellTable::new(std::ptr::null()) }.get(SpellId(1200)).is_none());
    }

    #[test]
    fn buffs_skip_empty_slots_and_know_permanent_ones() {
        let table = Fake::<ClientSpellManager>::new();
        let spells = unsafe { SpellTable::new(table.ptr()) };
        let slot = Fake::<EQ_Affect>::new();
        assert!(buff(slot.get(), 0, spells).is_none());
        slot.get().set_SpellID(-1);
        assert!(buff(slot.get(), 0, spells).is_none());
        slot.get().set_SpellID(1200);
        slot.get().set_Duration(-1);
        let unknown = buff(slot.get(), 3, spells).unwrap();
        assert_eq!((unknown.slot(), unknown.name().as_str(), unknown.ticks()), (3, "", None));
        slot.get().set_Duration(12);
        assert_eq!(buff(slot.get(), 3, spells).unwrap().ticks(), Some(12));
    }
}
