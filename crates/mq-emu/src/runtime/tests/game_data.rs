//! Inventory, spells, buffs, group, zone, ground items, pets, other spawns'
//! buffs, raids, mercenaries, extended targets, alternate abilities, skills,
//! the merchant and loot windows, tasks, friends, tribute and achievements
//! through `Mq`, against fake game objects: what each finds, and that missing
//! game data reads as nothing rather than crashing.

use mq_emu_sys::constants::{DB_STRING_ALT_ABILITY_NAME, MAX_TOTAL_BUFFS};
use mq_emu_sys::game::{SKILL_ACTIVATABLE, SKILL_HAS, mqemu_cached_buff};
use mq_emu_sys::layouts::{
    AchievementManager, CAltAbilityData, CGroup, CGroupMember, CLootWnd, CMercenaryManager, CMerchantWnd, CPetInfoWnd, CRaid, CTargetWnd, CTaskManager,
    ClientSpellManager, EQGroundItem, EQGroundItemListManager, EQWorldData, EQZoneInfo, ExtendedTargetList, ExtendedTargetSlot,
    ItemContainerInstance, MerchantPageHandler, PcClient, PcProfile, ProfileList, SharedTaskPlayerInfo, TaskSystemType, VePointer_ItemClient, eMercenaryState, eRaidState, zoneHeader,
};

use super::*;
use crate::game_fns::fake::{self as fake_game, FakeSkill};
use crate::inventory::InvSlot;
use crate::windows::fake as fake_windows;
use crate::windows::{ItemContainerKind, SlotLocation};
use crate::items::tests::{Fake, FakeItem, fill, set_name};
use crate::skills::SkillId;
use crate::xtarget::XTARGET_AUTO_HATER;
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

/// Runs one pulse with a fresh runtime and returns what `log` wrote.
fn pulse_log(log: impl Fn(&Mq<Recorder>) -> String + 'static) -> Vec<String> {
    log_of(in_pulse(move |plugin, mq| plugin.log.push(log(mq))))[2..].to_vec()
}

#[test]
fn without_more_game_data_everything_is_empty() {
    let log = pulse_log(|mq| {
        let me = mq.me().unwrap();
        format!(
            "{} {} {} {} {} {} {} {} {}",
            mq.target_buffs().count() + mq.pet_buffs().count() + mq.cached_buffs(&me).len(),
            mq.target_buffs_populated(),
            mq.raid().is_none() && !mq.raid_invite_pending(),
            mq.mercenary().is_none() && mq.mercenary_spawn().is_none(),
            mq.xtargets().count() + mq.haters().count(),
            mq.alt_abilities().count() as i32 + mq.alt_ability_points() + mq.alt_ability_points_spent(),
            mq.alt_ability_by_index(-1).is_none() && mq.alt_ability_by_index(i32::MAX).is_none(),
            mq.skill(SkillId(0)).is_none() && mq.skills().count() == 0,
            mq.merchant().is_none() && mq.loot_window().is_none(),
        )
    });
    assert_eq!(log, ["0 false true true 0 0 true true true"]);
}

#[test]
fn target_buffs_need_a_target_and_pet_buffs_a_pet() {
    let target = Box::leak(Box::new(Fake::<CTargetWnd>::new()));
    let pet = Box::leak(Box::new(Fake::<CPetInfoWnd>::new()));
    target.get().set_BuffSpellID_at(2, 1200);
    pet.get().set_Buff_at(0, 2510);
    pet.get().set_PetBuffTimer_at(0, u32::MAX);
    set_windows(|w| {
        w.target_window = target.ptr();
        w.pet_window = pet.ptr();
        w.target_buffs_populated = true;
    });
    set_zone((1, 0.0, 0.0), &[(20, "Gabober", 1, 5.0, 0.0)]);
    let log = |mq: &Mq<Recorder>| {
        format!(
            "{:?} {} {:?}",
            mq.target_buffs().map(|b| (b.slot(), b.spell_id().0)).collect::<Vec<_>>(),
            mq.target_buffs_populated(),
            mq.pet_buffs().map(|b| (b.spell_id().0, b.remaining())).collect::<Vec<_>>(),
        )
    };

    assert_eq!(pulse_log(log), ["[] false []"], "no target, no pet: the windows' leftovers don't count");
    HAS_TARGET.with(|t| t.set(true));
    player().set_PetID(20);
    let log = pulse_log(log);
    HAS_TARGET.with(|t| t.set(false));
    assert_eq!(log, ["[(2, 1200)] true [(2510, None)]"], "a pet's u32::MAX timer is a permanent buff");
}

#[test]
fn cached_buffs_come_from_macroquest_in_its_order() {
    set_zone((1, 0.0, 0.0), &[(20, "a rat", 1, 5.0, 0.0)]);
    let rat = ZONE.with(|zone| zone.borrow()[0].ptr());
    let buff = |slot, spell_id| mqemu_cached_buff { slot, spell_id, duration: 5, count: 0, remaining_ms: 30_000, age_ms: 0, caster: [0; 64] };
    // More than any first guess at the size: the call is repeated with room for all.
    let many: Vec<mqemu_cached_buff> = (0..MAX_TOTAL_BUFFS as i32 + 3).map(|i| buff(i, 1000 + i)).collect();
    fake_game::with(|game| game.cached_buffs.insert(rat as usize, many));

    let log = pulse_log(|mq| {
        let buffs = mq.cached_buffs(&mq.spawn(SpawnId(20)).unwrap());
        let mine = mq.cached_buffs(&mq.me().unwrap()).len();
        format!("{} {:?} {:?} {mine}", buffs.len(), buffs.first().map(|b| b.spell_id().0), buffs.last().map(|b| b.slot()))
    });
    assert_eq!(log, [format!("{} Some(1000) Some({}) 0", MAX_TOTAL_BUFFS + 3, MAX_TOTAL_BUFFS + 2)]);
}

#[test]
fn a_raid_is_only_there_once_joined() {
    let raid = Box::leak(Box::new(Fake::<CRaid>::new()));
    raid.get().set_locations_at(4, 1);
    raid.get().raidMembers(4).set_Name(set_name("Ann"));
    set_windows(|w| w.raid = raid.ptr());
    let log = |mq: &Mq<Recorder>| format!("{:?} {}", mq.raid().map(|r| r.members().map(|m| m.name()).collect::<Vec<_>>()), mq.raid_invite_pending());

    raid.get().set_raidState(eRaidState(4));
    assert_eq!(pulse_log(log), ["None false"], "no members: not in a raid, whatever the state says");
    raid.get().set_raidState(eRaidState(2));
    assert_eq!(pulse_log(log), ["None true"], "invited");
    // On emulator servers the state stays "not in a raid" (1) while you're in one.
    raid.get().set_raidState(eRaidState(1));
    raid.get().set_RaidMemberCount(1);
    assert_eq!(pulse_log(log), ["Some([\"Ann\"]) false"]);
}

#[test]
fn a_mercenary_is_there_out_or_suspended() {
    let manager = Box::leak(Box::new(Fake::<CMercenaryManager>::new()));
    manager.get().set_currMercenaryIndex(-1);
    set_windows(|w| w.mercenary_manager = manager.ptr());
    set_zone((1, 0.0, 0.0), &[(30, "Fred", 1, 5.0, 0.0)]);
    let log = |mq: &Mq<Recorder>| format!("{:?} {:?}", mq.mercenary().map(|m| m.state()), mq.mercenary_spawn().map(|s| s.id().0));

    assert_eq!(pulse_log(log), ["None None"], "none");
    manager.get().set_currMercenaryIndex(0);
    assert_eq!(pulse_log(log), ["Some(Suspended) None"]);
    manager.get().currentMercenary().set_hasMercenary(true);
    manager.get().currentMercenary().set_suspendedState(eMercenaryState(5));
    manager.get().set_mercenarySpawnId(30);
    assert_eq!(pulse_log(log), ["Some(Active) Some(30)"]);
}

#[test]
fn extended_targets_and_haters() {
    let game = game();
    let list = Box::leak(Box::new(Fake::<ExtendedTargetList>::new()));
    // The game's slots are one contiguous array.
    let block: &mut Vec<u8> = Box::leak(Box::new(vec![0u8; size_of::<ExtendedTargetSlot>() * 3]));
    let array = block.as_mut_ptr().cast::<ExtendedTargetSlot>();
    let slot = |i: usize| unsafe { &*array.add(i) };
    slot(0).set_xTargetType(XTARGET_AUTO_HATER);
    slot(0).set_SpawnID(20);
    slot(1).set_xTargetType(2);
    slot(1).set_SpawnID(99);
    slot(2).set_xTargetType(XTARGET_AUTO_HATER);
    list.get().m_targetSlots().set_m_array(array);
    list.get().m_targetSlots().set_m_length(3);
    game.pc.get().set_pExtendedTargetList(list.ptr());
    set_zone((1, 0.0, 0.0), &[(20, "a rat", 1, 5.0, 0.0)]);

    let log = pulse_log(|mq| {
        let roles: Vec<u32> = mq.xtargets().map(|x| x.role()).collect();
        let haters: Vec<u32> = mq.haters().map(|s| s.id().0).collect();
        format!("{roles:?} {haters:?} {:?}", mq.xtarget(1).and_then(|x| x.spawn_id()))
    });
    assert_eq!(log, ["[1, 2, 1] [20] Some(SpawnId(99))"], "an empty auto hater slot isn't a hater");
}

#[test]
fn alt_abilities_come_from_the_profile_list() {
    let game = game();
    let (rank1, rank3) = (Box::leak(Box::new(Fake::<CAltAbilityData>::new())), Box::leak(Box::new(Fake::<CAltAbilityData>::new())));
    rank1.get().set_GroupID(35);
    rank1.get().set_nName(1);
    rank3.get().set_GroupID(36);
    rank3.get().set_nName(2);
    fake_game::with(|g| {
        g.alt_abilities.insert(500, rank1.ptr());
        g.alt_abilities.insert(612, rank3.ptr());
        g.strings.insert((1, DB_STRING_ALT_ABILITY_NAME), "Innate Run Speed".into());
        g.strings.insert((2, DB_STRING_ALT_ABILITY_NAME), "Mass Group Buff".into());
    });
    let profile = game.profile.get();
    profile.AAList(0).set_AAIndex(500);
    profile.AAList(7).set_AAIndex(612);
    profile.AAList(8).set_AAIndex(999); // the game doesn't know it
    profile.set_AAPoints(4);
    profile.set_AAPointsSpent(21);

    let log = pulse_log(|mq| {
        let ids: Vec<i32> = mq.alt_abilities().map(|aa| aa.group_id()).collect();
        format!(
            "{ids:?} {:?} {:?} {:?} {} {}",
            mq.alt_ability(36).map(|aa| aa.name()),
            mq.alt_ability_by_name("innate run speed").map(|aa| aa.group_id()),
            mq.alt_ability_by_index(612).map(|aa| aa.group_id()),
            mq.alt_ability_points(),
            mq.alt_ability_points_spent(),
        )
    });
    assert_eq!(log, ["[35, 36] Some(\"Mass Group Buff\") Some(35) Some(36) 4 21"]);
}

#[test]
fn skills_are_the_ones_your_class_has() {
    let game = game();
    game.profile.get().set_Skill_at(40, 50);
    game.profile.get().set_Skill_at(10, 200);
    fake_game::with(|g| {
        g.skills.insert(40, FakeSkill { name: "Sneak".into(), values: [55, 50, 75], flags: SKILL_HAS | SKILL_ACTIVATABLE, timer: (0, 0) });
        g.skills.insert(10, FakeSkill { name: "Defense".into(), values: [210, 200, 210], flags: SKILL_HAS, timer: (0, 0) });
        g.skills.insert(11, FakeSkill { name: "Divination".into(), ..FakeSkill::default() });
    });

    let log = pulse_log(|mq| {
        let skills: Vec<(i32, i32)> = mq.skills().map(|s| (s.id().0, s.value())).collect();
        let abilities: Vec<String> = mq.abilities().map(|s| s.name()).collect();
        format!(
            "{skills:?} {abilities:?} {:?} {:?}",
            mq.skill_by_name("DIVINATION").map(|s| (s.id().0, s.is_known())),
            mq.skill(SkillId(100)).map(|s| s.id()),
        )
    });
    assert_eq!(log, ["[(10, 210), (40, 55)] [\"Sneak\"] Some((11, false)) None"]);
}

#[test]
fn merchant_and_loot_windows_need_their_spawn() {
    let merchant = Box::leak(Box::new(Fake::<CMerchantWnd>::new()));
    let loot = Box::leak(Box::new(Fake::<CLootWnd>::new()));
    set_zone((1, 0.0, 0.0), &[(40, "Merchant Jo", 1, 5.0, 0.0), (41, "a rat's corpse", 2, 5.0, 0.0)]);
    let (jo, corpse) = ZONE.with(|zone| (zone.borrow()[0].ptr(), zone.borrow()[1].ptr()));
    set_windows(|w| {
        w.merchant_window = merchant.ptr();
        w.loot_window = loot.ptr();
    });
    let log = |mq: &Mq<Recorder>| format!("{:?} {:?}", mq.merchant().map(|m| m.spawn().id().0), mq.loot_window().map(|l| l.corpse().id().0));

    assert_eq!(pulse_log(log), ["None None"], "no merchant or corpse");
    set_windows(|w| {
        w.active_merchant = jo;
        w.active_corpse = corpse;
    });
    assert_eq!(pulse_log(log), ["None Some(41)"], "the merchant window is hidden");
    merchant.get().set_dShow(true);
    assert_eq!(pulse_log(log), ["Some(40) Some(41)"]);
}

/// What `memorize_spells` hands the game for these gems: every gem, -1 for
/// the ones left as they are.
fn loadout(gems: &[(usize, i32)]) -> String {
    let mut spells = [-1; mq_emu_sys::constants::NUM_SPELL_GEMS];
    for &(gem, id) in gems {
        spells[gem] = id;
    }
    format!("memorize {spells:?}")
}

#[test]
fn spells_are_cast_memorized_and_forgotten() {
    let game = game();
    game.learn(1200, "Courage");
    game.learn(717, "Selo's Accelerando");
    game.learn(2000, "Heroism");
    let profile = game.profile.get();
    profile.set_Class(2);
    profile.set_Level(10);
    game.known[0].get().set_ClassLevel_at(2, 1);
    game.known[2].get().set_ClassLevel_at(2, 50);
    profile.set_MemorizedSpells_at(2, 1200);
    profile.set_MemorizedSpells_at(12, 717); // past the 12 gems spells are cast from
    profile.set_SpellBook_at(0, 1200);
    profile.set_SpellBook_at(1, 2000);
    fake_game::with(|g| {
        g.commands = vec!["/cast".into(), "/stopcast".into()];
        g.spells_work = true;
        g.ready_gems = vec![2];
    });

    let log = pulse_log(|mq| {
        let cast = [mq.cast_gem(2), mq.cast_gem(0), mq.cast_gem(12), mq.cast_spell(SpellId(1200)), mq.cast_spell(SpellId(717)), mq.stop_casting()];
        let ready = [mq.gem_ready(2), mq.gem_ready(3), mq.gem_ready(usize::MAX)];
        let memorized = [
            mq.memorize_spell(4, SpellId(1200)),
            mq.memorize_spells(&[(0, SpellId(1200)), (11, SpellId(1200))]),
            mq.memorize_spell(4, SpellId(717)),
            mq.memorize_spell(4, SpellId(2000)),
            mq.memorize_spell(12, SpellId(1200)),
            mq.memorize_spells(&[(5, SpellId(1200)), (6, SpellId(717))]),
            mq.memorize_spells(&[]),
        ];
        let forgot = [mq.forget_gem(2), mq.forget_gem(0), mq.forget_gem(12)];
        format!("{cast:?} {ready:?} {memorized:?} {forgot:?}")
    });
    assert_eq!(
        log,
        ["[true, false, false, true, false, true] [true, false, false] [true, true, false, false, false, false, false] [true, false, false]"],
        "empty gems, gems past the 12, spells not in the book or above your level, and a set with one of those aren't passed on"
    );
    assert_eq!(fake_game::log(), ["/cast 3".to_owned(), "/cast 3".into(), "/stopcast".into(), loadout(&[(4, 1200)]), loadout(&[(0, 1200), (11, 1200)]), "forget 2".into()]);

    fake_game::with(|g| {
        g.commands.clear();
        g.spells_work = false;
    });
    let log = pulse_log(|mq| format!("{:?}", [mq.cast_gem(2), mq.stop_casting(), mq.memorize_spell(4, SpellId(1200)), mq.forget_gem(2)]));
    assert_eq!(log, ["[false, false, false, false]"], "the game's answer");
    assert_eq!(fake_game::log(), [loadout(&[(4, 1200)]), "forget 2".into()]);
}

#[test]
fn what_a_spawn_is_casting() {
    let log = |mq: &Mq<Recorder>| format!("{:?}", mq.me().and_then(|me| me.casting()));
    player().CastingData().set_SpellID(1200);
    assert_eq!(pulse_log(log), ["Some(SpellId(1200))"]);
    player().CastingData().set_SpellID(-1);
    assert_eq!(pulse_log(log), ["None"], "not casting");
}

#[test]
fn stacks_are_split_and_put_away() {
    let game = game();
    let ration = FakeItem::new(13005, "Ration").stack(12, 20);
    fill(game.profile.get().InventoryContainer(), &mut game.inventory_slots, 34, &[]);
    fake_windows::with(|w| w.items_work = true);
    fake_game::with(|g| g.commands = vec!["/autoinventory".into()]);
    let in_bag = SlotLocation::new(ItemContainerKind::Inventory, 23, Some(2));

    let log = pulse_log(move |mq| format!("{:?}", [mq.split_stack(in_bag, 5), mq.split_stack(in_bag, 0), mq.auto_inventory()]));
    assert_eq!(log, ["[true, false, false]"], "nothing to split, or on the cursor");
    let possessions = ItemContainerInstance::eItemContainerPossessions.0;
    assert_eq!(fake_windows::log(), [format!("split {possessions} 23 2 5")]);
    assert!(fake_game::log().is_empty());

    fill(game.profile.get().InventoryContainer(), &mut game.inventory_slots, 34, &[(usize::from(InvSlot::CURSOR.0), &ration)]);
    assert_eq!(pulse_log(|mq| format!("{}", mq.auto_inventory())), ["true"]);
    assert_eq!(fake_windows::log(), ["attach_cursor"], "the game's /autoinventory needs the cursor's icon to show the item");
    assert_eq!(fake_game::log(), ["/autoinventory"]);
    fake_game::with(|g| g.commands.clear());
    assert_eq!(pulse_log(|mq| format!("{}", mq.auto_inventory())), ["false"], "the game's answer");
}

#[test]
fn merchants_items_are_selected_bought_and_sold() {
    let window = Box::leak(Box::new(Fake::<CMerchantWnd>::new()));
    let page = Box::leak(Box::new(Fake::<MerchantPageHandler>::new()));
    let (bread, ration) = (FakeItem::new(13005, "Bread"), FakeItem::new(13006, "Ration").stack(12, 20));
    // The merchant's items know their own slots, which needn't be their list's.
    bread.item.get().GlobalIndex().set_Location(ItemContainerInstance::eItemContainerMerchant);
    bread.item.get().GlobalIndex().Index().set_m_slots([7, -1, -1]);
    ration.item.get().GlobalIndex().set_Location(ItemContainerInstance::eItemContainerPossessions);
    let mut slots = Vec::new();
    fill(page.get().Items(), &mut slots, 2, &[(0, &bread)]);
    let pages = Box::leak(Box::new([page.ptr()]));
    window.get().PageHandlers().set_m_data(pages.as_mut_ptr());
    window.get().PageHandlers().set_m_size(1);
    window.get().set_dShow(true);
    set_zone((1, 0.0, 0.0), &[(40, "Merchant Jo", 1, 5.0, 0.0)]);
    let jo = ZONE.with(|zone| zone.borrow()[0].ptr());
    set_windows(|w| {
        w.merchant_window = window.ptr();
        w.active_merchant = jo;
    });
    fake_windows::with(|w| w.items_work = true);
    let mine = SlotLocation::new(ItemContainerKind::Inventory, 23, Some(2));
    let act = move |mq: &Mq<Recorder>| {
        let selected = [mq.select_merchant_item(0), mq.select_merchant_item(1), mq.select_item_to_sell(mine), mq.select_item_to_sell(SlotLocation::new(ItemContainerKind::Bank, 3, None))];
        format!("{selected:?} | {:?}", [mq.buy(2), mq.sell(3), mq.buy(0)])
    };

    assert_eq!(pulse_log(act), ["[true, false, true, false] | [false, false, false]"], "nothing selected to buy or sell");
    let (merchant, possessions) = (ItemContainerInstance::eItemContainerMerchant.0, ItemContainerInstance::eItemContainerPossessions.0);
    assert_eq!(fake_windows::log(), [format!("merchant_select {merchant} 7 -1"), format!("merchant_select {possessions} 23 2")]);

    window.get().pSelectedItem().set_m_pObject(bread.item.ptr());
    assert_eq!(pulse_log(act)[0].split_once(" | ").unwrap().1, "[true, false, false]", "the merchant's item is bought, not sold");
    assert_eq!(fake_windows::log()[2..], ["buy 2"]);
    window.get().pSelectedItem().set_m_pObject(ration.item.ptr());
    assert_eq!(pulse_log(act)[0].split_once(" | ").unwrap().1, "[false, true, false]", "yours is sold");
    assert_eq!(fake_windows::log()[2..], ["sell 3"]);

    window.get().set_dShow(false);
    assert_eq!(pulse_log(act), ["[false, false, false, false] | [false, false, false]"], "the window is closed");
    assert!(fake_windows::log().is_empty());
}

#[test]
fn without_tasks_friends_tribute_or_achievements_there_are_none() {
    let log = pulse_log(|mq| {
        format!(
            "{} {:?} {} {:?} {}",
            mq.quests().count(),
            mq.friends(),
            mq.is_friend("Ann"),
            mq.tribute(),
            mq.achievements().is_none(),
        )
    });
    assert_eq!(log, ["0 None false None true"]);
}

#[test]
fn tasks_are_found_by_title_and_id() {
    let manager = Box::leak(Box::new(Fake::<CTaskManager>::new()));
    let member = Box::leak(Box::new(Fake::<SharedTaskPlayerInfo>::new()));
    member.get().set_Name(set_name("Ann"));
    let solo = manager.get().QuestEntries(3);
    solo.set_TaskID(7);
    solo.set_TaskTitle(set_name("Rat Hunt"));
    solo.set_TaskSystem(TaskSystemType::cTaskSystemTypeSoloQuest);
    let shared = manager.get().SharedTaskEntries(0);
    shared.set_TaskID(9);
    shared.set_TaskTitle(set_name("Together"));
    shared.set_TaskSystem(TaskSystemType::cTaskSystemTypeSharedQuest);
    set_windows(|w| {
        w.task_manager = manager.ptr();
        w.task_members = member.ptr();
    });
    let log = pulse_log(|mq| {
        format!(
            "{:?} {:?} {:?} {:?} {}",
            mq.quests().map(|q| q.title()).collect::<Vec<_>>(),
            mq.quest("rat hunt").map(|q| q.id()),
            mq.quest_by_id(9).map(|q| q.members().map(|m| m.name()).collect::<Vec<_>>()),
            mq.quest("Rat").map(|q| q.id()),
            mq.quest_window_row(&mq.quest_by_id(7).unwrap()).is_none() && !mq.select_quest(&mq.quest_by_id(7).unwrap()),
        )
    });
    assert_eq!(log, ["[\"Together\", \"Rat Hunt\"] Some(7) Some([\"Ann\"]) None true"], "no task window: no row to select");
}

#[test]
fn friends_tribute_and_achievements_need_a_character() {
    let game = game();
    game.pc.get().set_CurrFavor(1500);
    game.pc.get().set_CareerFavor(9000);
    game.pc.get().set_BenefitTimer(540_000);
    let manager = Box::leak(Box::new(Fake::<AchievementManager>::new()));
    fake_game::with(|game| {
        game.friends = Some(vec!["Ann".into(), "Bob".into()]);
        game.active_favor_cost = 25;
        game.achievement_manager = manager.ptr();
    });
    let log = |mq: &Mq<Recorder>| {
        let tribute = mq.tribute().unwrap();
        let achievements = mq.achievements().unwrap();
        format!(
            "{:?} {} {} {} {} {:?} {} {} {}",
            mq.friends(),
            mq.is_friend("bob"),
            tribute.active,
            tribute.current_favor,
            tribute.career_favor,
            tribute.active_cost,
            tribute.time_left.as_secs(),
            achievements.is_ready(),
            achievements.len(),
        )
    };
    assert_eq!(pulse_log(log), ["Some([\"Ann\", \"Bob\"]) true false 1500 9000 None 540 false 0"], "inactive tribute costs nothing");
    set_windows(|w| w.tribute_active = true);
    manager.get().set_achievementClientReadOnlyDataSet(true);
    manager.get().set_achievementClientStatesSet(true);
    assert_eq!(pulse_log(log), ["Some([\"Ann\", \"Bob\"]) true true 1500 9000 Some(25) 540 true 0"]);
}
