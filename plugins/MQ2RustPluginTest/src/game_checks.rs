//! Selftest checks for items, inventory, spells, buffs, the group, pets,
//! zones and ground items: mq-emu's reading of each against MacroQuest's
//! `${Me...}`, `${Spell[...]}`, `${Group...}`, `${Zone...}` and `${Ground}`.

use mq_emu::sys::constants::{NUM_BANK_SLOTS, NUM_LONG_BUFFS, NUM_TEMP_BUFFS};
use mq_emu::{GameState, InvSlot, Mq, Plugin, Spell};

use crate::checks::{Check, Report, same_text};

/// What MacroQuest shows for a missing value.
pub const NULL: &str = "NULL";

pub fn id_or_null(id: Option<impl ToString>) -> String {
    id.map_or_else(|| NULL.to_owned(), |id| id.to_string())
}

/// Your class's level for `spell` as `${Spell[id].Level}` shows it. When
/// your class can't use it, mq-emu says None, and MacroQuest shows whichever
/// of the game's "can't use" markers (255, 254, 127) the spell has.
fn level_text<P: Plugin>(mq: &Mq<P>, spell: &Spell<'_>) -> String {
    match mq.spell_level(spell) {
        Some(level) => level.to_string(),
        None => {
            let theirs = mq.parse(&format!("${{Spell[{}].Level}}", spell.id()));
            if ["255", "254", "127"].contains(&theirs.as_str()) { theirs } else { "a level mq-emu says you can't use it at".into() }
        }
    }
}

/// Compares every `(what, ours, path)`, naming the first few that differ.
pub fn all_match<P: Plugin>(mq: &Mq<P>, rows: impl IntoIterator<Item = (String, String, String)>) -> Check {
    let mut compared = 0;
    let mut wrong = Vec::new();
    for (what, ours, path) in rows {
        compared += 1;
        let theirs = mq.parse(&path);
        if ours != theirs {
            wrong.push(format!("{what}: mq-emu {ours:?}, {path} {theirs:?}"));
        }
    }
    match wrong.len() {
        0 if compared == 0 => Check::Skip("nothing to compare".into()),
        0 => Check::Pass,
        n => Check::Fail(format!("{n} of {compared} differ; {}", wrong.into_iter().take(3).collect::<Vec<_>>().join("; "))),
    }
}

pub fn run<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    if mq.game_state() != GameState::InGame {
        report.print(mq, "items, spells, group and zone", Check::Skip("needs to be in game".into()));
        return;
    }
    inventory(mq, report);
    spells(mq, report);
    group_and_pet(mq, report);
    zone(mq, report);
    ground(mq, report);
}

fn inventory<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let Some(inventory) = mq.inventory() else {
        report.print(mq, "inventory() in game", Check::Fail("no inventory".into()));
        return;
    };
    // ${Me.Inventory[n]} takes numbers up to the last bag; the cursor only by name.
    let slots = InvSlot::all().map(|slot| {
        let path = match slot {
            InvSlot::CURSOR => "${Me.Inventory[cursor].ID}".to_owned(),
            InvSlot(n) => format!("${{Me.Inventory[{n}].ID}}"),
        };
        (slot.to_string(), id_or_null(inventory.get(slot).map(|item| item.id())), path)
    });
    report.print(mq, "inventory slots match ${Me.Inventory[n].ID}", all_match(mq, slots));
    let names = InvSlot::all().filter_map(|slot| inventory.get(slot).map(|item| (slot, item))).map(|(slot, item)| {
        (slot.to_string(), item.name(), format!("${{Me.Inventory[{}].Name}}", slot.name().unwrap_or_default()))
    });
    report.print(mq, "item names match ${Me.Inventory[name].Name}", all_match(mq, names));

    let bag_rows: Vec<(String, String, String)> = inventory
        .bags()
        .filter(|(_, bag)| bag.is_container())
        .flat_map(|(slot, bag)| {
            let pack = slot.name().unwrap_or_default();
            (0..bag.contents().capacity()).map(move |i| {
                (format!("{pack} slot {}", i + 1), id_or_null(bag.contents().get(i).map(|item| item.id())), format!("${{Me.Inventory[{pack}].Item[{}].ID}}", i + 1))
            })
        })
        .collect();
    report.print(mq, "bag contents match ${Me.Inventory[packN].Item[n].ID}", all_match(mq, bag_rows));

    let stacks = inventory.slots().all().map(|(at, item)| {
        let path = match at.in_bag {
            None => format!("${{Me.Inventory[{}].Stack}}", InvSlot(at.slot as u8).name().unwrap_or_default()),
            Some(i) => format!("${{Me.Inventory[{}].Item[{}].Stack}}", InvSlot(at.slot as u8).name().unwrap_or_default(), i + 1),
        };
        (item.name(), item.stack().to_string(), path)
    });
    report.print(mq, "stack sizes match ${...Stack}", all_match(mq, stacks));

    match inventory.slots().all().find(|(at, item)| at.in_bag.is_some() && item.is_stackable()) {
        Some((_, item)) => {
            let id = item.id();
            report.print(mq, &format!("inventory.count({id}) = ${{FindItemCount[{id}]}}"), same_text(inventory.count(id), mq.parse(&format!("${{FindItemCount[{id}]}}"))));
        }
        None => report.print(mq, "inventory.count(id) = ${FindItemCount[id]}", Check::Skip("no stackable item in a bag".into())),
    }

    let bank = mq.bank().expect("in game");
    let rows = (0..NUM_BANK_SLOTS).map(|i| (format!("bank {}", i + 1), id_or_null(bank.get(i).map(|item| item.id())), format!("${{Me.Bank[{}].ID}}", i + 1)));
    report.print(mq, "bank slots match ${Me.Bank[n].ID}", all_match(mq, rows));
}

fn spells<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let buffs: Vec<Option<i32>> = (0..NUM_LONG_BUFFS).map(|slot| mq.buffs().find(|b| b.slot() == slot).map(|b| b.spell_id().0)).collect();
    let rows = buffs.iter().enumerate().map(|(slot, id)| (format!("buff {}", slot + 1), id_or_null(*id), format!("${{Me.Buff[{}].Spell.ID}}", slot + 1)));
    // A buff's own .ID is its slot number; the spell's is .Spell.ID.
    report.print(mq, "buffs match ${Me.Buff[n].Spell.ID}", all_match(mq, rows));
    let songs: Vec<Option<i32>> = (0..NUM_TEMP_BUFFS).map(|slot| mq.songs().find(|b| b.slot() == slot).map(|b| b.spell_id().0)).collect();
    let rows = songs.iter().enumerate().map(|(slot, id)| (format!("song {}", slot + 1), id_or_null(*id), format!("${{Me.Song[{}].Spell.ID}}", slot + 1)));
    report.print(mq, "songs match ${Me.Song[n].Spell.ID}", all_match(mq, rows));
    let levels = mq
        .buffs()
        .map(|b| (b.name(), b.level().to_string(), format!("${{Me.Buff[{}].Level}}", b.slot() + 1)))
        .chain(mq.songs().map(|b| (b.name(), b.level().to_string(), format!("${{Me.Song[{}].Level}}", b.slot() + 1))));
    report.print(mq, "buff caster levels match ${Me.Buff[n].Level}", all_match(mq, levels));

    let rows = (0..12).map(|gem| (format!("gem {}", gem + 1), id_or_null(mq.gem(gem).map(|s| s.id())), format!("${{Me.Gem[{}].ID}}", gem + 1)));
    report.print(mq, "gems match ${Me.Gem[n].ID}", all_match(mq, rows));

    let book: Vec<(usize, Spell<'_>)> = mq.spell_book().collect();
    let rows = book.iter().map(|(slot, spell)| (spell.name(), spell.id().to_string(), format!("${{Me.Book[{}].ID}}", slot + 1)));
    report.print(mq, &format!("spell book ({} spells) matches ${{Me.Book[n].ID}}", book.len()), all_match(mq, rows));

    // Spell details, for every spell you can see: gems, book, buffs.
    let mut seen: Vec<Spell<'_>> = mq.gems().map(|(_, s)| s).chain(book.iter().map(|(_, s)| *s)).chain(mq.buffs().filter_map(|b| b.spell())).collect();
    seen.sort_by_key(|s| s.id());
    seen.dedup_by_key(|s| s.id());
    let rows = seen.iter().flat_map(|spell| {
        let id = spell.id();
        [
            (format!("{id} name"), spell.name(), format!("${{Spell[{id}].Name}}")),
            (format!("{id} mana"), spell.mana_cost().to_string(), format!("${{Spell[{id}].Mana}}")),
            (format!("{id} level"), level_text(mq, spell), format!("${{Spell[{id}].Level}}")),
        ]
    });
    report.print(mq, &format!("{} spells' name, mana and level match ${{Spell[id]}}", seen.len()), all_match(mq, rows));
    let rows = seen.iter().take(10).map(|spell| {
        let name = spell.name();
        (name.clone(), id_or_null(mq.spell_by_name(&name).map(|s| s.id())), format!("${{Spell[{name}].ID}}"))
    });
    report.print(mq, "spell_by_name chooses as ${Spell[name].ID} does", all_match(mq, rows));
}

fn group_and_pet<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    match mq.group() {
        Some(group) => {
            // ${Group} and ${Group.Members} count the others.
            report.print(mq, "group.len() - 1 = ${Group.Members}", same_text(group.len() - 1, mq.parse("${Group.Members}")));
            // ${Group.Member[n]} counts filled slots only.
            let rows = group.members().enumerate().map(|(n, m)| (format!("member {n}"), m.name(), format!("${{Group.Member[{n}].Name}}")));
            report.print(mq, "group members match ${Group.Member[n].Name}", all_match(mq, rows));
            report.print(mq, "group.leader() = ${Group.Leader.Name}", same_text(id_or_null(group.leader().map(|m| m.name())), mq.parse("${Group.Leader.Name}")));
        }
        None => report.print(mq, "no group: ${Me.Grouped} is FALSE", same_text("FALSE", mq.parse("${Me.Grouped}"))),
    }
    let theirs = mq.parse("${Me.Pet.ID}");
    report.print(mq, "pet() = ${Me.Pet.ID}", match mq.pet() {
        Some(pet) => same_text(pet.id(), theirs),
        None => Check::that(theirs == NULL || theirs == "0", || format!("no pet in mq-emu, MacroQuest says {theirs:?}")),
    });
    if let Some(pet) = mq.pet() {
        report.print(mq, "master_of(pet) is me", Check::that(mq.master_of(&pet).map(|m| m.id()) == mq.me().map(|m| m.id()), || format!("{:?}", pet.master_id())));
    }
}

fn zone<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let Some(zone) = mq.zone() else {
        report.print(mq, "zone() in game", Check::Fail("no zone".into()));
        return;
    };
    report.print(mq, "zone().id() = ${Zone.ID}", same_text(id_or_null(zone.id()), mq.parse("${Zone.ID}")));
    report.print(mq, "zone().short_name() = ${Zone.ShortName}", same_text(zone.short_name(), mq.parse("${Zone.ShortName}")));
    report.print(mq, "zone().name() = ${Zone.Name}", same_text(zone.name(), mq.parse("${Zone.Name}")));
    report.print(mq, "zone().zone_type().is_indoor() = ${Zone.Indoor}", same_text(if zone.zone_type().is_indoor() { "TRUE" } else { "FALSE" }, mq.parse("${Zone.Indoor}")));
    report.print(mq, "zone().info() is in the zone list", Check::that(zone.info(mq).is_some(), || "it isn't".into()));
    // ${Zone[id]} of the zone you're in reads the current zone (whose long
    // name can differ, "Nexus" vs the list's "The Nexus"), so compare others.
    let rows = mq.zones().filter(|z| Some(z.id()) != zone.id()).take(20).map(|z| {
        let id = z.id();
        (format!("zone {id}"), format!("{} {}", z.short_name(), z.name()), format!("${{Zone[{id}].ShortName}} ${{Zone[{id}].Name}}"))
    });
    report.print(mq, "zone list names match ${Zone[id]} (other zones)", all_match(mq, rows));
    let by_name = mq.zone_by_name(&zone.short_name()).map(|z| z.id());
    report.print(mq, "zone_by_name(short name) finds this zone", Check::that(by_name == zone.id(), || format!("{by_name:?}")));
}

fn ground<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let count = mq.ground_items().count();
    report.print(mq, "ground_items().count() = ${GroundItemCount}", same_text(count, mq.parse("${GroundItemCount}")));
    // ${Ground[n]} is the nth nearest, so compare the sets.
    let mut ours: Vec<i32> = mq.ground_items().map(|g| g.id()).collect();
    let mut theirs: Vec<i32> = (1..=count.min(100)).filter_map(|n| mq.parse(&format!("${{Ground[{n}].ID}}")).parse().ok()).collect();
    ours.sort_unstable();
    theirs.sort_unstable();
    report.print(mq, "ground item IDs match ${Ground[n].ID}", match count {
        0 => Check::Skip("nothing on the ground in this zone".into()),
        101.. => Check::Skip(format!("{count} items; only checks up to 100")),
        _ => Check::that(ours == theirs, || format!("mq-emu {ours:?}, MacroQuest {theirs:?}")),
    });
}
