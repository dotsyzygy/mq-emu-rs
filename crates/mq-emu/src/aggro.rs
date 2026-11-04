//! The aggro meter (your hate on your target, the player second on its list,
//! the aggro lock) and your target's target.

use mq_emu_sys::constants::{AD_PLAYER, AD_SECONDARY};
use mq_emu_sys::layouts::AggroMeterManagerClient;

use crate::game::{Spawn, SpawnId};
use crate::mq::Mq;
use crate::plugin::Plugin;

impl<P: Plugin> Mq<P> {
    fn aggro_meter(&self) -> Option<&AggroMeterManagerClient> {
        unsafe { (self.host().aggro)().as_ref() }
    }

    fn spawn_or_none(&self, id: u32) -> Option<Spawn<'_>> {
        if id == 0 { None } else { self.spawn(SpawnId(id)) }
    }

    /// Your share of your target's hate, 0 to 100 (`${Me.PctAggro}`), if
    /// there's an aggro meter.
    pub fn aggro_percent(&self) -> Option<u16> {
        Some(self.aggro_meter()?.aggroData(AD_PLAYER).AggroPct())
    }

    /// The share of your target's hate of the player second on its list
    /// (`${Me.SecondaryPctAggro}`): the one most hated after you, or after
    /// the top if that isn't you.
    pub fn secondary_aggro_percent(&self) -> Option<u16> {
        Some(self.aggro_meter()?.aggroData(AD_SECONDARY).AggroPct())
    }

    /// The player [`secondary_aggro_percent`](Mq::secondary_aggro_percent)
    /// is for (`${Me.SecondaryAggroPlayer}`).
    pub fn secondary_aggro_player(&self) -> Option<Spawn<'_>> {
        self.spawn_or_none(self.aggro_meter()?.AggroSecondaryID())
    }

    /// The spawn your aggro meter is locked on (`${Me.AggroLock}`).
    pub fn aggro_lock(&self) -> Option<Spawn<'_>> {
        self.spawn_or_none(self.aggro_meter()?.AggroLockID())
    }

    /// Your target's target (`${Me.TargetOfTarget}`), as the server last
    /// sent it. Not confirmed on this client: in game it has only been seen
    /// empty, as MacroQuest's was.
    pub fn target_of_target(&self) -> Option<Spawn<'_>> {
        self.spawn_or_none(self.me()?.raw().TargetOfTarget())
    }
}
