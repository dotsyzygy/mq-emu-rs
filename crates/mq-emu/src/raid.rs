//! Your raid.

use std::fmt;

use mq_emu_sys::constants::MAX_RAID_SIZE;
use mq_emu_sys::layouts::{CRaid, RaidMember as RawRaidMember};

use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::from_c_array;

/// `eRaidState::RaidStateInvited`.
const RAID_STATE_INVITED: i32 = 2;

/// Who may loot in a raid (`eRaidLootType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RaidLootType {
    /// The raid leader only.
    LeaderOnly,
    /// The raid leader and the group leaders.
    LeaderAndGroupLeaders,
    /// The raid leader and the members the leader named ([`Raid::looters`]).
    Assigned,
    /// A value this API doesn't know.
    Other(i32),
}

impl From<i32> for RaidLootType {
    fn from(value: i32) -> RaidLootType {
        match value {
            1 => RaidLootType::LeaderOnly,
            2 => RaidLootType::LeaderAndGroupLeaders,
            3 => RaidLootType::Assigned,
            other => RaidLootType::Other(other),
        }
    }
}

/// Your raid, for the duration of the current callback.
#[derive(Clone, Copy)]
pub struct Raid<'a> {
    raw: &'a CRaid,
}

impl<'a> Raid<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a CRaid {
        self.raw
    }

    /// The members, you included, with their raid slots.
    pub fn members(self) -> impl Iterator<Item = RaidMember<'a>> + 'a {
        let raw = self.raw;
        (0..MAX_RAID_SIZE).filter(move |&slot| raw.locations_at(slot) != 0).map(move |slot| RaidMember { raw: raw.raidMembers(slot), slot })
    }

    /// The member in raid slot `slot` (see [`RaidMember::slot`]).
    pub fn member(&self, slot: usize) -> Option<RaidMember<'a>> {
        (slot < MAX_RAID_SIZE && self.raw.locations_at(slot) != 0).then(|| RaidMember { raw: self.raw.raidMembers(slot), slot })
    }

    /// The member with this name, ignoring case.
    pub fn member_by_name(&self, name: &str) -> Option<RaidMember<'a>> {
        self.members().find(|member| member.name().eq_ignore_ascii_case(name))
    }

    /// How many are in the raid, you included (`${Raid.Members}`).
    pub fn len(&self) -> usize {
        self.raw.RaidMemberCount().max(0) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn leader_name(&self) -> String {
        from_c_array(&self.raw.RaidLeaderName())
    }

    /// The leader, found by name as `${Raid.Leader}` finds them.
    pub fn leader(&self) -> Option<RaidMember<'a>> {
        self.member_by_name(&self.leader_name())
    }

    /// The members of group `group` (0-based; `${Raid.Member[n].Group}` counts from 1).
    pub fn group(self, group: usize) -> impl Iterator<Item = RaidMember<'a>> + 'a {
        self.members().filter(move |member| member.group() == Some(group))
    }

    /// The raid's message of the day.
    pub fn motd(&self) -> String {
        from_c_array(&self.raw.RaidMOTD())
    }

    pub fn loot_type(&self) -> RaidLootType {
        RaidLootType::from(self.raw.LootType().0)
    }

    /// The members the leader named as looters, used with
    /// [`RaidLootType::Assigned`] (`${Raid.Looter[n]}`).
    pub fn looters(&self) -> Vec<String> {
        self.raw.RaidLooters().iter().map(|name| from_c_array(name)).filter(|name| !name.is_empty()).collect()
    }

    /// Whether `member` may loot under the raid's loot rules (`${Raid.Member[n].Looter}`).
    pub fn may_loot(&self, member: &RaidMember<'_>) -> bool {
        member.is_raid_leader()
            || match self.loot_type() {
                RaidLootType::LeaderAndGroupLeaders => member.is_group_leader(),
                RaidLootType::Assigned => self.looters().iter().any(|looter| looter.eq_ignore_ascii_case(&member.name())),
                _ => false,
            }
    }

    /// Closed to new members.
    pub fn is_locked(&self) -> bool {
        self.raw.Locked()
    }

    /// The sum of the members' levels (`${Raid.TotalLevels}`).
    pub fn total_levels(&self) -> u32 {
        self.raw.levelTotal()
    }

    /// The members flagged as raid main assists.
    pub fn main_assists(self) -> impl Iterator<Item = RaidMember<'a>> + 'a {
        self.members().filter(RaidMember::is_main_assist)
    }
}

impl fmt::Debug for Raid<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.members()).finish()
    }
}

/// A raid member. Find their spawn with [`Mq::spawn_by_name`], as MacroQuest
/// does; the raid doesn't keep it.
#[derive(Clone, Copy)]
pub struct RaidMember<'a> {
    raw: &'a RawRaidMember,
    slot: usize,
}

impl<'a> RaidMember<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a RawRaidMember {
        self.raw
    }

    /// Slot in the client's raid array (0 to 71). `${Raid.Member[n]}` counts
    /// filled slots only, so it differs where earlier slots are empty.
    pub fn slot(&self) -> usize {
        self.slot
    }

    pub fn name(&self) -> String {
        from_c_array(&self.raw.Name())
    }

    pub fn level(&self) -> u8 {
        self.raw.nLevel()
    }

    /// Class, 1 (warrior) to 16 (berserker).
    pub fn class(&self) -> u8 {
        self.raw.nClass()
    }

    /// Raid group, 0-based, or `None` if not in one (`${Raid.Member[n].Group}`
    /// is this + 1, and 0 when ungrouped).
    pub fn group(&self) -> Option<usize> {
        usize::try_from(self.raw.GroupNumber()).ok()
    }

    pub fn is_raid_leader(&self) -> bool {
        self.raw.RaidLeader()
    }

    pub fn is_group_leader(&self) -> bool {
        self.raw.GroupLeader()
    }

    pub fn is_main_assist(&self) -> bool {
        self.raw.RaidMainAssist()
    }

    /// May mark NPCs for the raid.
    pub fn is_marker(&self) -> bool {
        self.raw.RaidMarker()
    }

    /// The note the leader set on them in the raid window.
    pub fn note(&self) -> String {
        from_c_array(&self.raw.RaidNote())
    }
}

impl fmt::Debug for RaidMember<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RaidMember").field("slot", &self.slot).field("name", &self.name()).finish()
    }
}

impl<P: Plugin> Mq<P> {
    /// Your raid, if you're in one: if it has members, as `${Raid.Members}`
    /// counts them. (The raid's state doesn't say "in a raid" on emulator
    /// servers while you're in one.)
    pub fn raid(&self) -> Option<Raid<'_>> {
        let raw = unsafe { (self.host().raid)().as_ref() }?;
        (raw.RaidMemberCount() > 0).then_some(Raid { raw })
    }

    /// The raid's invited state, as `${Raid.Invited}` reads it. Raid invites
    /// on emulator servers arrive as a Yes/No dialog that doesn't set it, so
    /// to catch an invite, watch for "<name> invites you to join a raid" in
    /// [`Plugin::on_incoming_chat`] (as MQ2AutoAccept does) and answer with
    /// `/raidaccept`.
    pub fn raid_invite_pending(&self) -> bool {
        unsafe { (self.host().raid)().as_ref() }.is_some_and(|raw| raw.raidState().0 == RAID_STATE_INVITED)
    }
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::layouts::eRaidLootType;

    use super::*;
    use crate::items::tests::{Fake, set_name};

    fn join<'a>(raid: &'a CRaid, slot: usize, name: &str, group: i32) -> &'a RawRaidMember {
        raid.set_locations_at(slot, 1);
        let member = raid.raidMembers(slot);
        member.set_Name(set_name(name));
        member.set_GroupNumber(group);
        member
    }

    #[test]
    fn raid_members_groups_and_leader() {
        let fake = Fake::<CRaid>::new();
        let raw = fake.get();
        join(raw, 0, "Ann", 0).set_RaidLeader(true);
        join(raw, 5, "Bob", 0).set_nLevel(60);
        join(raw, 71, "Cy", -1).set_nClass(2);
        raw.set_RaidMemberCount(3);
        raw.set_RaidLeaderName(set_name("ann"));

        let raid = Raid { raw };
        let members: Vec<(usize, String)> = raid.members().map(|m| (m.slot(), m.name())).collect();
        assert_eq!(members, [(0, "Ann".into()), (5, "Bob".into()), (71, "Cy".into())]);
        assert_eq!(raid.len(), 3);
        assert_eq!(raid.leader().map(|m| m.slot()), Some(0), "found by name, ignoring case");
        assert_eq!(raid.group(0).map(|m| m.slot()).collect::<Vec<_>>(), [0, 5]);
        assert_eq!(raid.member(71).map(|m| (m.group(), m.class())), Some((None, 2)), "ungrouped");
        assert_eq!(raid.member_by_name("BOB").map(|m| m.level()), Some(60));
        assert!(raid.member(1).is_none() && raid.member(MAX_RAID_SIZE).is_none());
    }

    #[test]
    fn loot_rules_follow_the_loot_type() {
        let fake = Fake::<CRaid>::new();
        let raw = fake.get();
        join(raw, 0, "Ann", 0).set_RaidLeader(true);
        join(raw, 1, "Bob", 1).set_GroupLeader(true);
        join(raw, 2, "Cy", 1);
        let mut looters = raw.RaidLooters();
        looters[3] = set_name("cy");
        raw.set_RaidLooters(looters);
        let raid = Raid { raw };
        let may_loot = |raid: &Raid<'_>| (0..3).map(|slot| raid.may_loot(&raid.member(slot).unwrap())).collect::<Vec<_>>();

        raw.set_LootType(eRaidLootType(1));
        assert_eq!((raid.loot_type(), may_loot(&raid)), (RaidLootType::LeaderOnly, vec![true, false, false]));
        raw.set_LootType(eRaidLootType(2));
        assert_eq!(may_loot(&raid), [true, true, false]);
        raw.set_LootType(eRaidLootType(3));
        assert_eq!((raid.looters(), may_loot(&raid)), (vec!["cy".to_owned()], vec![true, false, true]));
        raw.set_LootType(eRaidLootType(9));
        assert_eq!((raid.loot_type(), may_loot(&raid)), (RaidLootType::Other(9), vec![true, false, false]));
    }
}
