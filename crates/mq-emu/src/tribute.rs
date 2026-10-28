//! Your tribute (`${Me.TributeActive}`, `${Me.CurrentFavor}`...).

use std::time::Duration;

use crate::mq::Mq;
use crate::plugin::Plugin;

/// Your tribute: favor and whether its benefits are on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tribute {
    /// Whether your tribute benefits are on (`${Me.TributeActive}`).
    pub active: bool,
    /// Favor you have to spend (`${Me.CurrentFavor}`).
    pub current_favor: i64,
    /// Favor you've earned in all (`${Me.CareerFavor}`).
    pub career_favor: i64,
    /// What the active benefits cost each time the timer runs out
    /// (`${Me.ActiveFavorCost}`), while they're on.
    pub active_cost: Option<i32>,
    /// Until the active benefits next cost favor (`${Me.TributeTimer}`).
    pub time_left: Duration,
}

impl<P: Plugin> Mq<P> {
    /// Your tribute, in game.
    pub fn tribute(&self) -> Option<Tribute> {
        let pc = self.local_pc()?;
        let active = (self.host().tribute_active)();
        let api = self.host().game_fns;
        Some(Tribute {
            active,
            current_favor: pc.CurrFavor(),
            career_favor: pc.CareerFavor(),
            active_cost: active.then(|| unsafe { (api.mqemu_active_favor_cost)() }),
            time_left: Duration::from_millis(u64::try_from(pc.BenefitTimer()).unwrap_or(0)),
        })
    }
}
