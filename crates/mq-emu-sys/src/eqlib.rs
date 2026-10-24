//! Imports from `eqlib.dll` (`EQLIB_VAR`).

use crate::types::{ForeignPointer, PlayerClient};

imports! {
    dll: "eqlib",
    names: EQLIB_IMPORTS,
    functions {}
    statics {
        // eqlib/Globals.h
        /// The local player's spawn.
        pub static mut pLocalPlayer: ForeignPointer<PlayerClient>;
        /// The spawn whose movement you control (usually the local player).
        pub static mut pControlledPlayer: ForeignPointer<PlayerClient>;
        /// The current target, if any.
        pub static mut pTarget: ForeignPointer<PlayerClient>;
    }
}
