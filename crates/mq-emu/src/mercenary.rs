//! Your mercenary.

use std::fmt;

use mq_emu_sys::constants::DB_STRING_MERCENARY_STANCE_NAME;
use mq_emu_sys::game::Api;
use mq_emu_sys::layouts::{CMercenaryManager, MercenaryInfo, MercenaryStanceInfo};

use crate::game::{Spawn, SpawnId};
use crate::game_fns::{game_array, text};
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::from_c_array;

/// `eMercenaryState::MercenaryState_Dead`.
const MERCENARY_DEAD: i32 = 0;
/// `eMercenaryState::MercenaryState_Active`.
const MERCENARY_ACTIVE: i32 = 5;

/// What your mercenary is doing, as `${Mercenary.State}` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MercenaryState {
    Active,
    Suspended,
    Dead,
}

/// One of your mercenary's stances.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MercenaryStance {
    /// What the game identifies the stance by.
    pub id: i32,
    /// "Passive", "Balanced", ..., or "" if the game has no name for it.
    pub name: String,
}

/// Your mercenary, for the duration of the current callback: the one you
/// have out, or the suspended one you'd unsuspend.
#[derive(Clone, Copy)]
pub struct Mercenary<'a> {
    raw: &'a CMercenaryManager,
    api: &'static Api,
}

impl<'a> Mercenary<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a CMercenaryManager {
        self.raw
    }

    pub fn state(&self) -> MercenaryState {
        let current = self.raw.currentMercenary();
        if !current.hasMercenary() {
            return MercenaryState::Suspended;
        }
        match current.suspendedState().0 {
            MERCENARY_DEAD => MercenaryState::Dead,
            MERCENARY_ACTIVE => MercenaryState::Active,
            _ => MercenaryState::Suspended,
        }
    }

    /// Its name, out or not.
    pub fn name(&self) -> String {
        from_c_array(&self.raw.currentMercenary().mercenaryInfo().name())
    }

    /// Its spawn's ID while it's out.
    pub fn spawn_id(&self) -> Option<SpawnId> {
        let id = self.raw.mercenarySpawnId();
        (id != 0).then_some(SpawnId(id))
    }

    /// Which of your mercenaries it is, 0-based (`${Mercenary.Index}` counts from 1).
    pub fn index(&self) -> Option<usize> {
        usize::try_from(self.raw.currMercenaryIndex()).ok()
    }

    /// Its current stance (`${Mercenary.Stance}`), if it's among the stances
    /// the game lists for it.
    pub fn stance(&self) -> Option<MercenaryStance> {
        let current = self.raw.currentMercenary().mercenaryInfo().stanceId();
        self.stance_infos().find(|info| info.stanceId() == current).map(|info| self.stance_of(info))
    }

    /// The stances it can take.
    pub fn stances(&self) -> Vec<MercenaryStance> {
        self.stance_infos().map(|info| self.stance_of(info)).collect()
    }

    fn stance_infos(&self) -> impl Iterator<Item = &'a MercenaryStanceInfo> + 'a {
        let list = self.raw.mercenaryStanceList();
        // SAFETY: the list holds m_length stance pointers, each null or live.
        unsafe { game_array(list.m_array(), list.m_length()) }.iter().filter_map(|info| unsafe { info.as_ref() })
    }

    fn stance_of(&self, info: &MercenaryStanceInfo) -> MercenaryStance {
        let api = self.api;
        let name = text(|out, cap| unsafe { (api.mqemu_db_string)(info.stanceStringId(), DB_STRING_MERCENARY_STANCE_NAME, out, cap) });
        MercenaryStance { id: info.stanceId(), name: name.unwrap_or_default() }
    }

    /// Every mercenary you own, this one included.
    pub fn owned(&self) -> impl Iterator<Item = OwnedMercenary<'a>> + 'a {
        let list = self.raw.mercenaries();
        // SAFETY: the list holds m_length mercenaries.
        unsafe { game_array(list.m_array(), list.m_length()) }.iter().map(|raw| OwnedMercenary { raw })
    }
}

impl fmt::Debug for Mercenary<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mercenary").field("name", &self.name()).field("state", &self.state()).finish()
    }
}

/// A mercenary you own.
#[derive(Clone, Copy)]
pub struct OwnedMercenary<'a> {
    raw: &'a MercenaryInfo,
}

impl<'a> OwnedMercenary<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a MercenaryInfo {
        self.raw
    }

    pub fn name(&self) -> String {
        from_c_array(&self.raw.name())
    }

    /// What the game identifies this mercenary by.
    pub fn id(&self) -> i32 {
        self.raw.id()
    }
}

impl fmt::Debug for OwnedMercenary<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OwnedMercenary").field("id", &self.id()).field("name", &self.name()).finish()
    }
}

impl<P: Plugin> Mq<P> {
    /// Your mercenary, out or suspended, if you have one (`${Mercenary}` isn't "NONE").
    pub fn mercenary(&self) -> Option<Mercenary<'_>> {
        let raw = unsafe { (self.host().mercenary_manager)().as_ref() }?;
        // Without one out, a mercenary index means one is suspended.
        (raw.currentMercenary().hasMercenary() || raw.currMercenaryIndex() != -1).then_some(Mercenary { raw, api: self.host().game_fns })
    }

    /// Your mercenary's spawn, while it's out in this zone.
    pub fn mercenary_spawn(&self) -> Option<Spawn<'_>> {
        self.spawn(self.mercenary()?.spawn_id()?)
    }
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::constants::DB_STRING_MERCENARY_STANCE_NAME;
    use mq_emu_sys::layouts::eMercenaryState;

    use super::*;
    use crate::game_fns::fake;
    use crate::items::tests::{Fake, set_name};

    fn mercenary(raw: &CMercenaryManager) -> Mercenary<'_> {
        Mercenary { raw, api: &fake::API }
    }

    #[test]
    fn states_are_named_as_macroquest_names_them() {
        let fake = Fake::<CMercenaryManager>::new();
        let raw = fake.get();
        let current = raw.currentMercenary();
        assert_eq!(mercenary(raw).state(), MercenaryState::Suspended, "none out");
        current.set_hasMercenary(true);
        for (value, state) in [(0, MercenaryState::Dead), (1, MercenaryState::Suspended), (5, MercenaryState::Active), (3, MercenaryState::Suspended)] {
            current.set_suspendedState(eMercenaryState(value));
            assert_eq!(mercenary(raw).state(), state, "{value}");
        }
    }

    #[test]
    fn stances_are_named_from_the_string_table() {
        let fake = Fake::<CMercenaryManager>::new();
        let raw = fake.get();
        let (passive, balanced) = (Fake::<MercenaryStanceInfo>::new(), Fake::<MercenaryStanceInfo>::new());
        passive.get().set_stanceId(1);
        passive.get().set_stanceStringId(100);
        balanced.get().set_stanceId(2);
        balanced.get().set_stanceStringId(101);
        let mut list = [passive.ptr(), std::ptr::null_mut(), balanced.ptr()];
        raw.mercenaryStanceList().set_m_array(list.as_mut_ptr());
        raw.mercenaryStanceList().set_m_length(3);
        raw.currentMercenary().mercenaryInfo().set_stanceId(2);
        raw.currentMercenary().mercenaryInfo().set_name(set_name("Fred"));
        fake::with(|game| game.strings.insert((101, DB_STRING_MERCENARY_STANCE_NAME), "Balanced".into()));

        let merc = mercenary(raw);
        assert_eq!(merc.name(), "Fred");
        assert_eq!(merc.stance(), Some(MercenaryStance { id: 2, name: "Balanced".into() }));
        let names: Vec<(i32, String)> = merc.stances().into_iter().map(|s| (s.id, s.name)).collect();
        assert_eq!(names, [(1, String::new()), (2, "Balanced".into())], "a null entry is skipped; a missing string reads as empty");
        raw.currentMercenary().mercenaryInfo().set_stanceId(7);
        assert_eq!(merc.stance(), None, "a stance the list doesn't have");
    }

    #[test]
    fn spawn_and_index_are_none_when_unset() {
        let fake = Fake::<CMercenaryManager>::new();
        let raw = fake.get();
        raw.set_currMercenaryIndex(-1);
        assert_eq!((mercenary(raw).spawn_id(), mercenary(raw).index()), (None, None));
        raw.set_mercenarySpawnId(42);
        raw.set_currMercenaryIndex(1);
        assert_eq!((mercenary(raw).spawn_id(), mercenary(raw).index()), (Some(SpawnId(42)), Some(1)));
    }
}
