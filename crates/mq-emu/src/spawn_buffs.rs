//! Buffs on others: your target's and your pet's as their windows show them,
//! and MacroQuest's cache of the buffs of every spawn you've targeted.
//!
//! The server only sends a spawn's buffs when you target it, a moment after
//! you do: until [`Mq::target_buffs_populated`] says they've arrived, an
//! empty list doesn't mean an unbuffed target.

use std::fmt;
use std::time::Duration;

use mq_emu_sys::constants::MAX_TOTAL_BUFFS;
use mq_emu_sys::game::mqemu_cached_buff;
use mq_emu_sys::layouts::{CXStr, HashTable_CXStr_int_ResizePolicyNoResize};

use crate::game::Spawn;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::spells::{Spell, SpellId, SpellTable};
use crate::text::{from_c_array, from_cxstr};

/// The most entries [`find_text`] walks in one hash bucket, against a
/// damaged table.
const MAX_CHAIN: usize = 1024;

/// Finds `key` in one of the game's `HashTable<CXStr>` (`FindFirst`).
fn find_text(table: &HashTable_CXStr_int_ResizePolicyNoResize, key: i32) -> Option<&CXStr> {
    let (buckets, size) = (table.m_table(), table.m_tableSize());
    if buckets.is_null() || size <= 0 {
        return None;
    }
    // The game hashes an integer key as itself, unsigned.
    let bucket = (key as u32 % size as u32) as usize;
    // SAFETY: the table has `size` buckets, each null or a chain of live nodes.
    let mut node = unsafe { *buckets.add(bucket) };
    for _ in 0..MAX_CHAIN {
        let entry = unsafe { node.as_ref() }?;
        if entry.m_key() == key {
            return Some(entry.m_value());
        }
        node = entry.next();
    }
    None
}

/// A buff on your target or pet, as its window shows it.
#[derive(Clone, Copy)]
pub struct WindowBuff<'a> {
    slot: usize,
    spell_id: SpellId,
    spell: Option<Spell<'a>>,
    remaining_ms: i32,
    caster: Option<&'a CXStr>,
}

impl<'a> WindowBuff<'a> {
    /// The window's slot, 0-based: `${Me.Pet.Buff[n]}` is slot n-1.
    pub fn slot(&self) -> usize {
        self.slot
    }

    pub fn spell_id(&self) -> SpellId {
        self.spell_id
    }

    /// The spell, if the client's spell table has it.
    pub fn spell(&self) -> Option<Spell<'a>> {
        self.spell
    }

    /// The spell's name, or "" if the client doesn't know the spell.
    pub fn name(&self) -> String {
        self.spell.map(|spell| spell.name()).unwrap_or_default()
    }

    /// Time left as the window counts it, or `None` for a buff that doesn't
    /// wear off.
    pub fn remaining(&self) -> Option<Duration> {
        u64::try_from(self.remaining_ms).ok().map(Duration::from_millis)
    }

    /// Who cast it, as the window knows it ("" if it doesn't).
    pub fn caster(&self) -> String {
        self.caster.map(from_cxstr).unwrap_or_default()
    }
}

impl fmt::Debug for WindowBuff<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WindowBuff").field("slot", &self.slot).field("spell_id", &self.spell_id).field("name", &self.name()).finish()
    }
}

/// The used slots of a buff window: spell IDs, timers (ms) and casters by
/// spell ID. A slot is used if its spell ID is above 0 (empty ones hold 0 or -1).
fn window_buffs<'a>(
    spell_id: impl Fn(usize) -> i32 + 'a,
    timer: impl Fn(usize) -> i32 + 'a,
    casters: &'a HashTable_CXStr_int_ResizePolicyNoResize,
    spells: SpellTable<'a>,
) -> impl Iterator<Item = WindowBuff<'a>> + 'a {
    (0..MAX_TOTAL_BUFFS).filter_map(move |slot| {
        let id = spell_id(slot);
        (id > 0).then(|| WindowBuff {
            slot,
            spell_id: SpellId(id),
            spell: spells.get(SpellId(id)),
            remaining_ms: timer(slot),
            caster: find_text(casters, id),
        })
    })
}

/// One of MacroQuest's cached buffs: what the server last sent for a spawn
/// you targeted (`${Spawn[...].Buff[n]}`).
#[derive(Clone, Copy)]
pub struct CachedBuff<'a> {
    raw: mqemu_cached_buff,
    spell: Option<Spell<'a>>,
}

impl<'a> CachedBuff<'a> {
    /// The buff slot on the spawn, 0-based (`${...Buff[n].Slot}` counts from 1).
    pub fn slot(&self) -> usize {
        self.raw.slot.max(0) as usize
    }

    pub fn spell_id(&self) -> SpellId {
        SpellId(self.raw.spell_id)
    }

    /// The spell, if the client's spell table has it.
    pub fn spell(&self) -> Option<Spell<'a>> {
        self.spell
    }

    /// The spell's name, or "" if the client doesn't know the spell.
    pub fn name(&self) -> String {
        self.spell.map(|spell| spell.name()).unwrap_or_default()
    }

    /// The caster's name.
    pub fn caster(&self) -> String {
        from_c_array(&self.raw.caster)
    }

    /// Time left now, counting down from when the server sent it, or `None`
    /// for a buff that doesn't wear off.
    pub fn remaining(&self) -> Option<Duration> {
        (self.raw.duration >= 0).then(|| Duration::from_millis(u64::from(self.raw.remaining_ms)))
    }

    /// Hits left, for buffs that wear off after a number of hits.
    pub fn count(&self) -> i32 {
        self.raw.count
    }

    /// How long ago the server sent it (`${...Buff[n].Staleness}`).
    pub fn age(&self) -> Duration {
        Duration::from_millis(u64::from(self.raw.age_ms))
    }
}

impl fmt::Debug for CachedBuff<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CachedBuff").field("slot", &self.slot()).field("spell_id", &self.spell_id()).field("name", &self.name()).finish()
    }
}

impl<P: Plugin> Mq<P> {
    /// Your target's buffs and debuffs as the target window shows them. Empty
    /// without a target, and until [`target_buffs_populated`](Mq::target_buffs_populated).
    pub fn target_buffs(&self) -> impl Iterator<Item = WindowBuff<'_>> + '_ {
        let window = self.target().and_then(|_| unsafe { (self.host().target_window)().as_ref() });
        window.into_iter().flat_map(|window| window_buffs(|i| window.BuffSpellID_at(i), |i| window.BuffTimer_at(i), window.Casters(), self.spell_table()))
    }

    /// Whether the server has sent your current target's buffs yet
    /// (`${Target.BuffsPopulated}`).
    pub fn target_buffs_populated(&self) -> bool {
        self.target().is_some() && (self.host().target_buffs_populated)()
    }

    /// Your pet's buffs as the pet window shows them; empty without a pet.
    pub fn pet_buffs(&self) -> impl Iterator<Item = WindowBuff<'_>> + '_ {
        let window = self.pet().and_then(|_| unsafe { (self.host().pet_window)().as_ref() });
        window.into_iter().flat_map(|window| {
            // The pet window keeps timers unsigned; a permanent buff's -1 reads as u32::MAX.
            window_buffs(|i| window.Buff_at(i), |i| window.PetBuffTimer_at(i) as i32, window.WhoCast(), self.spell_table())
        })
    }

    /// MacroQuest's cached buffs for `spawn`, in `${Spawn[id].Buff[n]}` order:
    /// what the server last sent when you targeted it. Empty for spawns you
    /// haven't targeted since they last changed zones. Expired buffs are left out.
    pub fn cached_buffs(&self, spawn: &Spawn<'_>) -> Vec<CachedBuff<'_>> {
        let api = self.host().game_fns;
        let raw = std::ptr::from_ref(spawn.raw()).cast_mut();
        let mut buffs: Vec<mqemu_cached_buff> = Vec::new();
        loop {
            let capacity = buffs.capacity();
            // SAFETY: `buffs` has room for `capacity` entries; the shim writes at most that many.
            let total = unsafe { (api.mqemu_cached_buffs)(raw, buffs.as_mut_ptr(), capacity) };
            if total <= capacity {
                unsafe { buffs.set_len(total) };
                break;
            }
            buffs.reserve_exact(total);
        }
        let spells = self.spell_table();
        buffs.into_iter().map(|raw| CachedBuff { raw, spell: spells.get(SpellId(raw.spell_id)) }).collect()
    }
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::layouts::{CTargetWnd, ClientSpellManager, HashTable_CXStr_int_ResizePolicyNoResize_HashNode as Node};

    use super::*;
    use crate::items::tests::Fake;
    use crate::spells::tests::fake_spell;
    use crate::text::tests::cstrrep;

    /// A caster table with `entries` in `size` buckets, chained as the game chains them.
    struct FakeTable {
        table: Fake<HashTable_CXStr_int_ResizePolicyNoResize>,
        _buckets: Vec<*mut Node>,
        _nodes: Vec<Fake<Node>>,
        _names: Vec<Vec<u64>>,
    }

    fn table(size: usize, entries: &[(i32, &str)]) -> FakeTable {
        let mut buckets = vec![std::ptr::null_mut::<Node>(); size];
        let mut nodes = Vec::new();
        let mut names = Vec::new();
        for (key, name) in entries {
            let node = Fake::<Node>::new();
            let text = cstrrep(0, name.len() as u32, name.as_bytes());
            node.get().m_value().set_m_data(text.as_ptr().cast_mut().cast());
            node.get().set_m_key(*key);
            let bucket = (*key as u32 % size as u32) as usize;
            node.get().set_next(buckets[bucket]);
            buckets[bucket] = node.ptr();
            nodes.push(node);
            names.push(text);
        }
        let table = Fake::<HashTable_CXStr_int_ResizePolicyNoResize>::new();
        table.get().set_m_table(buckets.as_mut_ptr());
        table.get().set_m_tableSize(size as i32);
        table.get().set_m_entryCount(entries.len() as i32);
        FakeTable { table, _buckets: buckets, _nodes: nodes, _names: names }
    }

    #[test]
    fn caster_names_are_found_through_chained_buckets() {
        // 5 and 12 share bucket 5 of 7; -3 hashes as 4294967293 % 7 = 2.
        let fake = table(7, &[(5, "Ann"), (12, "Bob"), (-3, "Cy")]);
        let found = |key| find_text(fake.table.get(), key).map(from_cxstr);
        assert_eq!((found(5), found(12), found(-3)), (Some("Ann".into()), Some("Bob".into()), Some("Cy".into())));
        assert_eq!(found(19), None, "same bucket, not there");
        assert_eq!(found(1), None, "empty bucket");
    }

    #[test]
    fn empty_or_missing_tables_find_nothing() {
        let empty = Fake::<HashTable_CXStr_int_ResizePolicyNoResize>::new();
        assert!(find_text(empty.get(), 5).is_none(), "no buckets");
        let fake = table(7, &[(5, "Ann")]);
        fake.table.get().set_m_tableSize(0);
        assert!(find_text(fake.table.get(), 5).is_none(), "size 0 would divide by zero");
        fake.table.get().set_m_tableSize(-7);
        assert!(find_text(fake.table.get(), 5).is_none(), "negative size");
    }

    #[test]
    fn a_looping_chain_ends() {
        let fake = table(7, &[(5, "Ann")]);
        let node = unsafe { *fake.table.get().m_table().add(5) };
        unsafe { (*node).set_next(node) };
        assert!(find_text(fake.table.get(), 12).is_none());
    }

    #[test]
    fn window_buffs_are_the_used_slots() {
        let window = Fake::<CTargetWnd>::new();
        let w = window.get();
        for slot in 0..MAX_TOTAL_BUFFS {
            w.set_BuffSpellID_at(slot, -1);
        }
        w.set_BuffSpellID_at(0, 1200);
        w.set_BuffTimer_at(0, 60_000);
        w.set_BuffSpellID_at(3, 0);
        w.set_BuffSpellID_at(MAX_TOTAL_BUFFS - 1, 2510);
        w.set_BuffTimer_at(MAX_TOTAL_BUFFS - 1, -1);
        let casters = table(10, &[(1200, "Soandso")]);
        let manager = Fake::<ClientSpellManager>::new();
        let courage = fake_spell(1200, "Courage");
        manager.get().set_Spells_at(1200, courage.ptr());
        let spells = unsafe { SpellTable::new(manager.ptr()) };

        let buffs: Vec<WindowBuff<'_>> = window_buffs(|i| w.BuffSpellID_at(i), |i| w.BuffTimer_at(i), casters.table.get(), spells).collect();
        assert_eq!(buffs.len(), 2);
        let (courage, last) = (buffs[0], buffs[1]);
        assert_eq!((courage.slot(), courage.name().as_str(), courage.caster().as_str()), (0, "Courage", "Soandso"));
        assert_eq!(courage.remaining(), Some(Duration::from_secs(60)));
        assert_eq!((last.slot(), last.spell_id(), last.name().as_str(), last.caster().as_str()), (MAX_TOTAL_BUFFS - 1, SpellId(2510), "", ""));
        assert_eq!(last.remaining(), None, "doesn't wear off");
    }

    #[test]
    fn the_windows_hold_every_buff_slot() {
        // Both windows' arrays are MAX_TOTAL_BUFFS long, as window_buffs assumes.
        for (record, field) in [("CTargetWnd", "BuffSpellID"), ("CTargetWnd", "BuffTimer"), ("CPetInfoWnd", "Buff"), ("CPetInfoWnd", "PetBuffTimer")] {
            let info = mq_emu_sys::layouts::FIELDS.iter().find(|f| f.record == record && f.field == field).unwrap();
            assert_eq!(info.size as usize, MAX_TOTAL_BUFFS * 4, "{record}::{field}");
        }
    }

    #[test]
    fn cached_buffs_read_their_fields() {
        let mut caster = [0 as std::ffi::c_char; 64];
        for (d, s) in caster.iter_mut().zip(b"Soandso") {
            *d = *s as std::ffi::c_char;
        }
        let raw = mqemu_cached_buff { slot: 4, spell_id: 1200, duration: 10, count: 3, remaining_ms: 59_000, age_ms: 1_000, caster };
        let buff = CachedBuff { raw, spell: None };
        assert_eq!((buff.slot(), buff.spell_id(), buff.caster().as_str(), buff.count()), (4, SpellId(1200), "Soandso", 3));
        assert_eq!((buff.remaining(), buff.age()), (Some(Duration::from_millis(59_000)), Duration::from_secs(1)));
        let permanent = CachedBuff { raw: mqemu_cached_buff { duration: -1, ..raw }, spell: None };
        assert_eq!(permanent.remaining(), None);
    }
}
