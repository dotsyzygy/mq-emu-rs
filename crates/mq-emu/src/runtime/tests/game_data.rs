//! Inventory, spells, buffs, group, zone, ground items, pets, other spawns'
//! buffs, raids, mercenaries, extended targets, aggro, alternate abilities,
//! skills (and using them), combat state, setting the target,
//! the merchant and loot windows, tasks, friends, tribute and achievements
//! through `Mq`, against fake game objects: what each finds, and that missing
//! game data reads as nothing rather than crashing.

use mq_emu_sys::constants::{DB_STRING_ALT_ABILITY_NAME, MAX_TOTAL_BUFFS};
use mq_emu_sys::game::{SKILL_ACTIVATABLE, SKILL_HAS, mqemu_cached_buff};
use mq_emu_sys::layouts::{
    AchievementManager, AggroMeterManagerClient, CAltAbilityData, eCharacterStatus, CGroup, CGroupMember, CLootWnd, CMercenaryManager, CMerchantWnd, CPetInfoWnd, CRaid, CTargetWnd, CTaskManager,
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
use crate::game::{Spawn, SpawnId};
use crate::CombatState;
use crate::xtarget::XTARGET_AUTO_HATER;
use crate::spells::{Buff, SpellId};
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
        plugin.log.push(format!(
            "{:?} {:?} {:?} {:?} {:?} {} {} {:?}",
            mq.hp_current(),
            mq.hp_max(),
            mq.hp_percent(),
            mq.mana_current(),
            mq.mana_max(),
            mq.is_auto_attacking(),
            mq.is_stunned(),
            mq.server_name(),
        ));
    });
    assert_eq!(log_of(rt)[2..], ["true true 0 true true true 0", "None None None None None false false None"]);
}

#[test]
fn your_hp_and_mana_are_your_characters_not_your_spawns() {
    let game = game();
    game.profile.get().set_Mana(10277);
    fake_game::with(|g| {
        g.hp = (450, 1800);
        g.max_mana = 12000;
    });
    // The spawn's own fields, which lag behind or differ in game.
    player().set_ManaCurrent(96);
    player().set_HPCurrent(1);
    let log = pulse_log(|mq| {
        format!(
            "{:?} {:?} {:?} {:?} {:?} {}",
            mq.hp_current(),
            mq.hp_max(),
            mq.hp_percent(),
            mq.mana_current(),
            mq.mana_max(),
            mq.me().unwrap().mana_current(),
        )
    });
    assert_eq!(log, ["Some(450) Some(1800) Some(25) Some(10277) Some(12000) 96"]);
}

#[test]
fn your_hp_percent_is_zero_without_a_maximum() {
    game();
    fake_game::with(|g| g.hp = (450, 0));
    assert_eq!(pulse_log(|mq| format!("{:?}", mq.hp_percent())), ["Some(0)"]);
}

#[test]
fn combat_stun_and_server_come_from_the_client_and_your_character() {
    let game = game();
    let info = unsafe { &*INFO.with(FakeObject::ptr) };
    let log = || pulse_log(|mq| format!("{} {} {:?}", mq.is_auto_attacking(), mq.is_stunned(), mq.server_name()));
    assert_eq!(log(), ["false false None"]);

    info.set_bAutoAttack(true);
    game.pc.get().set_Status(eCharacterStatus::eCharStatusStunned);
    let mut name = [0 as std::ffi::c_char; 40];
    for (dst, src) in name.iter_mut().zip(b"bristle") {
        *dst = *src as std::ffi::c_char;
    }
    info.set_WorldServerShortname(name);
    assert_eq!(log(), ["true true Some(\"bristle\")"]);

    // Frozen or dead isn't stunned.
    game.pc.get().set_Status(eCharacterStatus::eCharStatusFrozen);
    assert_eq!(log(), ["true false Some(\"bristle\")"]);
}

#[test]
fn spawn_states_match_macroquests_spawn_members() {
    use mq_emu_sys::constants::{PLAYER_STATE_STUNNED, STANDSTATE_DUCK, STANDSTATE_FEIGN, STANDSTATE_SIT, STANDSTATE_STAND};
    let log = || {
        pulse_log(|mq| {
            let me = mq.me().unwrap();
            format!("{} {} {} {} {} {}", me.is_sitting(), me.is_feigning(), me.is_invisible(), me.is_hovering(), me.is_moving(), me.is_stunned())
        })
    };
    player().set_StandState(STANDSTATE_STAND);
    assert_eq!(log(), ["false false false false false false"]);

    player().set_StandState(STANDSTATE_SIT);
    player().set_HideMode(2);
    player().set_SpeedRun(-0.7); // backing up
    player().set_PlayerState(PLAYER_STATE_STUNNED | 4);
    assert_eq!(log(), ["true false true false true true"]);

    player().set_StandState(STANDSTATE_FEIGN);
    player().set_RespawnTimer(12345);
    player().set_PlayerState(4); // aggressive, not stunned
    assert_eq!(log(), ["false true true true true false"]);

    player().set_StandState(STANDSTATE_DUCK);
    assert_eq!(log(), ["false false true true true false"]);
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
fn auras_are_listed_in_window_order() {
    game();
    let aura = |name: &str, spawn_id| {
        let mut raw = mq_emu_sys::game::mqemu_aura { name: [0; 64], spawn_id, cost: 3, icon_id: 7 };
        for (out, byte) in raw.name.iter_mut().zip(name.bytes()) {
            *out = byte as std::ffi::c_char;
        }
        raw
    };
    fake_game::with(|g| g.auras = vec![aura("Disciple's Aura", 0), aura("Master's Aura", 55)]);
    let log = pulse_log(|mq| format!("{:?}", mq.auras().iter().map(|a| (a.name.clone(), a.spawn_id.map(|id| id.0), a.cost, a.icon_id)).collect::<Vec<_>>()));
    assert_eq!(log, ["[(\"Disciple's Aura\", None, 3, 7), (\"Master's Aura\", Some(55), 3, 7)]"]);
}

#[test]
fn buff_time_left_comes_from_its_own_window() {
    let game = game();
    game.learn(1200, "Courage");
    game.learn(717, "Selo's Accelerando");
    let profile = game.profile.get();
    profile.Buffs(0).set_SpellID(1200);
    profile.Buffs(0).set_Duration(10);
    profile.Buffs(1).set_SpellID(717);
    profile.Buffs(1).set_Duration(-1); // doesn't wear off
    profile.Buffs(mq_emu_sys::constants::NUM_LONG_BUFFS).set_SpellID(717);
    profile.Buffs(mq_emu_sys::constants::NUM_LONG_BUFFS).set_Duration(3);
    // 717 in both windows: the song's time is the song window's.
    fake_game::with(|g| g.buff_timers = [((1200, false), 58_500), ((717, false), 1), ((717, true), 4_200)].into());
    let log = pulse_log(|mq| {
        let left = |buff: Buff<'_>| (buff.is_song(), buff.remaining().map(|d| d.as_millis()));
        format!("{:?} {:?}", mq.buffs().map(left).collect::<Vec<_>>(), mq.songs().map(left).collect::<Vec<_>>())
    });
    assert_eq!(log, ["[(false, Some(58500)), (false, None)] [(true, Some(4200))]"]);
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
fn combat_abilities_list_the_highest_rank_with_its_timer() {
    let game = game();
    game.learn(100, "Leopard Claw");
    game.learn(200, "Ashenhand Discipline");
    game.learn(201, "Ashenhand Discipline Rk. II");
    let [claw, ashen1, ashen2] = [&game.known[0], &game.known[1], &game.known[2]].map(|s| s.get());
    claw.set_ReuseTimerIndex(3);
    ashen1.set_SpellGroup(55);
    ashen1.set_SpellRank(1);
    ashen1.set_ReuseTimerIndex(-1);
    ashen2.set_SpellGroup(55);
    ashen2.set_SpellRank(2);
    ashen2.set_ReuseTimerIndex(5);
    let profile = game.profile.get();
    profile.set_CombatAbilities_at(0, 200); // outranked by slot 4
    profile.set_CombatAbilities_at(2, 100);
    profile.set_CombatAbilities_at(4, 201);
    profile.set_CombatAbilities_at(6, 9999); // a spell the client doesn't know
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as u32;
    profile.set_CombatAbilityTimers_at(3, now + 600);
    profile.set_CombatAbilityTimers_at(5, now - 1);
    fake_game::with(|g| g.spells_work = true);

    let log = pulse_log(|mq| {
        let list: Vec<(usize, String, bool)> = mq.combat_abilities().map(|a| (a.slot(), a.name(), a.is_ready())).collect();
        let claw = mq.combat_ability_by_name("LEOPARD CLAW").unwrap();
        let reuse = claw.reuse().as_secs();
        format!(
            "{list:?} {} {:?} {:?} {}",
            (590..=600).contains(&reuse),
            mq.combat_ability(0).map(|a| a.id()),
            mq.combat_ability(300).map(|a| a.id()),
            mq.use_combat_ability(&claw),
        )
    });
    assert_eq!(log, ["[(2, \"Leopard Claw\", false), (4, \"Ashenhand Discipline Rk. II\", true)] true None None true"]);
    assert_eq!(fake_game::with(|g| std::mem::take(&mut g.log)), ["disc 100"]);
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

/// A shaman with Inner Fire (3 s, 10 mana, range 100) in gem 3, ready, and
/// two spawns: Jo (40) near and Far (41) out of range.
fn caster() {
    let game = game();
    game.learn(1200, "Inner Fire");
    let spell = game.known[0].get();
    spell.set_CastTime(3000);
    spell.set_ManaCost(10);
    spell.set_Range(100.0);
    game.profile.get().set_MemorizedSpells_at(2, 1200);
    game.profile.get().set_Mana(100);
    fake_game::with(|g| {
        g.commands = vec!["/cast".into(), "/stopcast".into(), "/stand".into()];
        g.spells_work = true;
        g.ready_gems = vec![2];
    });
    player().CastingData().set_SpellID(-1);
    set_zone((1, 0.0, 0.0), &[(40, "Jo", 0, 10.0, 0.0), (41, "Far", 0, 500.0, 0.0)]);
}

thread_local! {
    static CAST_TASK: Cell<Option<crate::TaskId>> = const { Cell::new(None) };
}

/// Runs `cast` as a task that notes its result.
fn cast_task(cast: crate::Cast) -> &'static Runtime<Recorder> {
    let rt = with_task(move |task| async move {
        CAST_TASK.with(|id| id.set(Some(task.id())));
        let result = task.cast(cast).await;
        note(&task, &format!("{result:?}"));
    });
    rt.pulse();
    rt
}

/// The client shows a message, as eqclient.dll sees it.
fn client_shows(id: u32) {
    super::fake_client::shows(id);
}

#[test]
fn a_cast_that_fails_its_checks_ends_at_once() {
    use crate::{Cast, CastResult};
    caster();
    for (cast, expected) in [
        (Cast::new(SpellId(999)), CastResult::NotMemorized),
        (Cast::new(SpellId(1200)).on(SpawnId(41)), CastResult::OutOfRange),
        (Cast::new(SpellId(1200)).on(SpawnId(99)), CastResult::NoTarget),
    ] {
        let rt = cast_task(cast);
        rt.pulse();
        assert_eq!(task_notes(rt), [format!("{expected:?}")]);
        rt.shutdown();
    }
    fake_game::with(|g| g.ready_gems.clear());
    let rt = cast_task(Cast::new(SpellId(1200)));
    rt.pulse();
    assert_eq!(task_notes(rt), ["NotReady"]);
    assert!(fake_game::log().is_empty(), "nothing was cast");
}

#[test]
fn a_cast_follows_the_casting_state_and_the_games_messages() {
    use crate::{Cast, CastResult};
    caster();
    let rt = cast_task(Cast::new(SpellId(1200)).on(SpawnId(40)).restoring_target());
    rt.pulse();
    assert_eq!(fake_game::log(), ["/cast 3"]);
    assert_eq!(fake_target(), ZONE.with(|zone| zone.borrow()[0].ptr()), "targeted Jo to cast");
    player().CastingData().set_SpellID(1200);
    advance_clock(Duration::from_secs(2));
    rt.pulse();
    player().CastingData().set_SpellID(-1);
    client_shows(crate::eqstr::TARGET_RESISTED);
    rt.pulse();
    assert!(task_notes(rt).is_empty(), "a later message could still come");
    advance_clock(Duration::from_secs(1));
    rt.pulse();
    assert_eq!(task_notes(rt), [format!("{:?}", CastResult::Resisted)]);
    assert!(fake_target().is_null(), "back to no target");
}

#[test]
fn a_cast_with_no_message_landed() {
    caster();
    let rt = cast_task(crate::Cast::new(SpellId(1200)));
    rt.pulse();
    player().CastingData().set_SpellID(1200);
    fake_game::with(|g| g.cast_time_left_ms = 2500);
    rt.pulse();
    fake_game::with(|g| g.cast_time_left_ms = 100);
    rt.pulse();
    player().CastingData().set_SpellID(-1);
    fake_game::with(|g| g.cast_time_left_ms = -1);
    rt.pulse();
    advance_clock(Duration::from_secs(1));
    rt.pulse();
    assert_eq!(task_notes(rt), ["Landed"], "it ran its time");
}

#[test]
fn a_cast_that_stops_early_without_a_message_didnt_land() {
    caster();
    let rt = cast_task(crate::Cast::new(SpellId(1200)));
    rt.pulse();
    super::fake_client::shows(crate::eqstr::BEGIN_CASTING);
    player().CastingData().set_SpellID(1200);
    fake_game::with(|g| g.cast_time_left_ms = 2500);
    rt.pulse();
    player().CastingData().set_SpellID(-1);
    fake_game::with(|g| g.cast_time_left_ms = -1);
    rt.pulse();
    advance_clock(Duration::from_secs(1));
    rt.pulse();
    assert_eq!(task_notes(rt), ["EndedEarly"], "2.5 s were left");

    // Begun by its message, but never seen casting: no evidence it ran.
    let rt = cast_task(crate::Cast::new(SpellId(1200)));
    rt.pulse();
    super::fake_client::shows(crate::eqstr::BEGIN_CASTING);
    rt.pulse();
    advance_clock(Duration::from_secs(1));
    rt.pulse();
    assert_eq!(task_notes(rt), ["EndedEarly"]);
}

#[test]
fn a_cast_the_game_never_starts_ends() {
    caster();
    let rt = cast_task(crate::Cast::new(SpellId(1200)));
    rt.pulse();
    advance_clock(Duration::from_secs(2));
    rt.pulse();
    assert_eq!(task_notes(rt), ["NotStarted"]);

    let rt = cast_task(crate::Cast::new(SpellId(1200)));
    rt.pulse();
    client_shows(crate::eqstr::CANNOT_SEE_TARGET);
    rt.pulse();
    advance_clock(Duration::from_secs(1));
    rt.pulse();
    assert_eq!(task_notes(rt), ["CannotSee"], "a message without a cast is the outcome");
}

#[test]
fn messages_decide_the_outcome_and_the_worst_wins() {
    use crate::eqstr::*;
    caster();
    let rt = cast_task(crate::Cast::new(SpellId(1200)));
    rt.pulse();
    client_shows(BEGIN_CASTING);
    player().CastingData().set_SpellID(1200);
    rt.pulse();
    client_shows(SPELL_INTERRUPTED);
    client_shows(SPELL_FIZZLES);
    client_shows(SPELL_INTERRUPTED);
    player().CastingData().set_SpellID(-1);
    rt.pulse();
    advance_clock(Duration::from_secs(1));
    rt.pulse();
    assert_eq!(task_notes(rt), ["Fizzled"], "fizzled outranks interrupted, whatever the order");
}

#[test]
fn an_earlier_casts_late_interrupt_isnt_this_casts() {
    use crate::eqstr::*;
    caster();
    let rt = cast_task(crate::Cast::new(SpellId(1200)));
    rt.pulse();
    // The server's answer to stopping the last cast arrives after this one is sent.
    client_shows(SPELL_INTERRUPTED);
    client_shows(BEGIN_CASTING);
    player().CastingData().set_SpellID(1200);
    fake_game::with(|g| g.cast_time_left_ms = 200);
    rt.pulse();
    player().CastingData().set_SpellID(-1);
    fake_game::with(|g| g.cast_time_left_ms = -1);
    rt.pulse();
    advance_clock(Duration::from_secs(1));
    rt.pulse();
    assert_eq!(task_notes(rt), ["Landed"]);

    // A stopped cast's begin and interrupt both arrive late, before this one's
    // begin (seen in game: [12205, 439, 12205], and the buff landed).
    let rt = cast_task(crate::Cast::new(SpellId(1200)));
    rt.pulse();
    client_shows(BEGIN_CASTING);
    client_shows(SPELL_INTERRUPTED);
    client_shows(BEGIN_CASTING);
    player().CastingData().set_SpellID(1200);
    fake_game::with(|g| g.cast_time_left_ms = 200);
    rt.pulse();
    player().CastingData().set_SpellID(-1);
    fake_game::with(|g| g.cast_time_left_ms = -1);
    rt.pulse();
    advance_clock(Duration::from_secs(1));
    rt.pulse();
    assert_eq!(task_notes(rt), ["Landed"], "only what follows the last begin is this cast's");

    // Once it has begun, an interrupt is its own, by message or casting state.
    let rt = cast_task(crate::Cast::new(SpellId(1200)));
    rt.pulse();
    player().CastingData().set_SpellID(1200);
    rt.pulse();
    client_shows(SPELL_INTERRUPTED);
    player().CastingData().set_SpellID(-1);
    rt.pulse();
    advance_clock(Duration::from_secs(1));
    rt.pulse();
    assert_eq!(task_notes(rt), ["Interrupted"]);
}

#[test]
fn without_the_clients_messages_nothing_is_cast() {
    caster();
    super::fake_client::set_status(eq_client::abi::Status::UNKNOWN_CLIENT);
    let rt = cast_task(crate::Cast::new(SpellId(1200)));
    rt.pulse();
    assert_eq!(task_notes(rt), ["NoMessages"]);
    assert!(fake_game::log().is_empty(), "nothing was cast");
}

#[test]
fn a_cast_past_its_timeout_or_cancelled_is_stopped() {
    caster();
    let rt = cast_task(crate::Cast::new(SpellId(1200)).timeout(Duration::from_secs(4)));
    rt.pulse();
    player().CastingData().set_SpellID(1200);
    rt.pulse();
    advance_clock(Duration::from_secs(5));
    rt.pulse();
    assert_eq!(task_notes(rt), ["TimedOut"]);
    assert_eq!(fake_game::log(), ["/cast 3", "/stopcast"]);

    rt.shutdown();
    player().CastingData().set_SpellID(-1);
    let rt = cast_task(crate::Cast::new(SpellId(1200)));
    rt.pulse();
    player().CastingData().set_SpellID(1200);
    rt.pulse();
    assert!(Mq::new(rt).cancel_task(CAST_TASK.with(Cell::get).unwrap()));
    assert_eq!(fake_game::log(), ["/cast 3", "/stopcast"], "cancelling stops the cast");
}

#[test]
fn memorizing_waits_for_the_gem() {
    use crate::MemorizeResult;
    let game = game();
    game.learn(1200, "Inner Fire");
    game.known[0].get().set_ClassLevel_at(10, 1);
    let profile = game.profile.get();
    profile.set_Class(10);
    profile.set_Level(10);
    profile.set_SpellBook_at(0, 1200);
    fake_game::with(|g| g.spells_work = true);
    let rt = with_task(move |task| async move {
        let refused = task.memorize(4, SpellId(2000)).await;
        let memorized = task.memorize(4, SpellId(1200)).await;
        note(&task, &format!("{refused:?} {memorized:?}"));
    });
    rt.pulse();
    rt.pulse();
    assert_eq!(fake_game::log(), [loadout(&[(4, 1200)])], "only a spell in the book is memorized");
    profile.set_MemorizedSpells_at(4, 1200);
    rt.pulse();
    assert!(task_notes(rt).is_empty(), "in the gem but not ready yet");
    fake_game::with(|g| g.ready_gems = vec![4]);
    rt.pulse();
    assert_eq!(task_notes(rt), [format!("{:?} {:?}", MemorizeResult::Refused, MemorizeResult::Memorized)]);
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

#[test]
fn combat_state_endurance_and_standing() {
    use mq_emu_sys::constants::{COMBAT_STATE_COMBAT, COMBAT_STATE_REGEN, STANDSTATE_SIT, STANDSTATE_STAND};
    let game = game();
    game.profile.get().set_Endurance(30);
    fake_game::with(|g| g.max_endurance = 120);
    player().set_StandState(STANDSTATE_STAND);
    let log = || pulse_log(|mq| format!("{:?} {:?} {}", mq.combat_state(), mq.endurance_percent(), mq.me().unwrap().is_standing()));
    assert_eq!(log(), ["Some(Active) Some(25) true"]);

    COMBAT_STATE.with(|state| state.set(COMBAT_STATE_COMBAT));
    fake_game::with(|g| g.max_endurance = 0);
    assert_eq!(log(), ["Some(Combat) Some(0) true"], "no maximum reads 0%");

    COMBAT_STATE.with(|state| state.set(COMBAT_STATE_REGEN));
    player().set_StandState(STANDSTATE_SIT);
    assert_eq!(log(), ["Some(Resting) Some(0) false"]);
    COMBAT_STATE.with(|state| state.set(99));
    assert_eq!(pulse_log(|mq| format!("{:?}", mq.combat_state().map(CombatState::name))), ["Some(\"ACTIVE\")"], "as ${{Me.CombatState}} reads an unknown state");
}

#[test]
fn aggro_and_target_of_target() {
    game();
    set_zone((1, 0.0, 0.0), &[(20, "a rat", 1, 5.0, 0.0), (30, "Soandso", 0, 2.0, 0.0)]);
    let name = |spawn: Option<crate::Spawn<'_>>| spawn.map(|s| s.name());
    let log = || {
        pulse_log(move |mq| {
            format!(
                "{:?} {:?} {:?} {:?} {:?}",
                mq.aggro_percent(),
                mq.secondary_aggro_percent(),
                name(mq.secondary_aggro_player()),
                name(mq.aggro_lock()),
                name(mq.target_of_target())
            )
        })
    };
    assert_eq!(log(), ["None None None None None"]);

    let aggro = Box::leak(Box::new(Fake::<AggroMeterManagerClient>::new()));
    aggro.get().aggroData(mq_emu_sys::constants::AD_PLAYER).set_AggroPct(70);
    aggro.get().aggroData(mq_emu_sys::constants::AD_SECONDARY).set_AggroPct(40);
    aggro.get().set_AggroSecondaryID(30);
    aggro.get().set_AggroLockID(20);
    set_windows(|w| w.aggro = aggro.ptr());
    player().set_TargetOfTarget(30);
    assert_eq!(log(), ["Some(70) Some(40) Some(\"Soandso\") Some(\"a rat\") Some(\"Soandso\")"]);

    aggro.get().set_AggroLockID(0);
    player().set_TargetOfTarget(99); // gone from the zone
    assert_eq!(log(), ["Some(70) Some(40) Some(\"Soandso\") None None"]);
}

#[test]
fn the_target_is_set_and_cleared() {
    game();
    set_zone((1, 0.0, 0.0), &[(20, "a rat", 1, 5.0, 0.0)]);
    let log = pulse_log(|mq| {
        let rat = mq.spawn(SpawnId(20)).unwrap();
        let set = mq.set_target(Some(&rat));
        let targeted = mq.target().map(|s| s.id().0);
        let cleared = mq.set_target(None);
        format!("{set} {targeted:?} {cleared} {:?}", mq.target().map(|s| s.id().0))
    });
    assert_eq!(log, ["true Some(20) true None"]);
}

#[test]
fn abilities_and_alt_abilities_are_used() {
    let game = game();
    let (active, passive) = (Box::leak(Box::new(Fake::<CAltAbilityData>::new())), Box::leak(Box::new(Fake::<CAltAbilityData>::new())));
    active.get().set_GroupID(35);
    active.get().set_SpellID(1000);
    passive.get().set_GroupID(36);
    passive.get().set_SpellID(-1);
    game.profile.get().AAList(0).set_AAIndex(500);
    game.profile.get().AAList(1).set_AAIndex(612);
    game.profile.get().set_Skill_at(40, 50);
    game.profile.get().set_Skill_at(10, 200);
    fake_game::with(|g| {
        g.alt_abilities.insert(500, active.ptr());
        g.alt_abilities.insert(612, passive.ptr());
        g.skills.insert(40, FakeSkill { name: "Sneak".into(), flags: SKILL_HAS | SKILL_ACTIVATABLE, ..FakeSkill::default() });
        g.skills.insert(10, FakeSkill { name: "Defense".into(), flags: SKILL_HAS, ..FakeSkill::default() });
        // Longer than "/alt", as the game names it.
        g.commands = vec!["/alternateadv".into()];
    });

    let log = pulse_log(|mq| {
        let (sneak, defense) = (mq.skill(SkillId(40)).unwrap(), mq.skill(SkillId(10)).unwrap());
        let (active, passive) = (mq.alt_ability(35).unwrap(), mq.alt_ability(36).unwrap());
        format!("{} {} {} {}", mq.use_ability(&sneak), mq.use_ability(&defense), mq.use_alt_ability(&active), mq.use_alt_ability(&passive))
    });
    assert_eq!(log, ["true false true false"], "only abilities, and AAs that aren't passive");
    assert_eq!(fake_game::log(), ["skill 40", "/alternateadv activate 35"]);
}

#[test]
fn auto_attack_is_switched_and_melee_range_read() {
    game();
    set_zone((1, 0.0, 0.0), &[(20, "a rat", 1, 5.0, 0.0)]);
    fake_game::with(|g| g.commands = vec!["/attack".into()]);
    let log = pulse_log(|mq| {
        let (me, rat) = (mq.me().unwrap(), mq.spawn(SpawnId(20)).unwrap());
        let key = |spawn: &Spawn<'_>| std::ptr::from_ref(spawn.raw()) as usize;
        fake_game::with(|g| g.melee_ranges = [((key(&me), key(&rat)), 22.5)].into());
        format!("{} {} {} {}", mq.set_auto_attack(true), mq.set_auto_attack(false), mq.melee_range(&me, &rat), mq.melee_range(&rat, &me))
    });
    assert_eq!(log, ["true true 22.5 14"], "each way round is its own range");
    assert_eq!(fake_game::log(), ["/attack on", "/attack off"]);
}

#[test]
fn the_skill_stack_probe_reaches_the_shim() {
    let game = game();
    game.profile.get().set_Skill_at(10, 200);
    let log = pulse_log(|mq| {
        let skill = mq.skill(SkillId(10)).unwrap();
        let shift = |cap, through_eqlib| mq.skill_call_stack_shift(&skill, cap, through_eqlib);
        format!("{} {} {} {}", shift(false, true), shift(true, true), shift(false, false), shift(true, false))
    });
    assert_eq!(log, ["4 -4 0 0"]);
}
