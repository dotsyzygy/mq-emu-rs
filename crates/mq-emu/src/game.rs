//! Game state: spawns, positions and the overall game state.

use std::fmt;

use mq_emu_sys::constants as c;
use mq_emu_sys::layouts::PlayerClient;

use crate::text::from_c_array;

/// Where the client is: character select, in game, zoning, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GameState {
    PreCharacterSelect,
    CharacterSelect,
    CharacterCreate,
    PostCharacterSelect,
    /// `GAMESTATE_SOMETHING`: a transitional state.
    Transitional,
    InGame,
    LoggingIn,
    Unloading,
    /// A value MacroQuest doesn't name.
    Unknown(i32),
}

impl From<i32> for GameState {
    fn from(value: i32) -> Self {
        match value {
            c::GAMESTATE_PRECHARSELECT => GameState::PreCharacterSelect,
            c::GAMESTATE_CHARSELECT => GameState::CharacterSelect,
            c::GAMESTATE_CHARCREATE => GameState::CharacterCreate,
            c::GAMESTATE_POSTCHARSELECT => GameState::PostCharacterSelect,
            c::GAMESTATE_SOMETHING => GameState::Transitional,
            c::GAMESTATE_INGAME => GameState::InGame,
            c::GAMESTATE_LOGGINGIN => GameState::LoggingIn,
            c::GAMESTATE_UNLOADING => GameState::Unloading,
            other => GameState::Unknown(other),
        }
    }
}

/// A spawn's ID: stable for as long as the spawn exists, so it's what to keep
/// across pulses (a [`Spawn`] can't outlive the callback it came from).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpawnId(pub u32);

impl fmt::Display for SpawnId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// A point in the world. Note that EverQuest's `/loc` prints Y before X.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Position {
    pub fn distance(&self, other: &Position) -> f32 {
        let (dx, dy, dz) = (self.x - other.x, self.y - other.y, self.z - other.z);
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Distance ignoring height, as most range checks in the game do.
    pub fn distance_2d(&self, other: &Position) -> f32 {
        let (dx, dy) = (self.x - other.x, self.y - other.y);
        (dx * dx + dy * dy).sqrt()
    }
}

/// A direction in the world.
///
/// EverQuest stores headings as 0..512 "units" for a full turn, increasing
/// counter-clockwise seen from above (0 = north, 128 = west). MacroQuest shows
/// them two ways, and so does this type:
/// - [`degrees`](Heading::degrees): clockwise compass degrees (0 = north,
///   90 = east), like `${Me.Heading.Degrees}` and `${Target.HeadingTo}`;
/// - [`degrees_ccw`](Heading::degrees_ccw): counter-clockwise, like
///   `${Me.Heading.DegreesCCW}`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Heading {
    /// Normalized to 0..512.
    units: f32,
}

impl Heading {
    /// Units in a full turn.
    pub const FULL_TURN: f32 = 512.0;
    pub const NORTH: Heading = Heading { units: 0.0 };
    pub const WEST: Heading = Heading { units: 128.0 };
    pub const SOUTH: Heading = Heading { units: 256.0 };
    pub const EAST: Heading = Heading { units: 384.0 };

    /// From EverQuest units; any value, wrapped into 0..512.
    pub fn from_units(units: f32) -> Heading {
        let units = units.rem_euclid(Self::FULL_TURN);
        // rem_euclid can round up to FULL_TURN itself for tiny negative inputs.
        Heading { units: if units >= Self::FULL_TURN { 0.0 } else { units } }
    }

    /// From clockwise compass degrees (0 = north, 90 = east).
    pub fn from_degrees(degrees: f32) -> Heading {
        Heading::from_degrees_ccw(-degrees)
    }

    /// From counter-clockwise degrees (0 = north, 90 = west).
    pub fn from_degrees_ccw(degrees: f32) -> Heading {
        Heading::from_units(degrees * Self::FULL_TURN / 360.0)
    }

    /// EverQuest units, 0..512.
    pub fn units(self) -> f32 {
        self.units
    }

    /// Clockwise compass degrees, 0..360.
    pub fn degrees(self) -> f32 {
        (360.0 - self.degrees_ccw()).rem_euclid(360.0)
    }

    /// Counter-clockwise degrees, 0..360.
    pub fn degrees_ccw(self) -> f32 {
        self.units * 360.0 / Self::FULL_TURN
    }

    /// This heading turned left (counter-clockwise) by `degrees`.
    pub fn turned_left(self, degrees: f32) -> Heading {
        Heading::from_degrees_ccw(self.degrees_ccw() + degrees)
    }

    /// This heading turned right (clockwise) by `degrees`.
    pub fn turned_right(self, degrees: f32) -> Heading {
        self.turned_left(-degrees)
    }

    /// How far apart two headings are, in degrees, 0..=180.
    pub fn difference(self, other: Heading) -> f32 {
        angle_difference(self.degrees_ccw(), other.degrees_ccw())
    }

    /// The heading from `from` towards `to`, as MacroQuest's `/face` and
    /// `${Spawn.HeadingTo}` compute it. North if the points coincide.
    pub fn between(from: &Position, to: &Position) -> Heading {
        let radians = (to.x - from.x).atan2(to.y - from.y);
        Heading::from_units(radians * Self::FULL_TURN / std::f32::consts::TAU)
    }
}

/// The smallest angle between two directions in degrees, 0..=180. Directions
/// that differ by whole turns (0 and 360, -90 and 270) are 0 apart.
pub fn angle_difference(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// What kind of spawn this is (`PlayerClient::Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpawnType {
    Player,
    Npc,
    Corpse,
    Unknown(u8),
}

impl From<u8> for SpawnType {
    fn from(value: u8) -> Self {
        match value {
            0 => SpawnType::Player,
            1 => SpawnType::Npc,
            2 => SpawnType::Corpse,
            other => SpawnType::Unknown(other),
        }
    }
}

/// A spawn (you, another player, an NPC, a corpse) for the duration of the
/// current callback. Store its [`SpawnId`] to find it again later.
#[derive(Clone, Copy)]
pub struct Spawn<'a> {
    raw: &'a PlayerClient,
}

impl<'a> Spawn<'a> {
    /// # Safety
    /// `raw` must point to a live spawn for the lifetime `'a`, which must not
    /// extend past the current callback.
    pub unsafe fn from_raw(raw: *mut PlayerClient) -> Option<Spawn<'a>> {
        unsafe { raw.as_ref() }.map(|raw| Spawn { raw })
    }

    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a PlayerClient {
        self.raw
    }

    pub fn id(&self) -> SpawnId {
        SpawnId(self.raw.SpawnID())
    }

    /// The name shown in game, e.g. "a rat" or "Soandso".
    pub fn name(&self) -> String {
        from_c_array(&self.raw.DisplayedName())
    }

    /// The internal name, e.g. "a_rat00".
    pub fn raw_name(&self) -> String {
        from_c_array(&self.raw.Name())
    }

    pub fn level(&self) -> u8 {
        self.raw.Level()
    }

    pub fn spawn_type(&self) -> SpawnType {
        SpawnType::from(self.raw.Type())
    }

    pub fn position(&self) -> Position {
        Position { x: self.raw.X(), y: self.raw.Y(), z: self.raw.Z() }
    }

    /// The direction the spawn faces.
    pub fn heading(&self) -> Heading {
        Heading::from_units(self.raw.Heading())
    }

    /// The direction from this spawn towards `other`.
    pub fn heading_to(&self, other: &Spawn<'_>) -> Heading {
        Heading::between(&self.position(), &other.position())
    }

    pub fn hp_current(&self) -> i32 {
        self.raw.HPCurrent()
    }

    pub fn hp_max(&self) -> i32 {
        self.raw.HPMax()
    }

    /// HP as 0..=100. For NPCs the client often only knows the percentage.
    pub fn hp_percent(&self) -> i32 {
        let (current, max) = (self.raw.HPCurrent(), self.raw.HPMax());
        if max <= 0 { 0 } else { (i64::from(current) * 100 / i64::from(max)).clamp(0, 100) as i32 }
    }

    pub fn mana_current(&self) -> i32 {
        self.raw.ManaCurrent()
    }

    pub fn mana_max(&self) -> i32 {
        self.raw.ManaMax()
    }

    pub fn distance_to(&self, other: &Spawn<'_>) -> f32 {
        self.position().distance(&other.position())
    }
}

impl fmt::Debug for Spawn<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Spawn").field("id", &self.id()).field("name", &self.name()).field("level", &self.level()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_states_map_from_mq_values() {
        assert_eq!(GameState::from(c::GAMESTATE_INGAME), GameState::InGame);
        assert_eq!(GameState::from(-1), GameState::PreCharacterSelect);
        assert_eq!(GameState::from(42), GameState::Unknown(42));
    }

    /// MacroQuest's formulas, transcribed from MQ2BasicTypes.cpp and MQ2SpawnType.cpp.
    mod macroquest {
        /// `${Spawn.Heading.Degrees}`: `360 - fmod(units * 0.703125, 360)`.
        pub fn heading_degrees(units: f32) -> f32 {
            let heading = 360.0 - (units * 0.703125) % 360.0;
            if heading < 0.0 { heading + 360.0 } else { heading }
        }

        /// `${Spawn.Heading.DegreesCCW}`.
        pub fn heading_degrees_ccw(units: f32) -> f32 {
            units * 0.703125
        }

        /// The raw value behind `${Spawn.HeadingTo}`, before `.Degrees` flips it.
        pub fn heading_to_ccw(me: (f32, f32), target: (f32, f32)) -> f32 {
            let mut d = (me.1 - target.1).atan2(target.0 - me.0) * 180.0 / std::f32::consts::PI + 90.0;
            if d < 0.0 {
                d += 360.0
            } else if d >= 360.0 {
                d -= 360.0
            }
            d
        }
    }

    fn close(a: f32, b: f32) -> bool {
        angle_difference(a, b) < 1e-3
    }

    #[test]
    fn heading_degrees_match_macroquest() {
        // The bug found in game: Degrees is clockwise, so at every heading but
        // north and south it differs from the raw counter-clockwise value.
        for units in [0.0, 1.0, 100.0, 128.0, 255.9, 256.0, 300.0, 384.0, 511.0] {
            let heading = Heading::from_units(units);
            assert!(close(heading.degrees(), macroquest::heading_degrees(units)), "Degrees at {units}");
            assert!(close(heading.degrees_ccw(), macroquest::heading_degrees_ccw(units)), "DegreesCCW at {units}");
        }
    }

    #[test]
    fn heading_between_matches_macroquest_heading_to() {
        let me = Position { x: 10.0, y: -20.0, z: 0.0 };
        for (x, y) in [(10.0, 0.0), (30.0, -20.0), (10.0, -40.0), (-5.0, -20.0), (17.0, 3.0), (-40.0, -90.0)] {
            let target = Position { x, y, z: 5.0 };
            let ours = Heading::between(&me, &target);
            let theirs_ccw = macroquest::heading_to_ccw((me.x, me.y), (x, y));
            assert!(close(ours.degrees_ccw(), theirs_ccw), "to ({x}, {y}): {} vs {theirs_ccw}", ours.degrees_ccw());
            // `${...HeadingTo.Degrees}` is the clockwise form of the same value.
            assert!(close(ours.degrees(), macroquest::heading_degrees(theirs_ccw / 0.703125)));
        }
    }

    #[test]
    fn compass_directions() {
        let origin = Position::default();
        let at = |x, y| Heading::between(&origin, &Position { x, y, z: 0.0 });
        // EverQuest's +Y is north and +X is west.
        assert_eq!(at(0.0, 10.0), Heading::NORTH);
        assert_eq!(at(10.0, 0.0), Heading::WEST);
        assert_eq!(at(0.0, -10.0), Heading::SOUTH);
        assert_eq!(at(-10.0, 0.0), Heading::EAST);
        assert_eq!(Heading::EAST.degrees(), 90.0);
        assert_eq!(Heading::WEST.degrees(), 270.0);
        assert_eq!(Heading::WEST.degrees_ccw(), 90.0);
    }

    #[test]
    fn heading_conversions_round_trip_and_wrap() {
        assert_eq!(Heading::from_degrees(90.0), Heading::EAST);
        assert_eq!(Heading::from_degrees_ccw(90.0), Heading::WEST);
        assert_eq!(Heading::from_units(512.0), Heading::NORTH);
        assert_eq!(Heading::from_units(-128.0), Heading::EAST);
        assert!(Heading::from_units(-1e-6).units() < Heading::FULL_TURN);
        assert_eq!(Heading::NORTH.turned_left(90.0), Heading::WEST);
        assert_eq!(Heading::NORTH.turned_right(90.0), Heading::EAST);
        assert_eq!(Heading::WEST.difference(Heading::EAST), 180.0);
        assert_eq!(Heading::from_degrees(350.0).difference(Heading::from_degrees(10.0)), 20.0);
    }

    #[test]
    fn angle_difference_wraps_around() {
        // The in-game selftest failure: heading 512 is 360° to MacroQuest, 0° to us.
        assert_eq!(angle_difference(0.0, 360.0), 0.0);
        assert_eq!(angle_difference(350.0, 10.0), 20.0);
        assert_eq!(angle_difference(10.0, 350.0), 20.0);
        assert_eq!(angle_difference(90.0, 270.0), 180.0);
        assert_eq!(angle_difference(-90.0, 270.0), 0.0);
        assert_eq!(angle_difference(45.0, 45.0), 0.0);
        assert_eq!(angle_difference(0.0, 181.0), 179.0);
    }

    #[test]
    fn distances() {
        let a = Position { x: 0.0, y: 0.0, z: 0.0 };
        let b = Position { x: 3.0, y: 4.0, z: 12.0 };
        assert_eq!(a.distance(&b), 13.0);
        assert_eq!(a.distance_2d(&b), 5.0);
    }

    /// A zeroed PlayerClient-sized buffer, for exercising Spawn's accessors.
    fn with_fake_spawn(f: impl FnOnce(&Spawn<'_>, &PlayerClient)) {
        let mut storage = vec![0u64; size_of::<PlayerClient>().div_ceil(8)];
        let raw = storage.as_mut_ptr().cast::<PlayerClient>();
        let spawn = unsafe { Spawn::from_raw(raw) }.unwrap();
        f(&spawn, unsafe { &*raw });
    }

    #[test]
    fn spawn_reads_through_generated_accessors() {
        with_fake_spawn(|spawn, raw| {
            raw.set_SpawnID(77);
            raw.set_Level(65);
            raw.set_Type(1);
            raw.set_X(10.0);
            raw.set_Y(-20.0);
            raw.set_Z(3.5);
            raw.set_Heading(128.0);
            let mut name = raw.DisplayedName();
            for (d, s) in name.iter_mut().zip(b"a rat\0") {
                *d = *s as std::ffi::c_char;
            }
            raw.set_DisplayedName(name);

            assert_eq!(spawn.id(), SpawnId(77));
            assert_eq!(spawn.level(), 65);
            assert_eq!(spawn.spawn_type(), SpawnType::Npc);
            assert_eq!(spawn.position(), Position { x: 10.0, y: -20.0, z: 3.5 });
            assert_eq!(spawn.heading(), Heading::WEST);
            assert_eq!(spawn.name(), "a rat");
        });
    }

    #[test]
    fn hp_percent_handles_zero_and_overflowing_values() {
        with_fake_spawn(|spawn, raw| {
            raw.set_HPMax(0);
            assert_eq!(spawn.hp_percent(), 0);
            raw.set_HPMax(200);
            raw.set_HPCurrent(50);
            assert_eq!(spawn.hp_percent(), 25);
            raw.set_HPCurrent(500);
            assert_eq!(spawn.hp_percent(), 100);
            raw.set_HPMax(i32::MAX);
            raw.set_HPCurrent(i32::MAX);
            assert_eq!(spawn.hp_percent(), 100);
        });
    }

    #[test]
    fn null_spawn_is_none() {
        assert!(unsafe { Spawn::from_raw(std::ptr::null_mut()) }.is_none());
    }
}
