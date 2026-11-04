//! Your auras: the aura window's list (`${Me.Aura}`).

use mq_emu_sys::game::mqemu_aura;

use crate::game::SpawnId;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::from_c_array;

/// More than the game lets anyone have.
const MAX_AURAS: usize = 16;

/// One of your auras, as the aura window lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aura {
    /// The aura window's name for it. The game keeps no spell for it, and
    /// the name may not be the ability's that made it: it's often the next
    /// spell's (the ability's ID + 1).
    pub name: String,
    /// The aura's own spawn, if it has one.
    pub spawn_id: Option<SpawnId>,
    pub cost: i32,
    pub icon_id: i32,
}

impl<P: Plugin> Mq<P> {
    /// Your auras, in the aura window's order (`${Me.Aura[n]}`).
    pub fn auras(&self) -> Vec<Aura> {
        if self.local_pc().is_none() {
            return Vec::new();
        }
        let mut raw = [mqemu_aura { name: [0; 64], spawn_id: 0, cost: 0, icon_id: 0 }; MAX_AURAS];
        let count = unsafe { (self.host().game_fns.mqemu_auras)(raw.as_mut_ptr(), MAX_AURAS) }.min(MAX_AURAS);
        raw[..count]
            .iter()
            .map(|aura| Aura { name: from_c_array(&aura.name), spawn_id: (aura.spawn_id != 0).then_some(SpawnId(aura.spawn_id)), cost: aura.cost, icon_id: aura.icon_id })
            .collect()
    }
}
