//! Selftest checks for acting on spells and items: a spell cast and stopped,
//! a spell memorized and forgotten (leaving the gems as they were), a stack
//! split and its part put back by hand and with /autoinventory, and items
//! selected in an open merchant window (nothing is bought or sold).

use std::time::Duration;

use mq_emu::{GameState, ItemLocation, Mq, SlotLocation, SpellId, Task};

use crate::RustTest;
use crate::checks::{Check, Report};

/// How long a cast, a stop or an item move may take to show.
const TIMEOUT: Duration = Duration::from_secs(3);
/// How long memorizing one spell may take.
const MEMORIZE_TIMEOUT: Duration = Duration::from_secs(30);
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
        let chosen = mq.gems().find(|(gem, spell)| {
            *gem < GEMS
                && spell.is_beneficial()
                && [TT_SELF, TT_GROUP_V1, TT_GROUP_V2].contains(&spell.target_type())
                && spell.cast_time_ms() >= 1000
                && spell.mana_cost() <= mana
                && mq.gem_ready(*gem)
        });
        let chosen = chosen.filter(|_| skip.is_none()).map(|(gem, spell)| (gem, spell.id(), spell.name()));
        if chosen.is_none() {
            let why = skip.unwrap_or_else(|| "no ready gem with a self or group buff (cast time 1s or more) you have the mana for".into());
            report.print(mq, "casting a spell and stopping it", Check::Skip(why));
        }
        chosen
    });
    let Some((gem, id, name)) = chosen else { return };

    let cast = task.with(|mq| mq.cast_gem(gem));
    let started = task.wait_until_timeout(move |mq| casting(mq) == Some(id), TIMEOUT).await;
    task.with(|mq| {
        let theirs = mq.parse("${Me.Casting.ID}");
        let label = format!("cast_gem({gem}) casts {name}, as Spawn::casting and ${{Me.Casting.ID}} see");
        report.print(mq, &label, Check::that(cast && started && theirs == id.to_string(), || {
            format!("cast_gem: {cast}; casting {:?}, ${{Me.Casting.ID}} {theirs}", casting(mq))
        }));
    });
    let stopped = task.with(|mq| mq.stop_casting());
    let done = task.wait_until_timeout(|mq| casting(mq).is_none(), TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, "stop_casting() stops it", Check::that(stopped && done, || format!("stop_casting: {stopped}; casting {:?}", casting(mq))));
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
    let asked = task.with(|mq| mq.memorize_spell(gem, id));
    let filled = task.wait_until_timeout(move |mq| mq.gem(gem).map(|spell| spell.id()) == Some(id), MEMORIZE_TIMEOUT).await;
    task.with(|mq| {
        let theirs = mq.parse(&format!("${{Me.Gem[{}].ID}}", gem + 1));
        report.print(mq, &format!("memorize_spell({gem}, {name}) fills the gem, as ${{Me.Gem}} sees"), Check::that(asked && filled && theirs == id.to_string(), || {
            format!("memorize_spell: {asked}; gem {gem} has {:?}, ${{Me.Gem}} {theirs} (memorize {name} again if it's left empty)", mq.gem(gem).map(|s| s.id()))
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
