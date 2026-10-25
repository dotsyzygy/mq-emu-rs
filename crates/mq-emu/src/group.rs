//! Your group.

use std::fmt;

use mq_emu_sys::constants::MAX_GROUP_SIZE;
use mq_emu_sys::layouts::{CGroup, CGroupMember};

use crate::game::Spawn;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::from_cxstr;

/// A role a group member can be assigned (`eGroupRoles`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GroupRole {
    MainTank,
    MainAssist,
    Puller,
    MarkNpc,
    MasterLooter,
}

impl GroupRole {
    pub const ALL: [GroupRole; 5] =
        [GroupRole::MainTank, GroupRole::MainAssist, GroupRole::Puller, GroupRole::MarkNpc, GroupRole::MasterLooter];

    /// Bit in `CGroupMemberBase::CurrentRoleBits`.
    fn bit(self) -> u32 {
        1 << self as u32
    }
}

/// Your group, for the duration of the current callback.
#[derive(Clone, Copy)]
pub struct Group<'a> {
    raw: &'a CGroup,
}

impl<'a> Group<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a CGroup {
        self.raw
    }

    /// The members, you included, with their group slots (you are slot 0).
    pub fn members(self) -> impl Iterator<Item = GroupMember<'a>> + 'a {
        let raw = self.raw;
        // SAFETY: members are null or live while the group is.
        (0..MAX_GROUP_SIZE).filter_map(move |slot| unsafe { raw.m_groupMembers_at(slot).as_ref() }.map(|raw| GroupMember { raw, slot }))
    }

    /// The member in group slot `slot` (0 is you; see [`GroupMember::slot`]).
    pub fn member(&self, slot: usize) -> Option<GroupMember<'a>> {
        self.members().find(|member| member.slot == slot)
    }

    /// The member with this name, ignoring case.
    pub fn member_by_name(&self, name: &str) -> Option<GroupMember<'a>> {
        self.members().find(|member| member.name().eq_ignore_ascii_case(name))
    }

    /// How many are in the group, you included (`${Group}` counts the others).
    pub fn len(&self) -> usize {
        self.members().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn leader(&self) -> Option<GroupMember<'a>> {
        let leader = self.raw.m_groupLeader();
        self.members().find(|member| std::ptr::eq(member.raw, leader))
    }

    /// The member holding `role`, if anyone does.
    pub fn with_role(&self, role: GroupRole) -> Option<GroupMember<'a>> {
        self.members().find(|member| member.has_role(role))
    }
}

impl fmt::Debug for Group<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.members()).finish()
    }
}

/// A group member. Members out of the zone have no [`spawn`](GroupMember::spawn).
#[derive(Clone, Copy)]
pub struct GroupMember<'a> {
    raw: &'a CGroupMember,
    slot: usize,
}

impl<'a> GroupMember<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a CGroupMember {
        self.raw
    }

    /// Slot in the client's group array: 0 is you. `${Group.Member[n]}`
    /// counts filled slots only, so it differs where earlier slots are empty.
    pub fn slot(&self) -> usize {
        self.slot
    }

    pub fn name(&self) -> String {
        from_cxstr(self.raw.Name())
    }

    pub fn level(&self) -> i32 {
        self.raw.Level()
    }

    /// A mercenary rather than a player.
    pub fn is_mercenary(&self) -> bool {
        self.raw.Type() == 1
    }

    /// For a mercenary, the player who owns it.
    pub fn owner_name(&self) -> String {
        from_cxstr(self.raw.OwnerName())
    }

    /// Logged out (linkdead or camped) but still in the group.
    pub fn is_offline(&self) -> bool {
        self.raw.bIsOffline()
    }

    pub fn has_role(&self, role: GroupRole) -> bool {
        self.raw.CurrentRoleBits() & role.bit() != 0
    }

    /// Their spawn, if they're in your zone.
    pub fn spawn(&self) -> Option<Spawn<'a>> {
        // SAFETY: the client keeps the pointer current as members zone.
        unsafe { Spawn::from_raw(self.raw.pPlayer()) }
    }
}

impl fmt::Debug for GroupMember<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroupMember").field("slot", &self.slot).field("name", &self.name()).finish()
    }
}

impl<P: Plugin> Mq<P> {
    /// Your group, if you're in one.
    pub fn group(&self) -> Option<Group<'_>> {
        let raw = unsafe { self.local_pc()?.Group().as_ref() }?;
        Some(Group { raw })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::tests::Fake;
    use crate::text::tests::cstrrep;

    #[test]
    fn group_members_roles_and_leader() {
        let group = Fake::<CGroup>::new();
        let (me, bob) = (Fake::<CGroupMember>::new(), Fake::<CGroupMember>::new());
        let (me_name, bob_name) = (cstrrep(0, 2, b"Me"), cstrrep(0, 3, b"Bob"));
        me.get().Name().set_m_data(me_name.as_ptr().cast_mut().cast());
        bob.get().Name().set_m_data(bob_name.as_ptr().cast_mut().cast());
        bob.get().set_CurrentRoleBits(0x1 | 0x10);
        group.get().set_m_groupMembers_at(0, me.ptr());
        group.get().set_m_groupMembers_at(3, bob.ptr());
        group.get().set_m_groupLeader(bob.ptr());

        let group = Group { raw: group.get() };
        let members: Vec<(usize, String)> = group.members().map(|m| (m.slot(), m.name())).collect();
        assert_eq!(members, [(0, "Me".to_owned()), (3, "Bob".to_owned())]);
        assert_eq!(group.len(), 2);
        assert_eq!(group.leader().map(|m| m.name()), Some("Bob".to_owned()));
        assert_eq!(group.member_by_name("BOB").map(|m| m.slot()), Some(3));
        assert!(group.member(1).is_none());
        let bob = group.member(3).unwrap();
        assert!(bob.has_role(GroupRole::MainTank) && bob.has_role(GroupRole::MasterLooter) && !bob.has_role(GroupRole::Puller));
        assert_eq!(group.with_role(GroupRole::MainTank).map(|m| m.slot()), Some(3));
        assert!(group.with_role(GroupRole::MainAssist).is_none());
        assert!(bob.spawn().is_none(), "not in the zone");
    }
}
