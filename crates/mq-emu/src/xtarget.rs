//! The extended target window: its slots, what each is set to track, and
//! your aggro on what's in them.

use std::fmt;

use mq_emu_sys::constants::{AD_XTARGET1, MAX_AGGRO_METER_SIZE};
use mq_emu_sys::game::Api;
use mq_emu_sys::layouts::{AggroMeterManagerClient, ExtendedTargetSlot};

use crate::game::{Spawn, SpawnId};
use crate::game_fns::{game_array, text};
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::from_c_array;

/// `XTargetTypes::XTARGET_AUTO_HATER`: a slot that fills itself with whatever
/// is hating you.
pub const XTARGET_AUTO_HATER: u32 = 1;

/// Whether a slot holds anything (`eXTSlotStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XTargetStatus {
    Empty,
    /// Holds a spawn in your zone.
    InZone,
    /// Holds someone in another zone.
    OtherZone,
    /// A value this API doesn't know.
    Other(i32),
}

impl From<i32> for XTargetStatus {
    fn from(value: i32) -> XTargetStatus {
        match value {
            0 => XTargetStatus::Empty,
            1 => XTargetStatus::InZone,
            2 => XTargetStatus::OtherZone,
            other => XTargetStatus::Other(other),
        }
    }
}

/// One extended target slot, for the duration of the current callback.
#[derive(Clone, Copy)]
pub struct XTarget<'a> {
    raw: &'a ExtendedTargetSlot,
    slot: usize,
    aggro: Option<&'a AggroMeterManagerClient>,
    api: &'static Api,
}

impl<'a> XTarget<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a ExtendedTargetSlot {
        self.raw
    }

    /// The window's slot, 0-based: `${Me.XTarget[n]}` is slot n-1.
    pub fn slot(&self) -> usize {
        self.slot
    }

    /// What the slot is set to track (`XTargetTypes`: 0 nothing,
    /// [`XTARGET_AUTO_HATER`], specific players and NPCs, group roles...).
    pub fn role(&self) -> u32 {
        self.raw.xTargetType()
    }

    /// The window's name for the role ("Auto Hater", "Group Tank", ...;
    /// `${Me.XTarget[n].TargetType}`).
    pub fn role_name(&self) -> String {
        let api = self.api;
        let role = self.role();
        text(|out, cap| unsafe { (api.mqemu_xtarget_role_name)(role, out, cap) }).unwrap_or_default()
    }

    pub fn status(&self) -> XTargetStatus {
        XTargetStatus::from(self.raw.XTargetSlotStatus().0)
    }

    /// Something is in the slot (`${Me.XTarget}` counts these).
    pub fn is_set(&self) -> bool {
        self.role() != 0 && self.status() != XTargetStatus::Empty
    }

    /// The spawn in the slot, if it's in your zone.
    pub fn spawn_id(&self) -> Option<SpawnId> {
        let id = self.raw.SpawnID();
        (id != 0).then_some(SpawnId(id))
    }

    /// The name the slot shows.
    pub fn name(&self) -> String {
        from_c_array(&self.raw.Name())
    }

    /// Your share of its hate, 0 to 100 (`${Me.XTarget[n].PctAggro}`), if the
    /// aggro meter covers this slot.
    pub fn aggro_percent(&self) -> Option<u16> {
        let index = AD_XTARGET1 + self.slot;
        (index < MAX_AGGRO_METER_SIZE).then(|| self.aggro.map(|aggro| aggro.aggroData(index).AggroPct())).flatten()
    }
}

impl fmt::Debug for XTarget<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XTarget").field("slot", &self.slot).field("role", &self.role()).field("name", &self.name()).finish()
    }
}

impl<P: Plugin> Mq<P> {
    /// Every slot of the extended target window, set or not
    /// (`${Me.XTargetSlots}` of them).
    pub fn xtargets(&self) -> impl Iterator<Item = XTarget<'_>> + '_ {
        let list = self.local_pc().and_then(|pc| unsafe { pc.pExtendedTargetList().as_ref() });
        // SAFETY: the list holds m_length slots.
        let slots = list.map_or(&[][..], |list| unsafe { game_array(list.m_targetSlots().m_array(), list.m_targetSlots().m_length()) });
        let aggro = unsafe { (self.host().aggro)().as_ref() };
        let api = self.host().game_fns;
        slots.iter().enumerate().map(move |(slot, raw)| XTarget { raw, slot, aggro, api })
    }

    /// Extended target slot `slot` (0-based), set or not.
    pub fn xtarget(&self, slot: usize) -> Option<XTarget<'_>> {
        self.xtargets().nth(slot)
    }

    /// The spawns hating you that the extended target window knows of: its
    /// auto hater slots' spawns, in your zone.
    pub fn haters(&self) -> impl Iterator<Item = Spawn<'_>> + '_ {
        self.xtargets().filter(|x| x.role() == XTARGET_AUTO_HATER).filter_map(|x| self.spawn(x.spawn_id()?))
    }
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::layouts::eXTSlotStatus;

    use super::*;
    use crate::game_fns::fake;
    use crate::items::tests::{Fake, set_name};

    #[test]
    fn slots_read_role_status_name_and_aggro() {
        let slots: Vec<Fake<ExtendedTargetSlot>> = (0..2).map(|_| Fake::new()).collect();
        let aggro = Fake::<AggroMeterManagerClient>::new();
        aggro.get().aggroData(AD_XTARGET1 + 1).set_AggroPct(85);
        let (empty, hater) = (slots[0].get(), slots[1].get());
        hater.set_xTargetType(XTARGET_AUTO_HATER);
        hater.set_XTargetSlotStatus(eXTSlotStatus(1));
        hater.set_SpawnID(77);
        hater.set_Name(set_name("a goblin"));
        fake::with(|game| game.xtarget_roles.insert(XTARGET_AUTO_HATER, "Auto Hater".into()));

        let xt = |raw, slot| XTarget { raw, slot, aggro: Some(aggro.get()), api: &fake::API };
        let (empty, hater) = (xt(empty, 0), xt(hater, 1));
        assert!(!empty.is_set());
        assert_eq!((empty.status(), empty.spawn_id(), empty.role_name().as_str()), (XTargetStatus::Empty, None, ""));
        assert!(hater.is_set());
        assert_eq!((hater.status(), hater.spawn_id(), hater.name().as_str()), (XTargetStatus::InZone, Some(SpawnId(77)), "a goblin"));
        assert_eq!((hater.role_name().as_str(), hater.aggro_percent()), ("Auto Hater", Some(85)));
    }

    #[test]
    fn aggro_stops_where_the_meter_does() {
        let slot = Fake::<ExtendedTargetSlot>::new();
        let aggro = Fake::<AggroMeterManagerClient>::new();
        let xt = |slot_index, aggro| XTarget { raw: slot.get(), slot: slot_index, aggro, api: &fake::API };
        let last = MAX_AGGRO_METER_SIZE - AD_XTARGET1 - 1;
        assert_eq!(xt(last, Some(aggro.get())).aggro_percent(), Some(0));
        assert_eq!(xt(last + 1, Some(aggro.get())).aggro_percent(), None, "past the meter");
        assert_eq!(xt(0, None).aggro_percent(), None, "no meter");
        assert_eq!(XTargetStatus::from(9), XTargetStatus::Other(9));
    }
}
