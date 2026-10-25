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

/// EverQuest headings run from 0 to 512 for a full turn.
pub const HEADING_UNITS: f32 = 512.0;

/// Converts an EverQuest heading (0..512) to degrees (0..360).
pub fn heading_to_degrees(heading: f32) -> f32 {
    (heading * 360.0 / HEADING_UNITS).rem_euclid(360.0)
}

/// Converts degrees to an EverQuest heading (0..512).
pub fn degrees_to_heading(degrees: f32) -> f32 {
    (degrees * HEADING_UNITS / 360.0).rem_euclid(HEADING_UNITS)
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

    /// Raw EverQuest heading, 0..512.
    pub fn heading(&self) -> f32 {
        self.raw.Heading()
    }

    pub fn heading_degrees(&self) -> f32 {
        heading_to_degrees(self.raw.Heading())
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

    #[test]
    fn headings_convert_both_ways() {
        assert_eq!(heading_to_degrees(0.0), 0.0);
        assert_eq!(heading_to_degrees(128.0), 90.0);
        assert_eq!(heading_to_degrees(512.0), 0.0);
        assert_eq!(heading_to_degrees(-128.0), 270.0);
        assert_eq!(degrees_to_heading(180.0), 256.0);
        assert_eq!(degrees_to_heading(-90.0), 384.0);
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
            raw.set_Heading(256.0);
            let mut name = raw.DisplayedName();
            for (d, s) in name.iter_mut().zip(b"a rat\0") {
                *d = *s as std::ffi::c_char;
            }
            raw.set_DisplayedName(name);

            assert_eq!(spawn.id(), SpawnId(77));
            assert_eq!(spawn.level(), 65);
            assert_eq!(spawn.spawn_type(), SpawnType::Npc);
            assert_eq!(spawn.position(), Position { x: 10.0, y: -20.0, z: 3.5 });
            assert_eq!(spawn.heading_degrees(), 180.0);
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
