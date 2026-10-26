//! Selftest checks for buffs on others, the raid (formed with the other
//! characters running this plugin if you aren't in one), your mercenary,
//! extended targets, alternate abilities, skills and the merchant and loot windows:
//! mq-emu's reading of each against MacroQuest's `${Target.Buff}`,
//! `${Me.Pet.Buff}`, `${Raid}`, `${Mercenary}`, `${Me.XTarget}`,
//! `${Me.AltAbility}`, `${Me.Skill}`, `${Merchant}` and `${Corpse}`.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use mq_emu::sys::constants::{MAX_TOTAL_BUFFS, NUM_SKILLS};
use mq_emu::{Address, GameState, MercenaryState, Mq, Plugin, SkillId, SpawnId, Task};

use crate::RustTest;
use crate::checks::{self, Check, Report, same_text};
use crate::game_checks::{all_match, id_or_null};

/// Mercenaries can't be had on the server this is tested on, so their checks
/// only skip. Set to true where they can.
const MERCENARIES_TESTABLE: bool = false;
/// How long a target's buffs may take to arrive after targeting it.
const TARGET_BUFFS_TIMEOUT: Duration = Duration::from_secs(3);
/// How long to keep asking other characters to accept a raid or group invite.
const INVITE_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a formed or disbanded raid may take to show.
const RAID_TIMEOUT: Duration = Duration::from_secs(5);
/// How long leaving or joining a group may take to show.
const GROUP_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// A boolean as MacroQuest shows it.
fn tf(value: bool) -> &'static str {
    if value { "TRUE" } else { "FALSE" }
}

/// The checks that need no waiting. Target buffs and forming a raid with
/// other characters wait for the server, so they run later
/// ([`target_buffs`], [`raid_with_others`]).
pub fn run<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    if mq.game_state() != GameState::InGame {
        report.print(mq, "pet buffs, raid, mercenary, AAs and skills", Check::Skip("needs to be in game".into()));
        return;
    }
    pet_buffs(mq, report);
    if mq.raid().is_some() {
        raid(mq, report);
    } else {
        report.note(mq, "not in a raid: one is formed with the other characters running this plugin, under Raid");
    }
    if MERCENARIES_TESTABLE {
        mercenary(mq, report);
    } else {
        report.print(mq, "mercenary", Check::Skip("no mercenaries on this server".into()));
    }
    xtargets(mq, report);
    alt_abilities(mq, report);
    skills(mq, report);
    merchant_and_loot(mq, report);
}

/// Targets the nearest NPC (or anyone, if there's no NPC) when nothing is
/// targeted. Returns its ID if this did the targeting, to clear it after.
fn target_something<P: Plugin>(mq: &Mq<P>, report: &mut Report) -> Result<Option<SpawnId>, ()> {
    if let Some(target) = mq.target() {
        report.note(mq, &format!("using your target, {} (id {})", target.name(), target.id()));
        return Ok(None);
    }
    let Some(nearest) = mq.nearest(|s| s.is_npc()).or_else(|| mq.nearest(|_| true)) else {
        report.print(mq, "targeting something for the target buff checks", Check::Skip("no target, and nothing nearby to target".into()));
        return Err(());
    };
    let (id, name) = (nearest.id(), nearest.name());
    mq.command_now(format!("/target id {id}"));
    let targeted = mq.target().map(|t| t.id());
    report.print(mq, &format!("no target: /target id {id} targets the nearest, {name}"), Check::that(targeted == Some(id), || {
        format!("target() is {targeted:?}")
    }));
    if targeted == Some(id) { Ok(Some(id)) } else { Err(()) }
}

/// Your target's buffs: targets something if nothing is, waits for the
/// server to send its buffs, then compares the target window, MacroQuest's
/// cache and `${Target.Buff[n]}`.
pub async fn target_buffs(task: &Task<RustTest>, report: &mut Report) {
    let targeted = task.with(|mq| {
        if mq.game_state() != GameState::InGame {
            report.print(mq, "target buffs", Check::Skip("needs to be in game".into()));
            return Err(());
        }
        target_something(mq, report)
    });
    let Ok(targeted) = targeted else { return };
    let started = Instant::now();
    let arrived = task.wait_until_timeout(|mq| mq.target_buffs_populated(), TARGET_BUFFS_TIMEOUT).await;
    let took = started.elapsed();
    task.with(|mq| {
        report.print(mq, &format!("the target's buffs arrive ({} ms)", took.as_millis()), Check::that(arrived, || {
            format!("target_buffs_populated() still false after {TARGET_BUFFS_TIMEOUT:?}")
        }));
        target_buff_checks(mq, report);
        if let Some(id) = targeted {
            checks::clear_target(mq, report, id);
        }
    });
}

fn target_buff_checks<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let Some(target) = mq.target() else {
        report.print(mq, "target buffs", Check::Fail("the target went away".into()));
        return;
    };
    let cached = mq.cached_buffs(&target);
    report.print(mq, &format!("cached_buffs(target).len() ({}) = ${{Target.BuffCount}}", cached.len()), same_text(cached.len(), mq.parse("${Target.BuffCount}")));
    let rows = cached.iter().enumerate().flat_map(|(i, buff)| {
        let n = i + 1;
        [
            (format!("buff {n} spell"), buff.spell_id().to_string(), format!("${{Target.Buff[{n}].SpellID}}")),
            (format!("buff {n} slot"), (buff.slot() + 1).to_string(), format!("${{Target.Buff[{n}].Slot}}")),
            (format!("buff {n} caster"), buff.caster(), format!("${{Target.Buff[{n}].Caster}}")),
        ]
    });
    report.print(mq, "cached buffs match ${Target.Buff[n]} (spell, slot, caster)", all_match(mq, rows));
    report.print(mq, "target_buffs_populated() = ${Target.BuffsPopulated}", same_text(tf(mq.target_buffs_populated()), mq.parse("${Target.BuffsPopulated}")));
    // The target window and MacroQuest's cache come from the same message.
    if mq.target_buffs_populated() {
        let mut window: Vec<i32> = mq.target_buffs().map(|b| b.spell_id().0).collect();
        let mut cache: Vec<i32> = cached.iter().map(|b| b.spell_id().0).collect();
        window.sort_unstable();
        cache.sort_unstable();
        report.print(mq, "the target window shows the cached buffs", Check::that(window == cache, || format!("window {window:?}, cache {cache:?}")));
    } else {
        report.print(mq, "the target window shows the cached buffs", Check::Skip("the target's buffs never arrived".into()));
    }
}

fn pet_buffs<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    if mq.pet().is_some() {
        let ours: Vec<Option<i32>> = (0..MAX_TOTAL_BUFFS).map(|slot| mq.pet_buffs().find(|b| b.slot() == slot).map(|b| b.spell_id().0)).collect();
        let rows = ours.iter().enumerate().map(|(slot, id)| (format!("pet buff {}", slot + 1), id_or_null(*id), format!("${{Me.Pet.Buff[{}].ID}}", slot + 1)));
        report.print(mq, "pet buffs match ${Me.Pet.Buff[n].ID}", all_match(mq, rows));
    } else {
        report.print(mq, "pet buffs", Check::Skip("no pet".into()));
    }
}

/// Sends `invite` (e.g. "/raidinvite") to each of `peers`, then asks each
/// (with `accept`, e.g. "raid:accept") until they've accepted or the time is
/// up: invites reach them through the server. Returns who accepted.
async fn invite(task: &Task<RustTest>, peers: &[String], invite: &str, accept: &str) -> BTreeSet<String> {
    task.with(|mq| {
        for peer in peers {
            mq.command_now(format!("{invite} {peer}"));
        }
    });
    let started = Instant::now();
    let mut accepted = BTreeSet::new();
    while accepted.len() < peers.len() && started.elapsed() < INVITE_TIMEOUT {
        let waiting: Vec<&String> = peers.iter().filter(|p| !accepted.contains(*p)).collect();
        for peer in waiting {
            let reply = task.request_timeout(&Address::character(peer.clone()), accept, REQUEST_TIMEOUT).await;
            if reply.is_ok_and(|r| r.text() == "accepted") {
                accepted.insert(peer.clone());
            }
        }
        task.delay(Duration::from_millis(250)).await;
    }
    accepted
}

/// Passes if everyone in `peers` accepted.
fn all_accepted(peers: &[String], accepted: &BTreeSet<String>) -> Check {
    let missing: Vec<&String> = peers.iter().filter(|p| !accepted.contains(*p)).collect();
    Check::that(missing.is_empty(), || format!("{missing:?} never got an invite to accept"))
}

/// Whether `name` is in your group.
fn in_group<P: Plugin>(mq: &Mq<P>, name: &str) -> bool {
    mq.group().is_some_and(|g| g.member_by_name(name).is_some())
}

/// Forms a raid with the other characters running this plugin (`peers`) if
/// you aren't in one, checks it, and disbands it again. The server doesn't
/// deliver raid invites to your own group, so grouped ones leave the group
/// first and are invited back afterwards. Each answers requests to leave
/// and accept ("group:leave", "raid:accept", ...).
pub async fn raid_with_others(task: &Task<RustTest>, report: &mut Report, peers: &[String]) {
    let grouped = task.with(|mq| {
        if mq.game_state() != GameState::InGame {
            report.print(mq, "raid", Check::Skip("needs to be in game".into()));
            return None;
        }
        report.print(mq, "raid_invite_pending() = ${Raid.Invited}", same_text(tf(mq.raid_invite_pending()), mq.parse("${Raid.Invited}")));
        if mq.raid().is_some() {
            report.note(mq, "already in a raid: checked under the game data above");
            return None;
        }
        report.print(mq, "no raid: ${Raid.Members} is 0", same_text("0", mq.parse("${Raid.Members}")));
        if peers.is_empty() {
            report.print(mq, "forming a raid", Check::Skip("no other characters running this plugin to raid with".into()));
            return None;
        }
        Some(peers.iter().filter(|p| in_group(mq, p)).cloned().collect::<Vec<String>>())
    });
    let Some(grouped) = grouped else { return };

    if !grouped.is_empty() {
        for peer in &grouped {
            let _ = task.request_timeout(&Address::character(peer.clone()), "group:leave", REQUEST_TIMEOUT).await;
        }
        let out = task.wait_until_timeout(|mq| grouped.iter().all(|p| !in_group(mq, p)), GROUP_TIMEOUT).await;
        task.with(|mq| {
            report.print(mq, &format!("{} leave your group to be raid-invited", grouped.join(", ")), Check::that(out, || "still grouped".into()))
        });
    }

    let accepted = invite(task, peers, "/raidinvite", "raid:accept").await;
    let size = 1 + accepted.len();
    // Nobody accepted: there's no raid to wait for.
    let formed = !accepted.is_empty() && task.wait_until_timeout(|mq| mq.raid().is_some_and(|r| r.len() >= size), RAID_TIMEOUT).await;
    task.with(|mq| {
        report.print(mq, &format!("/raidinvite reaches {}, who accept", peers.join(", ")), all_accepted(peers, &accepted));
        if !accepted.is_empty() {
            let members = mq.raid().map_or(0, |r| r.len());
            report.print(mq, &format!("raid() shows the raid of {size}"), Check::that(formed, || {
                format!("raid() has {members} members; ${{Raid.Members}} {}", mq.parse("${Raid.Members}"))
            }));
        }
        if formed {
            raid(mq, report);
        }
    });

    // Leave things as they were.
    if !accepted.is_empty() {
        for peer in &accepted {
            let _ = task.request_timeout(&Address::character(peer.clone()), "raid:leave", REQUEST_TIMEOUT).await;
        }
        task.with(|mq| mq.command_now("/raiddisband"));
        let gone = task.wait_until_timeout(|mq| mq.raid().is_none(), RAID_TIMEOUT).await;
        task.with(|mq| {
            // MacroQuest's count too, so a raid() that can't see raids can't pass this.
            let theirs = mq.parse("${Raid.Members}");
            report.print(mq, "the selftest's raid is disbanded", Check::that(gone && theirs == "0", || {
                format!("raid() gone: {gone}; ${{Raid.Members}} {theirs}")
            }))
        });
    }
    if !grouped.is_empty() {
        let back = invite(task, &grouped, "/invite", "group:accept").await;
        let regrouped = task.wait_until_timeout(|mq| grouped.iter().all(|p| in_group(mq, p)), GROUP_TIMEOUT).await;
        task.with(|mq| {
            let check = match all_accepted(&grouped, &back) {
                Check::Pass => Check::that(regrouped, || "they accepted, but group() doesn't show them".into()),
                other => other,
            };
            report.print(mq, &format!("{} are invited back into your group", grouped.join(", ")), check);
        });
    }
}

fn raid<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let Some(raid) = mq.raid() else { return };
    report.print(mq, "raid.len() = ${Raid.Members}", same_text(raid.len(), mq.parse("${Raid.Members}")));
    // ${Raid.Member[n]} counts filled slots only.
    let rows = raid.members().enumerate().flat_map(|(i, m)| {
        let n = i + 1;
        [
            (format!("member {n} name"), m.name(), format!("${{Raid.Member[{n}].Name}}")),
            (format!("member {n} level"), m.level().to_string(), format!("${{Raid.Member[{n}].Level}}")),
            (format!("member {n} group"), m.group().map_or(0, |g| g + 1).to_string(), format!("${{Raid.Member[{n}].Group}}")),
            (format!("member {n} looter"), tf(raid.may_loot(&m)).to_owned(), format!("${{Raid.Member[{n}].Looter}}")),
        ]
    });
    report.print(mq, "raid members match ${Raid.Member[n]} (name, level, group, looter)", all_match(mq, rows));
    report.print(mq, "raid.leader() = ${Raid.Leader.Name}", same_text(id_or_null(raid.leader().map(|m| m.name())), mq.parse("${Raid.Leader.Name}")));
    report.print(mq, "raid.looters().len() = ${Raid.Looters}", same_text(raid.looters().len(), mq.parse("${Raid.Looters}")));
}

fn mercenary<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let merc = mq.mercenary();
    let state = match merc.map(|m| m.state()) {
        None => "NONE",
        Some(MercenaryState::Active) => "ACTIVE",
        Some(MercenaryState::Suspended) => "SUSPENDED",
        Some(MercenaryState::Dead) => "DEAD",
    };
    report.print(mq, "mercenary().state() = ${Mercenary.State}", same_text(state, mq.parse("${Mercenary.State}")));
    let Some(merc) = merc else { return };
    let stance = merc.stance().map(|s| s.name);
    report.print(mq, "mercenary().stance() = ${Mercenary.Stance}", same_text(id_or_null(stance), mq.parse("${Mercenary.Stance}")));
    report.print(mq, "mercenary().index() + 1 = ${Mercenary.Index}", same_text(merc.index().map_or(0, |i| i + 1), mq.parse("${Mercenary.Index}")));
    let spawn = mq.mercenary_spawn().map(|s| s.id());
    report.print(mq, "mercenary_spawn() = ${Mercenary.ID}", match spawn {
        Some(id) => same_text(id, mq.parse("${Mercenary.ID}")),
        None => Check::Skip(format!("{state}: not out in this zone")),
    });
}

fn xtargets<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let slots: Vec<_> = mq.xtargets().collect();
    report.print(mq, "xtargets().count() = ${Me.XTargetSlots}", same_text(slots.len(), mq.parse("${Me.XTargetSlots}")));
    report.print(mq, "set xtargets = ${Me.XTarget}", same_text(slots.iter().filter(|x| x.is_set()).count(), mq.parse("${Me.XTarget}")));
    let rows = slots.iter().flat_map(|x| {
        let n = x.slot() + 1;
        [
            (format!("slot {n} spawn"), x.spawn_id().map_or(0, |id| id.0).to_string(), format!("${{Me.XTarget[{n}].ID}}")),
            (format!("slot {n} role"), x.role_name(), format!("${{Me.XTarget[{n}].TargetType}}")),
            (format!("slot {n} aggro"), id_or_null(x.aggro_percent()), format!("${{Me.XTarget[{n}].PctAggro}}")),
        ]
    });
    report.print(mq, "extended target slots match ${Me.XTarget[n]} (spawn, role, aggro)", all_match(mq, rows));
}

fn alt_abilities<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    report.print(mq, "alt_ability_points() = ${Me.AAPoints}", same_text(mq.alt_ability_points(), mq.parse("${Me.AAPoints}")));
    report.print(mq, "alt_ability_points_spent() = ${Me.AAPointsSpent}", same_text(mq.alt_ability_points_spent(), mq.parse("${Me.AAPointsSpent}")));
    let owned: Vec<_> = mq.alt_abilities().collect();
    let rows = owned.iter().flat_map(|aa| {
        let id = aa.group_id();
        let (ready, reuse) = aa.readiness();
        [
            (format!("AA {id} name"), aa.name(), format!("${{Me.AltAbility[{id}].Name}}")),
            (format!("AA {id} rank"), aa.rank().to_string(), format!("${{Me.AltAbility[{id}].Rank}}")),
            (format!("AA {id} index"), aa.index().to_string(), format!("${{Me.AltAbility[{id}].Index}}")),
            (format!("AA {id} ready"), tf(ready).to_owned(), format!("${{Me.AltAbilityReady[{id}]}}")),
            (format!("AA {id} timer"), reuse.as_secs().to_string(), format!("${{Me.AltAbilityTimer[{id}].TotalSeconds}}")),
        ]
    });
    report.print(mq, &format!("{} owned AAs match ${{Me.AltAbility[id]}} (name, rank, index, ready, timer)", owned.len()), all_match(mq, rows));
    let rows = owned.iter().take(20).map(|aa| {
        let name = aa.name();
        (name.clone(), id_or_null(mq.alt_ability_by_name(&name).map(|a| a.group_id())), format!("${{Me.AltAbility[{name}].ID}}"))
    });
    report.print(mq, "alt_ability_by_name finds what ${Me.AltAbility[name]} does", all_match(mq, rows));
    let rows = owned.iter().take(20).map(|aa| {
        let index = aa.index();
        (format!("index {index}"), id_or_null(mq.alt_ability_by_index(index).map(|a| a.name())), format!("${{Me.AltAbility[{}].Name}}", aa.group_id()))
    });
    report.print(mq, "alt_ability_by_index(index) is the owned rank", all_match(mq, rows));
}

fn skills<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    let skills: Vec<_> = (0..NUM_SKILLS as i32).filter_map(|id| mq.skill(SkillId(id))).collect();
    let rows = skills.iter().flat_map(|skill| {
        let n = skill.id().0 + 1;
        [
            (format!("skill {n} value"), skill.value().to_string(), format!("${{Me.Skill[{n}]}}")),
            (format!("skill {n} base"), skill.base().to_string(), format!("${{Me.SkillBase[{n}]}}")),
            (format!("skill {n} cap"), skill.cap().to_string(), format!("${{Me.SkillCap[{n}]}}")),
        ]
    });
    report.print(mq, "skills match ${Me.Skill[n]}, ${Me.SkillBase[n]} and ${Me.SkillCap[n]}", all_match(mq, rows));
    let rows = skills.iter().filter(|s| s.is_known()).map(|skill| {
        let n = skill.id().0 + 1;
        (format!("skill {n}"), skill.name(), format!("${{Skill[{n}].Name}}"))
    });
    report.print(mq, "skill names match ${Skill[n].Name}", all_match(mq, rows));
    let abilities: Vec<_> = mq.abilities().collect();
    let rows = abilities.iter().flat_map(|skill| {
        let name = skill.name();
        [
            (format!("{name} ready"), tf(skill.is_ready()).to_owned(), format!("${{Me.AbilityReady[{name}]}}")),
            (format!("{name} reuse"), skill.reuse().1.as_millis().to_string(), format!("${{Me.AbilityTimerTotal[{name}].Raw}}")),
        ]
    });
    report.print(mq, &format!("{} abilities match ${{Me.AbilityReady[name]}} and their reuse time", abilities.len()), all_match(mq, rows));
}

fn merchant_and_loot<P: Plugin>(mq: &Mq<P>, report: &mut Report) {
    // ${Merchant} is NULL with no merchant at all, and Open FALSE while one's window is hidden.
    let open = mq.parse("${Merchant.Open}");
    report.print(mq, "merchant().is_some() = ${Merchant.Open}", match mq.merchant() {
        Some(_) => same_text("TRUE", open),
        None => Check::that(open == "NULL" || open == "FALSE", || format!("no merchant in mq-emu, MacroQuest says {open:?}")),
    });
    match mq.merchant() {
        Some(merchant) => {
            let items = merchant.items();
            report.print(mq, "merchant items = ${Merchant.Items}", same_text(items.capacity(), mq.parse("${Merchant.Items}")));
            let rows = (0..items.capacity()).map(|i| (format!("item {}", i + 1), id_or_null(items.get(i).map(|item| item.id())), format!("${{Merchant.Item[{}].ID}}", i + 1)));
            report.print(mq, "merchant items match ${Merchant.Item[n].ID}", all_match(mq, rows));
            report.print(mq, "merchant().spawn() = ${Merchant.ID}", same_text(merchant.spawn().id(), mq.parse("${Merchant.ID}")));
        }
        None => report.print(mq, "merchant items", Check::Skip("no merchant window open".into())),
    }
    report.print(mq, "loot_window().is_some() = ${Corpse.Open}", same_text(tf(mq.loot_window().is_some()), mq.parse("${Corpse.Open}")));
    match mq.loot_window() {
        Some(loot) => {
            report.print(mq, "loot_window().len() = ${Corpse.Items}", same_text(loot.len(), mq.parse("${Corpse.Items}")));
            let items = loot.items();
            let rows = (0..items.capacity()).map(|i| (format!("loot slot {}", i + 1), id_or_null(items.get(i).map(|item| item.id())), format!("${{Corpse.Item[{}].ID}}", i + 1)));
            report.print(mq, "loot slots match ${Corpse.Item[n].ID}", all_match(mq, rows));
        }
        None => report.print(mq, "loot items", Check::Skip("not looting a corpse".into())),
    }
}
