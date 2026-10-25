//! Inventory, spells, buffs, group, zone, ground items and pets through `Mq`,
//! against fake game objects: what each finds, and that missing game data
//! reads as nothing rather than crashing.

use mq_emu_sys::layouts::{
    CGroup, CGroupMember, ClientSpellManager, EQGroundItem, EQGroundItemListManager, EQWorldData, EQZoneInfo, PcClient,
    PcProfile, ProfileList, VePointer_ItemClient, zoneHeader,
};

use super::*;
use crate::inventory::InvSlot;
use crate::items::tests::{Fake, FakeItem, fill, set_name};
use crate::spells::SpellId;
use crate::spells::tests::fake_spell;
use crate::zone::ZoneId;

/// A character in a fake game; leaked so `'static` hooks can use it.
struct Game {
    pc: Fake<PcClient>,
    list: Fake<ProfileList>,
    profile: Fake<PcProfile>,
    spells: Fake<ClientSpellManager>,
    known: Vec<Fake<mq_emu_sys::layouts::EQ_Spell>>,
    items: Vec<FakeItem>,
    inventory_slots: Vec<VePointer_ItemClient>,
}

fn game() -> &'static mut Game {
    let game = Box::leak(Box::new(Game {
        pc: Fake::new(),
        list: Fake::new(),
        profile: Fake::new(),
        spells: Fake::new(),
        known: Vec::new(),
        items: Vec::new(),
        inventory_slots: Vec::new(),
    }));
    game.list.get().set_pFirst(game.profile.ptr());
    game.pc.get().ProfileManager().set_pFirst(game.list.ptr());
    for gem in 0..mq_emu_sys::constants::MAX_MEMORIZED_SPELLS {
        game.profile.get().set_MemorizedSpells_at(gem, -1);
    }
    for slot in 0..mq_emu_sys::constants::NUM_BOOK_SLOTS {
        game.profile.get().set_SpellBook_at(slot, -1);
    }
    LOCAL_PC.with(|p| p.set(game.pc.ptr()));
    SPELL_MANAGER.with(|p| p.set(game.spells.ptr()));
    game
}

impl Game {
    fn learn(&mut self, id: i32, name: &str) {
        let spell = fake_spell(id, name);
        self.spells.get().set_Spells_at(id as usize, spell.ptr());
        self.known.push(spell);
    }
}

#[test]
fn without_game_data_everything_is_empty() {
    let rt = in_pulse(|plugin, mq| {
        plugin.log.push(format!(
            "{} {} {} {} {} {} {}",
            mq.inventory().is_none(),
            mq.bank().is_none(),
            mq.buffs().count() + mq.songs().count() + mq.gems().count() + mq.spell_book().count(),
            mq.spell(SpellId(1)).is_none(),
            mq.zone().is_none() && mq.zone_info(ZoneId(1)).is_none() && mq.zones().count() == 0,
            mq.group().is_none(),
            mq.ground_items().count(),
        ));
    });
    assert_eq!(log_of(rt)[2..], ["true true 0 true true true 0"]);
}

#[test]
fn inventory_bank_and_items() {
    let game = game();
    let sword = FakeItem::new(1001, "Rusty Sword");
    let ration = FakeItem::new(13005, "Ration").stack(12, 20);
    let bag = FakeItem::new(17005, "Backpack").bag(8, &[(2, &ration)]);
    game.items.extend([sword, ration, bag]);
    let slots = &mut game.inventory_slots;
    fill(game.profile.get().InventoryContainer(), slots, 34, &[(13, &game.items[0]), (23, &game.items[2])]);

    let rt = in_pulse(|plugin, mq| {
        let inventory = mq.inventory().unwrap();
        plugin.log.push(format!("{:?}", inventory.get(InvSlot::PRIMARY).map(|i| i.name())));
        plugin.log.push(format!("{:?}", inventory.worn().map(|(slot, _)| slot.to_string()).collect::<Vec<_>>()));
        plugin.log.push(format!("{:?}", inventory.bags().map(|(slot, bag)| (slot.to_string(), bag.name())).collect::<Vec<_>>()));
        let (at, _) = inventory.find_by_name("ration").unwrap();
        plugin.log.push(format!("{at:?} {} {} {:?}", inventory.count(13005), inventory.free_bag_slots(), inventory.cursor()));
        plugin.log.push(format!("{}", mq.bank().unwrap().all().count()));
    });
    assert_eq!(
        log_of(rt)[2..],
        [
            "Some(\"Rusty Sword\")",
            "[\"mainhand\"]",
            "[(\"pack1\", \"Backpack\")]",
            "ItemLocation { slot: 23, in_bag: Some(2) } 12 7 None",
            "0",
        ]
    );
}

#[test]
fn spells_buffs_gems_and_book() {
    let game = game();
    game.learn(1200, "Courage");
    game.learn(717, "Selo's Accelerando");
    game.profile.get().set_Class(2);
    game.known[0].get().set_ClassLevel_at(2, 1);
    let profile = game.profile.get();
    profile.Buffs(0).set_SpellID(1200);
    profile.Buffs(0).set_Level(5);
    profile.Buffs(3).set_SpellID(9999); // a spell the client doesn't know
    profile.Buffs(mq_emu_sys::constants::NUM_LONG_BUFFS + 1).set_SpellID(717);
    profile.set_MemorizedSpells_at(2, 1200);
    profile.set_SpellBook_at(9, 1200);

    let rt = in_pulse(|plugin, mq| {
        let buffs: Vec<(usize, String, u8)> = mq.buffs().map(|b| (b.slot(), b.name(), b.level())).collect();
        let songs: Vec<(usize, String)> = mq.songs().map(|b| (b.slot(), b.name())).collect();
        plugin.log.push(format!("{buffs:?} {songs:?} {:?}", mq.buff(SpellId(717)).map(|b| b.slot())));
        let gems: Vec<(usize, String)> = mq.gems().map(|(gem, s)| (gem, s.name())).collect();
        let book: Vec<(usize, String)> = mq.spell_book().map(|(slot, s)| (slot, s.name())).collect();
        plugin.log.push(format!("{gems:?} {book:?} {} {}", mq.knows_spell(SpellId(1200)), mq.knows_spell(SpellId(717))));
        let courage = mq.spell_by_name("COURAGE").unwrap();
        plugin.log.push(format!("{} {:?} {:?}", courage.id(), mq.spell_level(&courage), mq.gem(99).map(|s| s.id())));
    });
    assert_eq!(
        log_of(rt)[2..],
        [
            "[(0, \"Courage\", 5), (3, \"\", 0)] [(1, \"Selo's Accelerando\")] Some(1)",
            "[(2, \"Courage\")] [(9, \"Courage\")] true false",
            "1200 Some(1) None",
        ]
    );
}

#[test]
fn group_members_and_their_spawns() {
    let game = game();
    let group = Box::leak(Box::new(Fake::<CGroup>::new()));
    let (me, bob) = (Box::leak(Box::new(Fake::<CGroupMember>::new())), Box::leak(Box::new(Fake::<CGroupMember>::new())));
    let names = Box::leak(Box::new([crate::text::tests::cstrrep(0, 2, b"Me"), crate::text::tests::cstrrep(0, 3, b"Bob")]));
    me.get().Name().set_m_data(names[0].as_ptr().cast_mut().cast());
    bob.get().Name().set_m_data(names[1].as_ptr().cast_mut().cast());
    me.get().set_pPlayer(PLAYER.with(FakeObject::ptr));
    group.get().set_m_groupMembers_at(0, me.ptr());
    group.get().set_m_groupMembers_at(1, bob.ptr());
    game.pc.get().set_Group(group.ptr());
    player().set_SpawnID(42);

    let rt = in_pulse(|plugin, mq| {
        let group = mq.group().unwrap();
        let members: Vec<(String, Option<u32>)> = group.members().map(|m| (m.name(), m.spawn().map(|s| s.id().0))).collect();
        plugin.log.push(format!("{members:?}"));
    });
    assert_eq!(log_of(rt)[2..], ["[(\"Me\", Some(42)), (\"Bob\", None)]"]);
}

#[test]
fn current_zone_and_zone_list() {
    let game = game();
    game.pc.get().set_currentZoneId(0x0001_0002);
    let header = Box::leak(Box::new(Fake::<zoneHeader>::new()));
    header.get().set_ShortName(set_name("qeynos2"));
    header.get().set_LongName(set_name("North Qeynos"));
    header.get().set_ZoneType(mq_emu_sys::layouts::EOutDoor::OutdoorCity);
    let world = Box::leak(Box::new(Fake::<EQWorldData>::new()));
    let info = Box::leak(Box::new(Fake::<EQZoneInfo>::new()));
    info.get().set_Id(2);
    info.get().set_ShortName(set_name("qeynos2"));
    info.get().set_LongName(set_name("North Qeynos"));
    world.get().set_ZoneArray_at(2, info.ptr());
    ZONE_INFO.with(|p| p.set(header.ptr()));
    WORLD_DATA.with(|p| p.set(world.ptr()));

    let rt = in_pulse(|plugin, mq| {
        let zone = mq.zone().unwrap();
        plugin.log.push(format!("{:?} {} {} {:?}", zone.id(), zone.instance(), zone.name(), zone.zone_type()));
        plugin.log.push(format!(
            "{:?} {:?} {:?}",
            zone.info(mq).map(|z| z.short_name()),
            mq.zone_by_name("north qeynos").map(|z| z.id()),
            mq.zone_info(ZoneId(999)).map(|z| z.id()),
        ));
    });
    assert_eq!(log_of(rt)[2..], ["Some(ZoneId(2)) 1 North Qeynos OutdoorCity", "Some(\"qeynos2\") Some(ZoneId(2)) None"]);
}

#[test]
fn ground_items_and_the_nearest_one() {
    let manager = Box::leak(Box::new(Fake::<EQGroundItemListManager>::new()));
    let (far, near) = (Box::leak(Box::new(Fake::<EQGroundItem>::new())), Box::leak(Box::new(Fake::<EQGroundItem>::new())));
    far.get().set_DropID(1);
    far.get().set_X(100.0);
    far.get().set_pNext(near.ptr());
    near.get().set_DropID(2);
    near.get().set_X(3.0);
    manager.get().set_Top(far.ptr());
    GROUND_ITEMS.with(|p| p.set(manager.ptr()));
    set_zone((1, 0.0, 0.0), &[]);

    let rt = in_pulse(|plugin, mq| {
        plugin.log.push(format!(
            "{:?} {:?} {:?}",
            mq.ground_items().map(|g| g.id()).collect::<Vec<_>>(),
            mq.nearest_ground_item(|_| true).map(|g| g.id()),
            mq.ground_item(7).map(|g| g.id()),
        ));
    });
    assert_eq!(log_of(rt)[2..], ["[1, 2] Some(2) None"]);
}

#[test]
fn pets_and_their_masters() {
    set_zone((1, 0.0, 0.0), &[(20, "Gabober", 1, 5.0, 0.0), (21, "Kebekn", 1, 9.0, 0.0)]);
    player().set_PetID(20);
    ZONE.with(|zone| unsafe { (*zone.borrow()[0].ptr()).set_MasterID(1) });

    let rt = in_pulse(|plugin, mq| {
        let pet = mq.pet().unwrap();
        let other = mq.spawn(SpawnId(21)).unwrap();
        plugin.log.push(format!(
            "{} {:?} {} {:?}",
            pet.name(),
            mq.master_of(&pet).map(|s| s.id()),
            other.is_pet(),
            mq.pet_of(&other).map(|s| s.id()),
        ));
    });
    assert_eq!(log_of(rt)[2..], ["Gabober Some(SpawnId(1)) false None"]);
}
