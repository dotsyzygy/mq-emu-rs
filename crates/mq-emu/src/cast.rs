//! Casting and memorizing as tasks that always say how they ended.
//!
//! MacroQuest reports no cast outcomes, so [`Task::cast`] works them out from
//! the casting state and the client's messages by their eqstr_us.txt IDs
//! (see [`Task::watch_messages`]), never their English text.
//!
//! ```ignore
//! mq.spawn_task(|task: Task<MyPlugin>| async move {
//!     match task.cast(Cast::new(heal).on(tank).restoring_target()).await {
//!         CastResult::Landed => {}
//!         CastResult::Interrupted | CastResult::Fizzled => { /* try again */ }
//!         other => task.with(|mq| mq.write_chat(&format!("heal: {other}"))),
//!     }
//! });
//! ```

use std::fmt;
use std::time::Duration;

use mq_emu_sys::constants::NUM_SPELL_GEMS;

use crate::game::SpawnId;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::spells::SpellId;
use crate::tasks::{Messages, Task};
use eq_client::eqstr::*;

/// How long the server has to start a cast before it counts as not started.
const START_WAIT: Duration = Duration::from_millis(1500);
/// How long after a cast ends its resist or immune message can still arrive.
const SETTLE: Duration = Duration::from_millis(750);
/// Casts shorter than this may start and end between two pulses.
const INSTANT: Duration = Duration::from_millis(500);
/// A cast last seen with no more than this left ran its time; one that stops
/// with more left ended early. A landed cast's state can clear before the
/// client's end time (one stopped with 525 ms left, its sound playing).
const DUE: Duration = Duration::from_millis(750);
/// Added to the cast time for [`Cast`]'s default timeout.
const TIMEOUT_MARGIN: Duration = Duration::from_secs(5);
/// How long [`Task::memorize`] waits for a gem.
const MEMORIZE_TIMEOUT: Duration = Duration::from_secs(30);
/// `TargetType_TargetCorpse`.
const TARGET_CORPSE: u8 = 15;
/// Spells on several PCs at once: AE PC v1, Group v1, AE PC v2, Group v2.
const TARGET_PCS: [u8; 4] = [2, 3, 40, 41];

/// What [`Task::cast`] casts and on whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cast {
    spell: SpellId,
    target: Option<SpawnId>,
    restore_target: bool,
    timeout: Option<Duration>,
}

impl Cast {
    /// Casts `spell` from the gem it's memorized in, on your current target.
    pub fn new(spell: SpellId) -> Cast {
        Cast { spell, target: None, restore_target: false, timeout: None }
    }

    /// Targets `spawn` first.
    pub fn on(self, spawn: SpawnId) -> Cast {
        Cast { target: Some(spawn), ..self }
    }

    /// Goes back to the previous target (or none) once the cast has ended,
    /// if [`on`](Cast::on) changed it. Not when the task is cancelled.
    pub fn restoring_target(self) -> Cast {
        Cast { restore_target: true, ..self }
    }

    /// Gives up, stopping the cast, after `timeout`. The default is the
    /// spell's cast time and 5 seconds.
    pub fn timeout(self, timeout: Duration) -> Cast {
        Cast { timeout: Some(timeout), ..self }
    }
}

/// How a cast ended. The variants up to [`Distracted`](CastResult::Distracted)
/// come from the client's messages; when a cast gets several, the later one in
/// this list wins, as in MQ2Cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CastResult {
    /// It finished with no message saying otherwise.
    Landed,
    Interrupted,
    Resisted,
    /// The gem or the global cooldown wasn't ready.
    NotReady,
    Fizzled,
    NotStanding,
    Stunned,
    NotEnoughMana,
    OutOfRange,
    /// No target, a dead one, or one the spell doesn't work on.
    NoTarget,
    /// No line of sight.
    CannotSee,
    MissingComponents,
    /// Not in this zone, or only outdoors.
    NotHere,
    /// A stronger or conflicting effect blocked it.
    DidNotTakeHold,
    Immune,
    /// Too distracted, silenced or invulnerable.
    Distracted,
    /// Not in game.
    NotInGame,
    /// Not memorized in one of the 12 gems, or not a spell the client knows.
    NotMemorized,
    AlreadyCasting,
    Feigning,
    /// The game didn't start it and said nothing.
    NotStarted,
    /// It was still casting at the timeout, and was stopped.
    TimedOut,
    /// It stopped before its cast time was up, and the client said nothing:
    /// something else stopped it, or the server did. It didn't land.
    EndedEarly,
    /// The client's messages can't be watched (eqclient.dll missing, or a
    /// client build it doesn't know), so the outcome couldn't be known.
    /// Nothing was cast.
    NoMessages,
}

impl CastResult {
    pub fn is_landed(self) -> bool {
        self == CastResult::Landed
    }

    /// Whether the spell went off: it landed, or reached its target and was
    /// resisted, didn't take hold (a stronger buff was on) or met an immune
    /// target. Its mana is spent and its gem is recasting; casting it again
    /// at once won't do better. Every other outcome ended before that.
    pub fn completed(self) -> bool {
        matches!(self, CastResult::Landed | CastResult::Resisted | CastResult::DidNotTakeHold | CastResult::Immune)
    }
}

impl fmt::Display for CastResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CastResult::Landed => "landed",
            CastResult::Interrupted => "interrupted",
            CastResult::Resisted => "resisted",
            CastResult::NotReady => "not ready",
            CastResult::Fizzled => "fizzled",
            CastResult::NotStanding => "not standing",
            CastResult::Stunned => "stunned",
            CastResult::NotEnoughMana => "not enough mana",
            CastResult::OutOfRange => "out of range",
            CastResult::NoTarget => "no target",
            CastResult::CannotSee => "can't see the target",
            CastResult::MissingComponents => "missing components",
            CastResult::NotHere => "doesn't work here",
            CastResult::DidNotTakeHold => "didn't take hold",
            CastResult::Immune => "immune",
            CastResult::Distracted => "distracted",
            CastResult::NotInGame => "not in game",
            CastResult::NotMemorized => "not memorized",
            CastResult::AlreadyCasting => "already casting",
            CastResult::Feigning => "feigning",
            CastResult::NotStarted => "not started",
            CastResult::TimedOut => "timed out",
            CastResult::EndedEarly => "ended before its cast time, with no message",
            CastResult::NoMessages => "the client's messages can't be watched",
        })
    }
}

/// How [`Task::memorize`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MemorizeResult {
    /// The spell is in the gem and ready to cast.
    Memorized,
    /// Not in your book, above your level, or the gem isn't one of the 12.
    Refused,
    TimedOut,
}

/// A message's meaning for a cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Line {
    Began,
    Ended(CastResult),
}

/// Every message a cast watches, with what it means.
const MESSAGES: &[(u32, Line)] = {
    use CastResult::*;
    use Line::{Began, Ended};
    &[
        (BEGIN_CASTING, Began),
        (SPELL_INTERRUPTED, Ended(Interrupted)),
        (CASTING_INTERRUPTED, Ended(Interrupted)),
        (TARGET_RESISTED, Ended(Resisted)),
        (RECAST_NOT_MET, Ended(NotReady)),
        (RECOVERY_NOT_MET, Ended(NotReady)),
        (NOT_RECOVERED, Ended(NotReady)),
        (SPELL_FIZZLES, Ended(Fizzled)),
        (MUST_BE_STANDING, Ended(NotStanding)),
        (CANT_CAST_STUNNED, Ended(Stunned)),
        (INSUFFICIENT_MANA, Ended(NotEnoughMana)),
        (TARGET_OUT_OF_RANGE, Ended(OutOfRange)),
        (MUST_SELECT_TARGET, Ended(NoTarget)),
        (MUST_TARGET_GROUP_MEMBER, Ended(NoTarget)),
        (216, Ended(NoTarget)),
        (217, Ended(NoTarget)),
        (218, Ended(NoTarget)),
        (219, Ended(NoTarget)),
        (220, Ended(NoTarget)),
        (221, Ended(NoTarget)),
        (222, Ended(NoTarget)),
        (223, Ended(NoTarget)),
        (224, Ended(NoTarget)),
        (225, Ended(NoTarget)),
        (226, Ended(NoTarget)),
        (227, Ended(NoTarget)),
        (228, Ended(NoTarget)),
        (229, Ended(NoTarget)),
        (CANNOT_SEE_TARGET, Ended(CannotSee)),
        (MISSING_COMPONENTS, Ended(MissingComponents)),
        (405, Ended(MissingComponents)),
        (406, Ended(MissingComponents)),
        (407, Ended(MissingComponents)),
        (408, Ended(MissingComponents)),
        (SPELL_DOES_NOT_WORK_HERE, Ended(NotHere)),
        (ONLY_OUTDOORS, Ended(NotHere)),
        (SPELL_DID_NOT_TAKE_HOLD, Ended(DidNotTakeHold)),
        (SPELL_WOULD_NOT_TAKE_HOLD, Ended(DidNotTakeHold)),
        (SPELL_TOO_POWERFUL, Ended(DidNotTakeHold)),
        (IMMUNE_TO_SLOW, Ended(Immune)),
        (IMMUNE_TO_SNARE, Ended(Immune)),
        (IMMUNE_TO_FEAR, Ended(Immune)),
        (IMMUNE_TO_STUN, Ended(Immune)),
        (IMMUNE_TO_SPELL, Ended(Immune)),
        (TARGET_HAS_NO_MANA, Ended(Immune)),
        (TARGET_UNAFFECTED, Ended(Immune)),
        (CANNOT_BE_MESMERIZED, Ended(Immune)),
        (CANNOT_BE_MESMERIZED_BY_THIS, Ended(Immune)),
        (TOO_DISTRACTED_TO_CAST, Ended(Distracted)),
        (CANT_CAST_INVULNERABLE, Ended(Distracted)),
        (SILENCED, Ended(Distracted)),
    ]
};

fn classify(id: u32) -> Option<Line> {
    MESSAGES.iter().find(|(message, _)| *message == id).map(|(_, line)| *line)
}

/// A cast that passed its checks.
struct Prepared {
    gem: usize,
    cast_time: Duration,
    /// Set when the cast changed your target.
    previous_target: Option<Option<SpawnId>>,
    stood: bool,
    /// On several PCs: one that a stronger buff blocks says it didn't take
    /// hold, but the others got it.
    on_pcs: bool,
}

/// The checks that need nothing from the game but reading, then standing and
/// targeting.
fn prepare<P: Plugin>(mq: &Mq<P>, cast: &Cast) -> Result<Prepared, CastResult> {
    let me = mq.me().ok_or(CastResult::NotInGame)?;
    let spell = mq.spell(cast.spell).ok_or(CastResult::NotMemorized)?;
    if me.casting().is_some() {
        return Err(CastResult::AlreadyCasting);
    }
    if mq.is_stunned() {
        return Err(CastResult::Stunned);
    }
    if me.is_feigning() {
        return Err(CastResult::Feigning);
    }
    let gem = (0..NUM_SPELL_GEMS).find(|&gem| mq.gem(gem).is_some_and(|memorized| memorized.id() == cast.spell)).ok_or(CastResult::NotMemorized)?;
    if !mq.gem_ready(gem) {
        return Err(CastResult::NotReady);
    }
    if mq.mana_current().unwrap_or(0) < spell.mana_cost() {
        return Err(CastResult::NotEnoughMana);
    }
    let mut previous_target = None;
    if let Some(id) = cast.target {
        let target = mq.spawn(id).ok_or(CastResult::NoTarget)?;
        if target.is_corpse() != (spell.target_type() == TARGET_CORPSE) {
            return Err(CastResult::NoTarget);
        }
        if spell.range() > 0.0 && me.distance_to(&target) > spell.range() {
            return Err(CastResult::OutOfRange);
        }
        let current = mq.target().map(|target| target.id());
        if current != Some(id) {
            mq.set_target(Some(&target));
            previous_target = Some(current);
        }
    }
    let stood = me.is_sitting() && mq.eq_command("/stand", "");
    let on_pcs = TARGET_PCS.contains(&spell.target_type());
    Ok(Prepared { gem, cast_time: Duration::from_millis(u64::from(spell.cast_time_ms())), previous_target, stood, on_pcs })
}

/// Stops the cast if the task is cancelled while it's casting.
struct StopOnDrop<'a, P: Plugin> {
    task: &'a Task<P>,
    spell: SpellId,
    armed: bool,
}

impl<P: Plugin> Drop for StopOnDrop<'_, P> {
    fn drop(&mut self) {
        if self.armed {
            self.task.with(|mq| {
                if mq.me().and_then(|me| me.casting()) == Some(self.spell) {
                    mq.stop_casting();
                }
            });
        }
    }
}

impl<P: Plugin> Task<P> {
    /// Casts a spell from its gem and waits until it has ended, saying how.
    /// It checks first, without touching the game: in game, the spell
    /// memorized in one of the 12 gems and ready, enough mana, not already
    /// casting, stunned or feigning, and for [`Cast::on`] the target there,
    /// alive (dead for a spell on corpses) and in range. A failed check is
    /// returned at once. Then it stands if you're sitting, targets, and casts.
    ///
    /// The outcome comes from the casting state and the client's messages by
    /// ID (the ones MQ2Cast reads as text). A cast [`Landed`](CastResult::Landed)
    /// only if it ran its time ([`cast_time_left`](Mq::cast_time_left) was
    /// about done when it stopped) and no message said otherwise (a group or
    /// AE PC spell lands even if it didn't take hold on someone); stopping
    /// sooner without one is [`EndedEarly`](CastResult::EndedEarly). Messages can come up to a moment after
    /// the cast ends, so this waits that long. It always ends, at the latest
    /// at the [timeout](Cast::timeout). Without the client's messages
    /// (eqclient.dll), nothing is cast: [`NoMessages`](CastResult::NoMessages).
    ///
    /// Cancelling the task stops the cast, as an emergency heal needs.
    pub async fn cast(&self, cast: Cast) -> CastResult {
        let ids: Vec<u32> = MESSAGES.iter().map(|(id, _)| *id).collect();
        let Ok(messages) = self.watch_messages(&ids) else { return CastResult::NoMessages };
        let prepared = match self.with(|mq| prepare(mq, &cast)) {
            Ok(prepared) => prepared,
            Err(result) => return result,
        };
        if prepared.stood {
            self.next_pulse().await;
        }
        let mut stop = StopOnDrop { task: self, spell: cast.spell, armed: true };
        let result = self.follow(&cast, &prepared, &messages).await;
        stop.armed = false;
        if let (true, Some(previous)) = (cast.restore_target, prepared.previous_target) {
            self.with(|mq| {
                let previous = previous.and_then(|id| mq.spawn(id));
                mq.set_target(previous.as_ref());
            });
        }
        result
    }

    async fn follow(&self, cast: &Cast, prepared: &Prepared, messages: &Messages) -> CastResult {
        if !self.with(|mq| mq.cast_gem(prepared.gem)) {
            return CastResult::NotStarted;
        }
        let now = self.now();
        let start = now();
        let timeout = cast.timeout.unwrap_or(prepared.cast_time + TIMEOUT_MARGIN);
        let mut started = prepared.cast_time < INSTANT;
        // Whether this cast is known to have begun. An interrupt before then
        // is an earlier cast's, arriving late (a stopped one's, say). The
        // server's messages come in order, so an earlier cast's (its late
        // "begin casting" too) all come before this one's begin: each begin
        // drops what came before it.
        let mut began = false;
        let mut worst: Option<CastResult> = None;
        let mut ended_at = None;
        // The cast time left when last seen casting.
        let mut last_left: Option<Duration> = None;
        loop {
            self.next_pulse().await;
            for id in messages.take() {
                match classify(id) {
                    Some(Line::Began) => (started, began, worst) = (true, true, None),
                    Some(Line::Ended(CastResult::Interrupted)) if !began => {}
                    // One member's; it landed if it ran its time.
                    Some(Line::Ended(CastResult::DidNotTakeHold)) if prepared.on_pcs => {}
                    Some(Line::Ended(result)) => worst = worst.max(Some(result)),
                    None => {}
                }
            }
            let (casting, left) = self.with(|mq| (mq.me().and_then(|me| me.casting()) == Some(cast.spell), mq.cast_time_left()));
            if casting {
                (started, began) = (true, true);
                ended_at = None;
                last_left = left.or(last_left);
                if now().duration_since(start) >= timeout {
                    self.with(|mq| mq.stop_casting());
                    return CastResult::TimedOut;
                }
                continue;
            }
            if !started && worst.is_none() {
                if now().duration_since(start) >= START_WAIT {
                    return CastResult::NotStarted;
                }
                continue;
            }
            let ended = *ended_at.get_or_insert_with(now);
            if now().duration_since(ended) >= SETTLE {
                let ran_its_time = prepared.cast_time < INSTANT || last_left.is_some_and(|left| left <= DUE);
                return worst.unwrap_or(if ran_its_time { CastResult::Landed } else { CastResult::EndedEarly });
            }
        }
    }

    /// Memorizes a spell from your book into gem `gem` (0-based) and waits
    /// until it's there and ready to cast. Ends at once if it already is.
    pub async fn memorize(&self, gem: usize, spell: SpellId) -> MemorizeResult {
        let done = move |mq: &Mq<P>| mq.gem(gem).is_some_and(|memorized| memorized.id() == spell) && mq.gem_ready(gem);
        if self.with(done) {
            return MemorizeResult::Memorized;
        }
        if !self.with(|mq| mq.gem(gem).is_some_and(|memorized| memorized.id() == spell) || mq.memorize_spell(gem, spell)) {
            return MemorizeResult::Refused;
        }
        if self.wait_until_timeout(done, MEMORIZE_TIMEOUT).await { MemorizeResult::Memorized } else { MemorizeResult::TimedOut }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_means_one_thing() {
        for (n, (id, _)) in MESSAGES.iter().enumerate() {
            assert!(!MESSAGES[n + 1..].iter().any(|(other, _)| other == id), "{id} is listed twice");
        }
        assert_eq!(classify(BEGIN_CASTING), Some(Line::Began));
        assert_eq!(classify(TARGET_RESISTED), Some(Line::Ended(CastResult::Resisted)));
        assert_eq!(classify(1), None);
        assert!(eq_client::eqstr::ONLY_WORKS_ON.clone().all(|id| classify(id) == Some(Line::Ended(CastResult::NoTarget))));
        assert!(eq_client::eqstr::NEED_INSTRUMENT.clone().all(|id| classify(id) == Some(Line::Ended(CastResult::MissingComponents))));
    }

    #[test]
    fn only_spells_that_went_off_completed() {
        use CastResult::*;
        let completed = [Landed, Resisted, DidNotTakeHold, Immune];
        let ended_before = [
            Interrupted, NotReady, Fizzled, NotStanding, Stunned, NotEnoughMana, OutOfRange, NoTarget, CannotSee, MissingComponents, NotHere, Distracted,
            NotInGame, NotMemorized, AlreadyCasting, Feigning, NotStarted, TimedOut, EndedEarly, NoMessages,
        ];
        assert!(completed.iter().all(|result| result.completed()));
        assert!(ended_before.iter().all(|result| !result.completed()), "{:?}", ended_before.iter().find(|result| result.completed()));
    }

    #[test]
    fn the_later_outcome_wins() {
        assert!(CastResult::Resisted > CastResult::Interrupted);
        assert!(CastResult::Immune > CastResult::Resisted);
        assert_eq!(Some(CastResult::Interrupted).max(Some(CastResult::Fizzled)), Some(CastResult::Fizzled));
    }
}
