//! Zones: the one you're in, and every zone the client knows.

use std::fmt;

use mq_emu_sys::constants::MAX_ZONES;
use mq_emu_sys::layouts::{EOutDoor, EQWorldData, EQZoneInfo, zoneHeader};

use crate::game::{Heading, Position};
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::from_c_array;

/// A zone's ID (`${Zone.ID}`), the same for every instance of the zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ZoneId(pub u16);

impl ZoneId {
    /// The base zone of a dynamic zone ID, which shares the low 15 bits with
    /// it (`EQWorldData::GetZoneBaseId`).
    pub fn from_raw(raw: u32) -> ZoneId {
        ZoneId((raw & 0x7fff) as u16)
    }
}

impl fmt::Display for ZoneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Whether a zone is indoors, outdoors and/or a city (`EOutDoor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZoneType {
    IndoorDungeon,
    Outdoor,
    OutdoorCity,
    DungeonCity,
    IndoorCity,
    OutdoorDungeon,
    Unknown(u8),
}

impl From<EOutDoor> for ZoneType {
    fn from(value: EOutDoor) -> ZoneType {
        match value {
            EOutDoor::IndoorDungeon => ZoneType::IndoorDungeon,
            EOutDoor::Outdoor => ZoneType::Outdoor,
            EOutDoor::OutdoorCity => ZoneType::OutdoorCity,
            EOutDoor::DungeonCity => ZoneType::DungeonCity,
            EOutDoor::IndoorCity => ZoneType::IndoorCity,
            EOutDoor::OutdoorDungeon => ZoneType::OutdoorDungeon,
            EOutDoor(other) => ZoneType::Unknown(other),
        }
    }
}

impl ZoneType {
    /// As `${Zone.Indoor}`.
    pub fn is_indoor(self) -> bool {
        matches!(self, ZoneType::IndoorDungeon | ZoneType::DungeonCity | ZoneType::IndoorCity)
    }

    /// As `${Zone.Outdoor}`.
    pub fn is_outdoor(self) -> bool {
        matches!(self, ZoneType::Outdoor | ZoneType::OutdoorCity | ZoneType::OutdoorDungeon)
    }

    /// Cities, where anyone can bind (MacroQuest's `${Zone.NoBind}` is the opposite).
    pub fn is_bindable(self) -> bool {
        matches!(self, ZoneType::OutdoorCity | ZoneType::DungeonCity | ZoneType::IndoorCity)
    }
}

/// The zone you're in, for the duration of the current callback.
#[derive(Clone, Copy)]
pub struct CurrentZone<'a> {
    raw: &'a zoneHeader,
    /// `PcClient::currentZoneId`: zone ID in the low 16 bits, instance above.
    current: Option<u32>,
}

impl<'a> CurrentZone<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a zoneHeader {
        self.raw
    }

    /// `${Zone.ID}`, from your character's current zone.
    pub fn id(&self) -> Option<ZoneId> {
        self.current.map(ZoneId::from_raw)
    }

    /// The instance number; 0 outside instances.
    pub fn instance(&self) -> u16 {
        self.current.map_or(0, |id| (id >> 16) as u16)
    }

    /// E.g. "qeynos2" (`${Zone.ShortName}`).
    pub fn short_name(&self) -> String {
        from_c_array(&self.raw.ShortName())
    }

    /// E.g. "North Qeynos" (`${Zone.Name}`).
    pub fn name(&self) -> String {
        from_c_array(&self.raw.LongName())
    }

    pub fn zone_type(&self) -> ZoneType {
        ZoneType::from(self.raw.ZoneType())
    }

    pub fn gravity(&self) -> f32 {
        self.raw.ZoneGravity()
    }

    /// Where you land if you zone in without a zone line, e.g. after death
    /// with no bind here.
    pub fn safe_point(&self) -> (Position, Heading) {
        let raw = self.raw;
        (Position { x: raw.SafeXLoc(), y: raw.SafeYLoc(), z: raw.SafeZLoc() }, Heading::from_units(raw.SafeHeading()))
    }

    /// The client's near clip plane distance.
    pub fn min_clip(&self) -> f32 {
        self.raw.MinClip()
    }

    /// The client's far clip plane distance: nothing beyond is drawn.
    pub fn max_clip(&self) -> f32 {
        self.raw.MaxClip()
    }

    /// The zone's entry in the client's zone list.
    pub fn info(&self, mq: &'a Mq<impl Plugin>) -> Option<ZoneInfo<'a>> {
        mq.zone_info(self.id()?)
    }
}

impl fmt::Debug for CurrentZone<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CurrentZone").field("id", &self.id()).field("short_name", &self.short_name()).finish()
    }
}

/// A zone the client knows (`EQWorldData::ZoneArray`), whether or not you're in it.
#[derive(Clone, Copy)]
pub struct ZoneInfo<'a> {
    raw: &'a EQZoneInfo,
}

impl<'a> ZoneInfo<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a EQZoneInfo {
        self.raw
    }

    pub fn id(&self) -> ZoneId {
        ZoneId::from_raw(self.raw.Id() as u32)
    }

    pub fn short_name(&self) -> String {
        from_c_array(&self.raw.ShortName())
    }

    pub fn name(&self) -> String {
        from_c_array(&self.raw.LongName())
    }

    /// The expansion that added it (`EQExpansion`: 0 classic, 1 Kunark, ...).
    pub fn expansion(&self) -> i32 {
        self.raw.EQExpansion().0
    }

    /// The level needed to enter, 0 if none.
    pub fn min_level(&self) -> i32 {
        self.raw.MinLevel()
    }

    /// `EQZoneFlag_*` bits (eqlib `game/EQClasses.h`).
    pub fn flags(&self) -> u64 {
        self.raw.ZoneFlags()
    }
}

impl fmt::Debug for ZoneInfo<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ZoneInfo").field("id", &self.id()).field("short_name", &self.short_name()).finish()
    }
}

/// Every zone in `world`, in ID order.
fn zones(world: Option<&EQWorldData>) -> impl Iterator<Item = ZoneInfo<'_>> {
    // SAFETY: entries are null or zones loaded for the whole session.
    (0..MAX_ZONES).filter_map(move |id| unsafe { world?.ZoneArray_at(id).as_ref() }).map(|raw| ZoneInfo { raw })
}

impl<P: Plugin> Mq<P> {
    fn world_data(&self) -> Option<&EQWorldData> {
        unsafe { (self.host().world_data)().as_ref() }
    }

    /// The zone you're in, if in game.
    pub fn zone(&self) -> Option<CurrentZone<'_>> {
        let raw = unsafe { (self.host().zone_info)().as_ref() }?;
        Some(CurrentZone { raw, current: self.local_pc().map(|pc| pc.currentZoneId() as u32) })
    }

    /// A zone by ID, from the client's zone list (`${Zone[id]}`).
    pub fn zone_info(&self, id: ZoneId) -> Option<ZoneInfo<'_>> {
        let index = usize::from(id.0);
        let raw = unsafe { self.world_data().filter(|_| index < MAX_ZONES)?.ZoneArray_at(index).as_ref() }?;
        Some(ZoneInfo { raw })
    }

    /// A zone by short name ("qeynos2") or long name ("North Qeynos"),
    /// ignoring case (`${Zone[name]}`).
    pub fn zone_by_name(&self, name: &str) -> Option<ZoneInfo<'_>> {
        zones(self.world_data())
            .find(|zone| zone.short_name().eq_ignore_ascii_case(name) || zone.name().eq_ignore_ascii_case(name))
    }

    /// Every zone the client knows, in ID order.
    pub fn zones(&self) -> impl Iterator<Item = ZoneInfo<'_>> + '_ {
        zones(self.world_data())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::tests::{Fake, set_name};

    #[test]
    fn zone_types_match_macroquest() {
        for (raw, indoor, outdoor, bindable) in
            [(0, true, false, false), (1, false, true, false), (2, false, true, true), (3, true, false, true), (4, true, false, true), (5, false, true, false)]
        {
            let kind = ZoneType::from(EOutDoor(raw));
            assert_eq!((kind.is_indoor(), kind.is_outdoor(), kind.is_bindable()), (indoor, outdoor, bindable), "{kind:?}");
        }
        assert_eq!(ZoneType::from(EOutDoor(9)), ZoneType::Unknown(9));
    }

    #[test]
    fn current_zone_splits_id_and_instance() {
        let header = Fake::<zoneHeader>::new();
        header.get().set_ShortName(set_name("qeynos2"));
        let zone = CurrentZone { raw: header.get(), current: Some(0x0003_0002) };
        assert_eq!((zone.id(), zone.instance(), zone.short_name().as_str()), (Some(ZoneId(2)), 3, "qeynos2"));
        // A dynamic zone ID keeps its base zone in the low 15 bits.
        assert_eq!(ZoneId::from_raw(0x8000 | 77), ZoneId(77));
        let unknown = CurrentZone { raw: header.get(), current: None };
        assert_eq!((unknown.id(), unknown.instance()), (None, 0));
    }

    #[test]
    fn the_zone_list_skips_empty_entries() {
        let world = Fake::<EQWorldData>::new();
        let qeynos = Fake::<EQZoneInfo>::new();
        qeynos.get().set_Id(2);
        qeynos.get().set_ShortName(set_name("qeynos2"));
        qeynos.get().set_LongName(set_name("North Qeynos"));
        world.get().set_ZoneArray_at(2, qeynos.ptr());
        let found: Vec<String> = zones(Some(world.get())).map(|zone| zone.name()).collect();
        assert_eq!(found, ["North Qeynos"]);
        assert_eq!(zones(None).count(), 0);
    }
}
