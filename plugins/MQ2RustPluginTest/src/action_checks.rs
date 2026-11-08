//! Selftest checks for acting on spells and items: a spell cast and stopped,
//! cast as a task that's cancelled and one that lands, a spell memorized and forgotten (leaving the gems as they were), a stack
//! split and its part put back by hand and with /autoinventory, and items
//! selected in an open merchant window (nothing is bought or sold).

use std::time::Duration;

use mq_emu::{Cast, CastResult, GameState, eqstr, ItemLocation, MemorizeResult, Mq, SlotLocation, SpellId, Task};

use crate::RustTest;
use crate::checks::{Check, Report};

/// How long a cast, a stop or an item move may take to show.
const TIMEOUT: Duration = Duration::from_secs(3);
/// Tries at a cast that lands, as each may fizzle.
const CAST_ATTEMPTS: u32 = 3;
/// How long a cast task may take: the spell, its message and the gem's recast.
const CAST_TASK_TIMEOUT: Duration = Duration::from_secs(20);
/// Spell target types (`eSpellTargetType`) that reach only you and your group.
const TT_GROUP_V1: u8 = 3;
const TT_SELF: u8 = 6;
const TT_GROUP_V2: u8 = 41;
/// The gems spells are cast from.
const GEMS: usize = 12;
/// Windows the game won't cast while open: merchant, trade, give, loot, bank.
const NO_CASTING_WINDOWS: [&str; 5] = ["MerchantWnd", "TradeWnd", "GiveWnd", "LootWnd", "BigBankWnd"];

fn casting(mq: &Mq<RustTest>) -> Option<SpellId> {
    mq.me().and_then(|me| me.casting())
}

fn in_game(mq: &Mq<RustTest>, report: &mut Report, what: &str) -> bool {
    let in_game = mq.game_state() == GameState::InGame;
    if !in_game {
        report.print(mq, what, Check::Skip("needs to be in game".into()));
    }
    in_game
}

pub async fn spells(task: &Task<RustTest>, report: &mut Report) {
    let ready = task.with(|mq| {
        if !in_game(mq, report, "acting on spells") {
            return false;
        }
        let ours: Vec<bool> = (0..GEMS).map(|gem| mq.gem_ready(gem)).collect();
        let theirs: Vec<bool> = (1..=GEMS).map(|gem| mq.parse(&format!("${{Me.SpellReady[{gem}]}}")) == "TRUE").collect();
        report.print(mq, "gem_ready matches ${Me.SpellReady[n]} for every gem", Check::that(ours == theirs, || format!("mq-emu {ours:?}, MacroQuest {theirs:?}")));
        let (ours, theirs) = (mq.gem_count().to_string(), mq.parse("${Me.NumGems}"));
        report.print(mq, "gem_count matches ${Me.NumGems}", Check::that(ours == theirs, || format!("mq-emu {ours}, MacroQuest {theirs}")));
        true
    });
    if !ready {
        return;
    }
    cast_and_stop(task, report).await;
    memorize_and_forget(task, report).await;
}

/// Casts a beneficial spell that reaches only you or your group, and stops it
/// before it lands (mana is only spent when a cast completes).
async fn cast_and_stop(task: &Task<RustTest>, report: &mut Report) {
    let chosen = task.with(|mq| {
        let blocking = NO_CASTING_WINDOWS.into_iter().find(|name| mq.window(name).is_some_and(|w| w.is_open()));
        let skip = if mq.parse("${Me.Class.ShortName}") == "BRD" {
            Some("bards sing rather than cast".to_owned())
        } else if let Some(name) = blocking {
            Some(format!("the game won't cast with {name} open"))
        } else if casting(mq).is_some() {
            Some("already casting".to_owned())
        } else {
            None
        };
        let mana: i32 = mq.parse("${Me.CurrentMana}").parse().unwrap_or(0);
        let usable: Vec<_> = mq
            .gems()
            .filter(|(gem, spell)| {
                *gem < GEMS
                    && spell.is_beneficial()
                    && [TT_SELF, TT_GROUP_V1, TT_GROUP_V2].contains(&spell.target_type())
                    && spell.cast_time_ms() >= 1000
                    && spell.mana_cost() <= mana
                    && mq.gem_ready(*gem)
            })
            .collect();
        // A buff, so landing shows in your buff window; else any of them.
        let chosen = usable.iter().find(|(_, spell)| spell.duration_cap() > 0).or(usable.first()).map(|(gem, spell)| (*gem, *spell));
        let chosen = chosen.filter(|_| skip.is_none()).map(|(gem, spell)| (gem, spell.id(), spell.name()));
        if chosen.is_none() {
            let why = skip.unwrap_or_else(|| "no ready gem with a self or group buff (cast time 1s or more) you have the mana for".into());
            report.print(mq, "casting a spell and stopping it", Check::Skip(why));
        }
        chosen
    });
    let Some((gem, id, name)) = chosen else { return };

    let watched = task.watch_messages(&[eqstr::BEGIN_CASTING]);
    task.with(|mq| {
        report.print(mq, "eqclient.dll is loaded and hooks the client's messages", Check::that(watched.is_ok(), || {
            watched.as_ref().err().map(ToString::to_string).unwrap_or_default()
        }));
    });
    land(task, report, gem, id, &name, "first, before any cast is stopped").await;
    let shown = watched.map(|messages| messages.take()).unwrap_or_default();
    task.with(|mq| {
        report.print(mq, "casting shows \"You begin casting\" (12205), by ID", Check::that(shown.contains(&eqstr::BEGIN_CASTING), || format!("messages seen: {shown:?}")));
    });

    task.wait_until_timeout(move |mq| mq.gem_ready(gem), CAST_TASK_TIMEOUT).await;
    let cast = task.with(|mq| mq.cast_gem(gem));
    let started = task.wait_until_timeout(move |mq| casting(mq) == Some(id), TIMEOUT).await;
    task.with(|mq| {
        let theirs = mq.parse("${Me.Casting.ID}");
        let label = format!("cast_gem({gem}) casts {name}, as Spawn::casting and ${{Me.Casting.ID}} see");
        report.print(mq, &label, Check::that(cast && started && theirs == id.to_string(), || {
            format!("cast_gem: {cast}; casting {:?}, ${{Me.Casting.ID}} {theirs}", casting(mq))
        }));
    });
    task.with(|mq| {
        let ours = mq.cast_time_left().map(|left| left.as_millis() as i64);
        let theirs: Option<i64> = mq.parse("${Me.CastTimeLeft.Raw}").parse().ok();
        let close = matches!((ours, theirs), (Some(ours), Some(theirs)) if ours > 0 && (ours - theirs).abs() <= 100);
        report.print(mq, "cast_time_left matches ${Me.CastTimeLeft} while casting", Check::that(close, || format!("mq-emu {ours:?} ms, MacroQuest {theirs:?} ms")));
    });
    let stopped = task.with(|mq| mq.stop_casting());
    let done = task.wait_until_timeout(|mq| casting(mq).is_none(), TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, "stop_casting() stops it", Check::that(stopped && done, || format!("stop_casting: {stopped}; casting {:?}", casting(mq))));
        let left = mq.cast_time_left();
        report.print(mq, "cast_time_left is None when not casting", Check::that(left.is_none(), || format!("{left:?}")));
    });
    cast_tasks(task, report, gem, id, &name).await;
}

/// The same spell through `Task::cast`: cancelled mid-cast, then left to land.
async fn cast_tasks(task: &Task<RustTest>, report: &mut Report, gem: usize, id: SpellId, name: &str) {
    task.wait_until_timeout(move |mq| mq.gem_ready(gem), CAST_TASK_TIMEOUT).await;
    let cast = task.with(|mq| {
        mq.spawn_task(move |task: Task<RustTest>| async move {
            task.cast(Cast::new(id)).await;
        })
    });
    let started = task.wait_until_timeout(move |mq| casting(mq) == Some(id), TIMEOUT).await;
    let cancelled = task.with(|mq| mq.cancel_task(cast));
    let stopped = task.wait_until_timeout(|mq| casting(mq).is_none(), TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, &format!("cancelling a Task::cast of {name} stops the cast"), Check::that(started && cancelled && stopped, || {
            format!("started {started}, cancelled {cancelled}; casting {:?}", casting(mq))
        }));
    });

    // Stopping a cast may leave the server ending the next one early; give it time.
    task.delay(Duration::from_secs(3)).await;
    land(task, report, gem, id, name, "after two stopped casts and 3 s").await;
    let unknown = task.cast(Cast::new(SpellId(-5))).await;
    task.with(|mq| {
        report.print(mq, "Task::cast of a spell not in a gem ends NotMemorized at once", Check::that(unknown == CastResult::NotMemorized, || format!("ended {unknown:?}")));
    });
}

/// Casts `id` through `Task::cast` until it doesn't fizzle, checks it landed
/// and spent its mana, and prints a trace of it.
async fn land(task: &Task<RustTest>, report: &mut Report, gem: usize, id: SpellId, name: &str, when: &str) {
    task.wait_until_timeout(move |mq| casting(mq).is_none() && mq.gem_ready(gem), CAST_TASK_TIMEOUT).await;
    let is_buff = task.with(|mq| mq.spell(id).is_some_and(|spell| spell.duration_cap() > 0));
    let buff_before = task.with(|mq| buff_left(mq, id));
    let trace = start_trace(task);
    let watched = task.watch_messages(TRACED);
    // A fizzle is chance, not a failure: cast again once the gem is ready.
    let mut fizzles = 0;
    let result = loop {
        let result = task.cast(Cast::new(id).timeout(CAST_TASK_TIMEOUT)).await;
        if result != CastResult::Fizzled || fizzles + 1 == CAST_ATTEMPTS {
            break result;
        }
        fizzles += 1;
        task.wait_until_timeout(move |mq| casting(mq).is_none() && mq.gem_ready(gem), CAST_TASK_TIMEOUT).await;
    };
    // The buff shows up a moment after the cast lands. A refresh of one
    // already on can't be told from its time left, so only a new one counts.
    let checkable = is_buff && buff_before.is_none();
    let landed_buff = checkable && task.wait_until_timeout(move |mq| buff_left(mq, id).is_some(), Duration::from_secs(3)).await;
    let shown = watched.map(|messages| messages.take()).unwrap_or_default();
    task.with(|mq| {
        let buff_label = format!("{name} {when}: Task::cast says Landed exactly when the buff lands on you");
        if !is_buff {
            report.print(mq, &buff_label, Check::Skip(format!("{name} isn't a buff; memorize a self buff to check landing")));
        } else if !checkable {
            report.print(mq, &buff_label, Check::Skip(format!("{name} was already on you; click it off before the selftest to check landing")));
        } else {
            let agree = (result == CastResult::Landed) == landed_buff;
            report.print(mq, &buff_label, Check::that(agree, || {
                format!("Task::cast ended {result:?}; buff before {buff_before:?}, now {:?}", buff_left(mq, id))
            }));
        }
        // Didn't take hold (a stronger buff on you) still went off.
        let label = format!("Task::cast of {name} {when} completes: Landed, or the spell went off and didn't stick (fizzles cast again, up to {CAST_ATTEMPTS} tries)");
        report.print(mq, &label, Check::that(result.completed(), || format!("ended {result:?} ({result}) after {fizzles} fizzle(s); messages seen, in order: {shown:?}")));
    });
    // The trace explains a failure; otherwise it's noise. (Mana isn't a
    // check: a regeneration tick can give the cost straight back.)
    let failed = !result.completed() || (checkable && (result == CastResult::Landed) != landed_buff);
    if failed {
        end_trace(task, &trace, &format!("{name} {when}"));
    } else {
        task.with(|mq| mq.cancel_task(trace.task));
    }
}

/// Time left on your buff from `id`: `None` without it; a permanent one counts as a day.
fn buff_left(mq: &Mq<RustTest>, id: SpellId) -> Option<Duration> {
    mq.buff(id).map(|buff| buff.remaining().unwrap_or(Duration::from_secs(86_400)))
}

/// A timeline of the cast checks, while casts can't be told apart reliably:
/// every change in the casting state, time left and mana, and every cast
/// message as it arrives, printed to chat when the checks are done.
pub(crate) struct Trace {
    lines: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    task: mq_emu::TaskId,
}

const TRACED: &[u32] = &[
    eqstr::BEGIN_CASTING,
    eqstr::SPELL_INTERRUPTED,
    eqstr::CASTING_INTERRUPTED,
    eqstr::SPELL_FIZZLES,
    eqstr::TARGET_RESISTED,
    eqstr::SPELL_DID_NOT_TAKE_HOLD,
    eqstr::SPELL_WOULD_NOT_TAKE_HOLD,
    eqstr::RECAST_NOT_MET,
    eqstr::RECOVERY_NOT_MET,
    eqstr::NOT_RECOVERED,
];

pub(crate) fn start_trace(task: &Task<RustTest>) -> Trace {
    let lines = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let kept = lines.clone();
    let task = task.with(|mq| {
        mq.spawn_task(move |task: Task<RustTest>| async move {
            let messages = task.watch_messages(TRACED).ok();
            let start = std::time::Instant::now();
            let mut last = String::new();
            loop {
                let shown = messages.as_ref().map(|m| m.take()).unwrap_or_default();
                let state = task.with(|mq| {
                    let casting = mq.me().and_then(|me| me.casting()).map(|id| id.0);
                    let left = mq.cast_time_left().map(|left| left.as_millis());
                    format!("casting {casting:?}, left {left:?} ms, mana {:?}/{:?}", mq.mana_current(), mq.mana_max())
                });
                let ms = start.elapsed().as_millis();
                for message in shown {
                    kept.borrow_mut().push(format!("{ms:>6} ms  message {message}"));
                }
                if state != last {
                    kept.borrow_mut().push(format!("{ms:>6} ms  {state}"));
                    last = state;
                }
                task.next_pulse().await;
            }
        })
    });
    Trace { lines, task }
}

pub(crate) fn end_trace(task: &Task<RustTest>, trace: &Trace, name: &str) {
    task.with(|mq| {
        mq.cancel_task(trace.task);
        crate::say(mq, &format!("cast trace for {name} (12205 begin, 439/12477 interrupted, 173 fizzle):"));
        for line in trace.lines.borrow().iter() {
            crate::say(mq, line);
        }
    });
}

/// Memorizes a spell into an empty gem and forgets it again; with no empty
/// gem, forgets the last gem's spell and memorizes it back.
async fn memorize_and_forget(task: &Task<RustTest>, report: &mut Report) {
    let plan = task.with(|mq| {
        let gems = mq.parse("${Me.NumGems}").parse::<usize>().unwrap_or(8).min(GEMS);
        let level = mq.me().map_or(0, |me| me.level());
        let memorized: Vec<SpellId> = mq.gems().map(|(_, spell)| spell.id()).collect();
        let plan = match (0..gems).find(|gem| mq.gem(*gem).is_none()) {
            Some(empty) => mq
                .spell_book()
                .find(|(_, spell)| !memorized.contains(&spell.id()) && mq.spell_level(spell).is_some_and(|needed| needed <= level))
                .map(|(_, spell)| (empty, spell.id(), spell.name(), true)),
            None => (0..gems).rev().find_map(|gem| mq.gem(gem).map(|spell| (gem, spell.id(), spell.name(), false))),
        };
        if plan.is_none() {
            report.print(mq, "memorizing and forgetting a spell", Check::Skip("no empty gem and a spell in your book to put in it".into()));
        }
        plan.map(|plan| (plan, mq.parse("${Me.Standing}") == "TRUE"))
    });
    let Some(((gem, id, name, into_empty), standing)) = plan else { return };
    // The game won't memorize while casting.
    task.wait_until_timeout(|mq| casting(mq).is_none(), TIMEOUT).await;

    if into_empty {
        memorize(task, report, gem, id, &name).await;
        forget(task, report, gem, &name).await;
    } else {
        forget(task, report, gem, &name).await;
        memorize(task, report, gem, id, &name).await;
    }
    // Memorizing sits you down.
    task.with(|mq| {
        if standing && mq.parse("${Me.Standing}") != "TRUE" {
            mq.command("/stand");
        }
    });
}

async fn memorize(task: &Task<RustTest>, report: &mut Report, gem: usize, id: SpellId, name: &str) {
    // Task::memorize asks with memorize_spell, and waits for the gem to be ready.
    let result = task.memorize(gem, id).await;
    task.with(|mq| {
        let theirs = mq.parse(&format!("${{Me.Gem[{}].ID}}", gem + 1));
        let ready = mq.parse(&format!("${{Me.SpellReady[{}]}}", gem + 1));
        let label = format!("Task::memorize({gem}, {name}) ends Memorized with the gem filled and ready, as ${{Me.Gem}} and ${{Me.SpellReady}} see");
        report.print(mq, &label, Check::that(result == MemorizeResult::Memorized && theirs == id.to_string() && ready == "TRUE", || {
            format!("ended {result:?}; ${{Me.Gem}} {theirs}, ${{Me.SpellReady}} {ready} (memorize {name} again if it's left empty)")
        }));
    });
}

async fn forget(task: &Task<RustTest>, report: &mut Report, gem: usize, name: &str) {
    let asked = task.with(|mq| mq.forget_gem(gem));
    let emptied = task.wait_until_timeout(move |mq| mq.gem(gem).is_none(), TIMEOUT).await;
    task.with(|mq| {
        let theirs = mq.parse(&format!("${{Me.Gem[{}]}}", gem + 1));
        report.print(mq, &format!("forget_gem({gem}) forgets {name}, as ${{Me.Gem}} sees"), Check::that(asked && emptied && theirs == "NULL", || {
            format!("forget_gem: {asked}; gem {gem} has {:?}, ${{Me.Gem}} {theirs}", mq.gem(gem).map(|s| s.id()))
        }));
    });
}

fn item_at(mq: &Mq<RustTest>, at: ItemLocation) -> Option<(i32, i32)> {
    mq.inventory()?.slots().all().find(|(place, _)| *place == at).map(|(_, item)| (item.id(), item.stack()))
}

fn cursor(mq: &Mq<RustTest>) -> Option<(i32, i32)> {
    mq.inventory()?.cursor().map(|item| (item.id(), item.stack()))
}

fn count(mq: &Mq<RustTest>, id: i32) -> i32 {
    mq.inventory().map_or(0, |inventory| inventory.count(id))
}

/// A stack of two or more in a bag, preferring a closed bag: its place, ID
/// and stack.
fn stack_in_bag(mq: &Mq<RustTest>) -> Option<(ItemLocation, i32, i32)> {
    let inventory = mq.inventory()?;
    let stacks = || inventory.slots().all().filter(|(at, item)| at.in_bag.is_some() && item.is_stackable() && item.stack() >= 2);
    let closed = stacks().find(|(at, _)| mq.item_slot_window(SlotLocation::inventory(*at)).is_none());
    closed.or_else(|| stacks().next()).map(|(at, item)| (at, item.id(), item.stack()))
}

/// Splits one off a stack and drops it back on, then splits one off again and
/// puts it away with /autoinventory.
pub async fn items(task: &Task<RustTest>, report: &mut Report) {
    let found = task.with(|mq| {
        if !in_game(mq, report, "splitting a stack") {
            return None;
        }
        let skip = if cursor(mq).is_some() {
            Some("something is on the cursor")
        } else if mq.merchant().is_some() {
            Some("the merchant window is open (clicking selects there)")
        } else {
            None
        };
        let found = stack_in_bag(mq).filter(|_| skip.is_none());
        if found.is_none() {
            report.print(mq, "splitting a stack", Check::Skip(skip.unwrap_or("no stack of two or more in a bag").into()));
        }
        found.map(|found| (found, mq.item_slot_window(SlotLocation::inventory(found.0)).is_none()))
    });
    let Some(((at, id, stack), closed)) = found else { return };
    let place = SlotLocation::inventory(at);

    let split = task.with(|mq| mq.split_stack(place, 1));
    let parted = task.wait_until_timeout(move |mq| cursor(mq) == Some((id, 1)) && item_at(mq, at) == Some((id, stack - 1)), TIMEOUT).await;
    task.with(|mq| {
        let still_closed = !closed || mq.item_slot_window(place).is_none();
        let bag = if closed { ", in a closed bag it closes again" } else { "" };
        report.print(mq, &format!("split_stack({place}, 1) picks up one of {stack}{bag}"), Check::that(split && parted && still_closed, || {
            format!("split_stack: {split}; the cursor has {:?}, there {:?}; bag closed again: {still_closed}", cursor(mq), item_at(mq, at))
        }));
    });
    let dropped = task.with(|mq| cursor(mq).is_some() && mq.drop_item(place));
    let joined = task.wait_until_timeout(move |mq| cursor(mq).is_none() && item_at(mq, at) == Some((id, stack)), TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, &format!("drop_item({place}) joins it back on"), Check::that(dropped && joined, || {
            format!("drop_item: {dropped}; there {:?}, the cursor has {:?}", item_at(mq, at), cursor(mq))
        }));
    });
    if !joined {
        return;
    }

    let total = task.with(|mq| count(mq, id));
    let split = task.with(|mq| mq.split_stack(place, 1));
    let parted = task.wait_until_timeout(move |mq| cursor(mq) == Some((id, 1)), TIMEOUT).await;
    let put = task.with(|mq| split && parted && mq.auto_inventory());
    let away = task.wait_until_timeout(move |mq| cursor(mq).is_none() && count(mq, id) == total, TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, "auto_inventory() puts the cursor's item away", Check::that(put && away, || {
            format!("split again: {split} {parted}; auto_inventory: {put}; the cursor has {:?}, {} carried of {total}", cursor(mq), count(mq, id))
        }));
        // Whatever happened, nothing stays on the cursor.
        if cursor(mq).is_some() {
            mq.drop_item(place);
        }
    });
}

/// A clicky item that's safe to use (the user's pick).
const CLICKY: &str = "Little Buddy Confidence Booster 5000";

/// MacroQuest's reuse time left on the clicky, in seconds (0 when ready).
fn clicky_timer(mq: &Mq<RustTest>) -> i64 {
    mq.parse(&format!("${{FindItem[={CLICKY}].TimerReady}}")).parse().unwrap_or(0)
}

/// Uses the clicky as `/useitem` does: it starts casting its spell, or its
/// reuse timer starts.
pub async fn use_clicky(task: &Task<RustTest>, report: &mut Report) {
    const WHAT: &str = "using a clicky item";
    let found = task.with(|mq| {
        if !in_game(mq, report, WHAT) {
            return None;
        }
        let item = mq.inventory().and_then(|inventory| {
            inventory.slots().all().find_map(|(at, item)| {
                (item.name() == CLICKY).then(|| (at, item.effect(mq_emu::ItemEffect::Clicky).map(|effect| effect.spell_id())))
            })
        });
        let skip = if casting(mq).is_some() {
            Some("already casting".to_owned())
        } else if let Some(open) = NO_CASTING_WINDOWS.iter().find(|name| mq.window(name).is_some_and(|w| w.is_open())) {
            Some(format!("{open} is open"))
        } else if item.is_none() {
            Some(format!("no {CLICKY} in your inventory"))
        } else if clicky_timer(mq) > 0 {
            Some(format!("{CLICKY} isn't ready ({} s)", clicky_timer(mq)))
        } else {
            None
        };
        if let Some(why) = skip {
            report.print(mq, WHAT, Check::Skip(why));
            return None;
        }
        item
    });
    let Some((at, spell)) = found else { return };
    let place = SlotLocation::inventory(at);
    let used = task.with(|mq| mq.use_item(place));
    let took = task.wait_until_timeout(move |mq| (spell.is_some() && casting(mq) == spell) || clicky_timer(mq) > 0, TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, &format!("use_item({place}) uses {CLICKY} (casts its spell, or its timer starts)"), Check::that(used && took, || {
            format!("use_item: {used}; casting {:?} (its spell {spell:?}); ${{FindItem[={CLICKY}].TimerReady}} {}", casting(mq), clicky_timer(mq))
        }));
    });
}

/// Selects one of the merchant's items and one of yours, without buying or
/// selling.
pub async fn merchant(task: &Task<RustTest>, report: &mut Report) {
    let found = task.with(|mq| {
        let Some(merchant) = mq.merchant() else {
            report.print(mq, "selecting items at a merchant", Check::Skip("open a merchant's window to include".into()));
            return None;
        };
        let theirs = merchant.items().iter().next().map(|(slot, item)| (slot, item.id(), item.name()));
        let inventory = mq.inventory()?;
        let mine = inventory.slots().all().find(|(at, item)| at.in_bag.is_some() && !item.is_no_drop() && !item.is_container());
        let mine = mine.map(|(at, item)| (at, item.id(), item.name()));
        if theirs.is_none() || mine.is_none() {
            report.print(mq, "selecting items at a merchant", Check::Skip("the merchant has nothing, or you have nothing in a bag to sell".into()));
        }
        theirs.zip(mine)
    });
    let Some(((slot, their_id, their_name), (at, my_id, my_name))) = found else { return };
    let selected = |mq: &Mq<RustTest>| mq.merchant().and_then(|merchant| merchant.selected_item()).map(|item| item.id());

    let asked = task.with(|mq| mq.select_merchant_item(slot));
    let shown = task.wait_until_timeout(move |mq| selected(mq) == Some(their_id), TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, &format!("select_merchant_item({slot}) selects {their_name}"), Check::that(asked && shown, || {
            format!("select_merchant_item: {asked}; selected {:?}", selected(mq))
        }));
    });
    let place = SlotLocation::inventory(at);
    let asked = task.with(|mq| mq.select_item_to_sell(place));
    let shown = task.wait_until_timeout(move |mq| selected(mq) == Some(my_id), TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, &format!("select_item_to_sell({place}) selects {my_name}"), Check::that(asked && shown, || {
            format!("select_item_to_sell: {asked}; selected {:?}", selected(mq))
        }));
    });
}
