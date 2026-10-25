//! The zone's spawns: every player, NPC, corpse, pet and mount the client knows.
//!
//! eqlib keeps them in a linked list (`PlayerManagerClient::PlayerList`, then
//! each spawn's `m_pNext`). Walking it is cheap for a zone's few hundred
//! spawns, and like every [`Spawn`], what it yields can't outlive the callback.

use std::marker::PhantomData;

use mq_emu_sys::layouts::{PlayerClient, PlayerManagerClient};

use crate::game::{Position, Spawn, SpawnId};
use crate::mq::Mq;
use crate::plugin::Plugin;

/// The most spawns [`Spawns`] will walk. Far above any real zone; it only
/// guarantees a damaged list (e.g. a cycle) can't hang the game.
pub const MAX_SPAWNS: usize = 10_000;

/// Iterates over the zone's spawns in the client's list order.
pub struct Spawns<'a> {
    next: *mut PlayerClient,
    walked: usize,
    _borrow: PhantomData<&'a ()>,
}

impl<'a> Spawns<'a> {
    /// # Safety
    /// `manager` must be null or the game's spawn manager, and the spawns must
    /// stay alive for `'a`, which must not extend past the current callback.
    pub(crate) unsafe fn new(manager: *mut PlayerManagerClient) -> Spawns<'a> {
        let first = unsafe { manager.as_ref() }.map_or(std::ptr::null_mut(), |m| m.PlayerList().m_pFirstNode());
        Spawns { next: first, walked: 0, _borrow: PhantomData }
    }
}

impl<'a> Iterator for Spawns<'a> {
    type Item = Spawn<'a>;

    fn next(&mut self) -> Option<Spawn<'a>> {
        if self.walked >= MAX_SPAWNS {
            return None;
        }
        // SAFETY: list nodes are live spawns for the current callback (see `new`).
        let spawn = unsafe { Spawn::from_raw(self.next) }?;
        self.next = spawn.raw().m_pNext();
        self.walked += 1;
        Some(spawn)
    }
}

impl<P: Plugin> Mq<P> {
    /// Every spawn in the zone, including you.
    pub fn spawns(&self) -> Spawns<'_> {
        unsafe { Spawns::new((self.host().spawn_manager)()) }
    }

    /// The spawn with this ID, if it's still in the zone.
    pub fn spawn(&self, id: SpawnId) -> Option<Spawn<'_>> {
        self.spawns().find(|spawn| spawn.id() == id)
    }

    /// The first spawn whose shown name (e.g. "a rat", "Soandso") matches,
    /// ignoring case.
    pub fn spawn_by_name(&self, name: &str) -> Option<Spawn<'_>> {
        self.spawns().find(|spawn| spawn.name().eq_ignore_ascii_case(name))
    }

    /// The spawn closest to you (the controlled spawn) that `filter` accepts,
    /// not counting yourself. Distance is 3D.
    pub fn nearest(&self, filter: impl Fn(&Spawn<'_>) -> bool) -> Option<Spawn<'_>> {
        let me = self.controlled()?;
        let (my_id, here) = (me.id(), me.position());
        self.nearest_to(&here, |spawn| spawn.id() != my_id && filter(spawn))
    }

    /// The spawn closest to `point` that `filter` accepts.
    pub fn nearest_to(&self, point: &Position, filter: impl Fn(&Spawn<'_>) -> bool) -> Option<Spawn<'_>> {
        self.spawns()
            .filter(|spawn| filter(spawn))
            .map(|spawn| (spawn.position().distance(point), spawn))
            .min_by(|(a, _), (b, _)| a.total_cmp(b))
            .map(|(_, spawn)| spawn)
    }
}
