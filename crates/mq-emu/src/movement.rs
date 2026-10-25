//! Moving and turning, the way MacroQuest's own movement does it: holding the
//! game's movement keys (`EQExecuteCmd`, as if the player pressed them) and
//! turning through MacroQuest's smooth turn (`gFaceAngle`, applied a little
//! each pulse).
//!
//! Everything here acts on the *controlled* spawn: you, or your mount while
//! riding. Running game commands outside the in-game state crashes the
//! client, so every action checks the game state first.

use std::fmt;

use mq_emu_sys::constants as c;
use mq_emu_sys::layouts::EverQuestinfo;

use crate::game::{GameState, Heading, Position, Spawn};
use crate::mq::Mq;
use crate::plugin::Plugin;

/// A movement-related key-bindable game command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoveKey {
    Forward,
    Back,
    TurnLeft,
    TurnRight,
    StrafeLeft,
    StrafeRight,
    Jump,
    /// Toggles auto-run.
    AutoRun,
    /// Toggles ducking.
    Duck,
    /// Toggles between running and walking.
    RunWalk,
}

impl MoveKey {
    pub const ALL: [MoveKey; 10] = [
        MoveKey::Forward,
        MoveKey::Back,
        MoveKey::TurnLeft,
        MoveKey::TurnRight,
        MoveKey::StrafeLeft,
        MoveKey::StrafeRight,
        MoveKey::Jump,
        MoveKey::AutoRun,
        MoveKey::Duck,
        MoveKey::RunWalk,
    ];

    /// Keys that move or turn while held; [`Mq::stop_moving`] releases them.
    pub const HELD: [MoveKey; 6] =
        [MoveKey::Forward, MoveKey::Back, MoveKey::TurnLeft, MoveKey::TurnRight, MoveKey::StrafeLeft, MoveKey::StrafeRight];

    /// The game command (`CMD_*`) behind this key.
    pub fn command(self) -> u32 {
        match self {
            MoveKey::Forward => c::CMD_FORWARD,
            MoveKey::Back => c::CMD_BACK,
            MoveKey::TurnLeft => c::CMD_LEFT,
            MoveKey::TurnRight => c::CMD_RIGHT,
            MoveKey::StrafeLeft => c::CMD_STRAFE_LEFT,
            MoveKey::StrafeRight => c::CMD_STRAFE_RIGHT,
            MoveKey::Jump => c::CMD_JUMP,
            MoveKey::AutoRun => c::CMD_AUTORUN,
            MoveKey::Duck => c::CMD_DUCK,
            MoveKey::RunWalk => c::CMD_RUN_WALK,
        }
    }

    /// The game's name for the command, as `/keypress` takes it.
    pub fn game_name(self) -> &'static str {
        match self {
            MoveKey::Forward => "forward",
            MoveKey::Back => "back",
            MoveKey::TurnLeft => "left",
            MoveKey::TurnRight => "right",
            MoveKey::StrafeLeft => "strafe_left",
            MoveKey::StrafeRight => "strafe_right",
            MoveKey::Jump => "jump",
            MoveKey::AutoRun => "autorun",
            MoveKey::Duck => "duck",
            MoveKey::RunWalk => "run_walk",
        }
    }
}

/// Why a movement action wasn't taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveError {
    /// Game commands outside the in-game state crash the client, so none were sent.
    NotInGame,
    /// There's no controlled spawn (e.g. still zoning).
    NoPlayer,
    /// The game doesn't allow this command right now (as `/keypress` reports
    /// "disabled"), so it wasn't pressed.
    Disabled,
}

impl fmt::Display for MoveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MoveError::NotInGame => "not in game",
            MoveError::NoPlayer => "no controlled spawn",
            MoveError::Disabled => "the game has that command disabled right now",
        })
    }
}

impl std::error::Error for MoveError {}

/// Whether the game considers the key's command down. This is the game's own
/// state (`EverQuestinfo::keyDown`), set however the command was pressed;
/// `KeypressHandler::CommandState` only tracks physical keys.
pub(crate) fn is_down(info: &EverQuestinfo, key: MoveKey) -> bool {
    // Every MoveKey's command is within the array (keyDown_at checks bounds).
    info.keyDown_at(key.command() as usize) != 0
}

impl<P: Plugin> Mq<P> {
    fn ensure_in_game(&self) -> Result<(), MoveError> {
        if self.game_state() == GameState::InGame { Ok(()) } else { Err(MoveError::NotInGame) }
    }

    /// The spawn whose movement you control: you, or your mount while riding.
    pub fn controlled(&self) -> Option<Spawn<'_>> {
        unsafe { Spawn::from_raw((self.host().controlled_player)()) }
    }

    // ----- keys -----

    /// Holds (`down = true`) or releases a movement key, exactly as if the
    /// player pressed it. Held keys stay down until released, including
    /// across pulses; [`Mq::stop_moving`] releases them all. Like `/keypress`,
    /// a command the game has disabled isn't pressed; releasing always works,
    /// so a key can't get stuck down.
    pub fn set_key(&self, key: MoveKey, down: bool) -> Result<(), MoveError> {
        self.ensure_in_game()?;
        if down && !(self.host().command_enabled)(key.command()) {
            return Err(MoveError::Disabled);
        }
        (self.host().execute_cmd)(key.command(), down);
        Ok(())
    }

    /// Presses and releases a key, e.g. to jump or toggle auto-run.
    pub fn tap_key(&self, key: MoveKey) -> Result<(), MoveError> {
        self.set_key(key, true)?;
        self.set_key(key, false)
    }

    /// Whether the game currently allows this key's command; [`Mq::set_key`]
    /// won't press it otherwise.
    pub fn is_key_enabled(&self, key: MoveKey) -> bool {
        (self.host().command_enabled)(key.command())
    }

    /// True while the game considers the key held, by anyone (the player or a plugin).
    pub fn is_key_down(&self, key: MoveKey) -> bool {
        unsafe { (self.host().everquest_info)().as_ref() }.is_some_and(|info| is_down(info, key))
    }

    /// Releases every held movement key and stops any smooth turn. Toggles
    /// (auto-run, ducking) are left alone.
    pub fn stop_moving(&self) -> Result<(), MoveError> {
        self.ensure_in_game()?;
        for key in MoveKey::HELD {
            if self.is_key_down(key) {
                (self.host().execute_cmd)(key.command(), false);
            }
        }
        self.stop_turning();
        Ok(())
    }

    // ----- turning -----

    /// Starts turning smoothly towards `heading`, a few degrees per pulse,
    /// like MacroQuest's `/face`. Check [`Mq::is_turning`] to see when it's done.
    pub fn face(&self, heading: Heading) -> Result<(), MoveError> {
        self.ensure_in_game()?;
        if self.controlled().is_none() {
            return Err(MoveError::NoPlayer);
        }
        (self.host().set_face_angle)(f64::from(heading.units()));
        Ok(())
    }

    /// Starts turning smoothly towards a point.
    pub fn face_point(&self, point: &Position) -> Result<(), MoveError> {
        let from = self.controlled().ok_or(MoveError::NoPlayer)?.position();
        self.face(Heading::between(&from, point))
    }

    /// Turns to `heading` at once (and cancels any smooth turn).
    pub fn face_now(&self, heading: Heading) -> Result<(), MoveError> {
        self.ensure_in_game()?;
        let controlled = self.controlled().ok_or(MoveError::NoPlayer)?;
        self.stop_turning();
        controlled.raw().set_Heading(heading.units());
        Ok(())
    }

    /// The heading a smooth turn is heading for, if one is in progress.
    pub fn turn_target(&self) -> Option<Heading> {
        let angle = (self.host().face_angle)();
        (angle != c::FACE_ANGLE_INACTIVE).then(|| Heading::from_units(angle as f32))
    }

    pub fn is_turning(&self) -> bool {
        self.turn_target().is_some()
    }

    /// Cancels a smooth turn, leaving the current heading as it is.
    pub fn stop_turning(&self) {
        (self.host().set_face_angle)(c::FACE_ANGLE_INACTIVE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn keys_map_to_eqlib_commands() {
        assert_eq!(MoveKey::Forward.command(), 2);
        assert_eq!(MoveKey::TurnRight.command(), 4);
        assert_eq!(MoveKey::TurnLeft.command(), 5);
        assert_eq!(MoveKey::RunWalk.command(), 284);
        let commands: HashSet<_> = MoveKey::ALL.iter().map(|k| k.command()).collect();
        assert_eq!(commands.len(), MoveKey::ALL.len(), "two keys share a command");
        let names: HashSet<_> = MoveKey::ALL.iter().map(|k| k.game_name()).collect();
        assert_eq!(names.len(), MoveKey::ALL.len(), "two keys share a name");
    }

    #[test]
    fn held_keys_are_the_movement_keys() {
        for key in MoveKey::HELD {
            assert!(!matches!(key, MoveKey::Jump | MoveKey::AutoRun | MoveKey::Duck | MoveKey::RunWalk), "{key:?}");
        }
    }

    #[test]
    fn key_state_reads_the_games_key_down_array() {
        let mut storage = vec![0u64; size_of::<EverQuestinfo>().div_ceil(8)];
        let info = unsafe { &*storage.as_mut_ptr().cast::<EverQuestinfo>() };
        assert!(!is_down(info, MoveKey::Forward));
        info.set_keyDown_at(MoveKey::Forward.command() as usize, 1);
        info.set_keyDown_at(MoveKey::RunWalk.command() as usize, 1);
        assert!(is_down(info, MoveKey::Forward));
        assert!(is_down(info, MoveKey::RunWalk));
        assert!(!is_down(info, MoveKey::Back));
    }
}
