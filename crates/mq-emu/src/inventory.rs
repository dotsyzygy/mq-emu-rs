//! Your inventory, bank and shared bank.

use std::fmt;

use mq_emu_sys::layouts::{PcClient, PcProfile};

use crate::items::{Item, ItemLocation, ItemSlots};
use crate::mq::Mq;
use crate::plugin::Plugin;

/// The most profile lists [`Mq::inventory`] and friends walk to find the
/// current profile; the client has a handful. Guards against a damaged list.
const MAX_PROFILE_LISTS: usize = 64;

/// An inventory slot (`InvSlot_*`): worn slots, the ten bag slots, then the
/// cursor. Its number is MacroQuest's `${Me.Inventory[n]}` index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InvSlot(pub u8);

impl InvSlot {
    pub const CHARM: InvSlot = InvSlot(0);
    pub const LEFT_EAR: InvSlot = InvSlot(1);
    pub const HEAD: InvSlot = InvSlot(2);
    pub const FACE: InvSlot = InvSlot(3);
    pub const RIGHT_EAR: InvSlot = InvSlot(4);
    pub const NECK: InvSlot = InvSlot(5);
    pub const SHOULDERS: InvSlot = InvSlot(6);
    pub const ARMS: InvSlot = InvSlot(7);
    pub const BACK: InvSlot = InvSlot(8);
    pub const LEFT_WRIST: InvSlot = InvSlot(9);
    pub const RIGHT_WRIST: InvSlot = InvSlot(10);
    pub const RANGE: InvSlot = InvSlot(11);
    pub const HANDS: InvSlot = InvSlot(12);
    pub const PRIMARY: InvSlot = InvSlot(13);
    pub const SECONDARY: InvSlot = InvSlot(14);
    pub const LEFT_FINGER: InvSlot = InvSlot(15);
    pub const RIGHT_FINGER: InvSlot = InvSlot(16);
    pub const CHEST: InvSlot = InvSlot(17);
    pub const LEGS: InvSlot = InvSlot(18);
    pub const FEET: InvSlot = InvSlot(19);
    pub const WAIST: InvSlot = InvSlot(20);
    pub const POWER_SOURCE: InvSlot = InvSlot(21);
    pub const AMMO: InvSlot = InvSlot(22);
    /// The first bag slot (`pack1`); the rest follow.
    pub const BAG1: InvSlot = InvSlot(23);
    /// The last bag slot (`pack10`).
    pub const BAG10: InvSlot = InvSlot(32);
    pub const CURSOR: InvSlot = InvSlot(33);

    /// MacroQuest's names (`szItemSlot`), as `${Me.Inventory[name]}` takes them.
    const NAMES: [&'static str; 34] = [
        "charm", "leftear", "head", "face", "rightear", "neck", "shoulder", "arms", "back", "leftwrist", "rightwrist",
        "ranged", "hands", "mainhand", "offhand", "leftfinger", "rightfinger", "chest", "legs", "feet", "waist",
        "powersource", "ammo", "pack1", "pack2", "pack3", "pack4", "pack5", "pack6", "pack7", "pack8", "pack9", "pack10",
        "cursor",
    ];

    /// Every slot, in order.
    pub fn all() -> impl Iterator<Item = InvSlot> {
        (Self::CHARM.0..=Self::CURSOR.0).map(InvSlot)
    }

    /// Worn slots: charm through ammo.
    pub fn worn() -> impl Iterator<Item = InvSlot> {
        (Self::CHARM.0..=Self::AMMO.0).map(InvSlot)
    }

    /// The ten bag slots.
    pub fn bags() -> impl Iterator<Item = InvSlot> {
        (Self::BAG1.0..=Self::BAG10.0).map(InvSlot)
    }

    /// The slot MacroQuest calls `name` (e.g. "mainhand", "pack3"), ignoring case.
    pub fn from_name(name: &str) -> Option<InvSlot> {
        Self::NAMES.iter().position(|n| n.eq_ignore_ascii_case(name)).map(|i| InvSlot(i as u8))
    }

    /// MacroQuest's name for the slot, e.g. "mainhand".
    pub fn name(self) -> Option<&'static str> {
        Self::NAMES.get(usize::from(self.0)).copied()
    }

    pub fn is_worn(self) -> bool {
        self <= Self::AMMO
    }

    pub fn is_bag(self) -> bool {
        (Self::BAG1..=Self::BAG10).contains(&self)
    }

    fn index(self) -> usize {
        usize::from(self.0)
    }
}

impl fmt::Display for InvSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => f.write_str(name),
            None => write!(f, "slot {}", self.0),
        }
    }
}

/// Your inventory: what you wear, your bags and their contents, and the cursor.
#[derive(Clone, Copy)]
pub struct Inventory<'a> {
    slots: ItemSlots<'a>,
}

impl<'a> Inventory<'a> {
    /// The item in `slot` (for a bag slot, the bag itself).
    pub fn get(&self, slot: InvSlot) -> Option<Item<'a>> {
        self.slots.get(slot.index())
    }

    /// What's on your cursor.
    pub fn cursor(&self) -> Option<Item<'a>> {
        self.get(InvSlot::CURSOR)
    }

    /// What you wear, by slot.
    pub fn worn(self) -> impl Iterator<Item = (InvSlot, Item<'a>)> + 'a {
        InvSlot::worn().filter_map(move |slot| self.get(slot).map(|item| (slot, item)))
    }

    /// Your bags (or other items in bag slots), by slot.
    pub fn bags(self) -> impl Iterator<Item = (InvSlot, Item<'a>)> + 'a {
        InvSlot::bags().filter_map(move |slot| self.get(slot).map(|item| (slot, item)))
    }

    /// Every slot as a container, for its iterators and searches
    /// ([`ItemSlots::all`], [`find`](ItemSlots::find), [`count`](ItemSlots::count)...),
    /// which include bag contents and the cursor.
    pub fn slots(&self) -> ItemSlots<'a> {
        self.slots
    }

    /// The first item anywhere in your inventory that `filter` accepts.
    pub fn find(&self, filter: impl Fn(&Item<'_>) -> bool) -> Option<(ItemLocation, Item<'a>)> {
        self.slots.find(filter)
    }

    /// The first item with this exact name, ignoring case (`${FindItem[=name]}`).
    pub fn find_by_name(&self, name: &str) -> Option<(ItemLocation, Item<'a>)> {
        self.slots.find_by_name(name)
    }

    /// How many of this item you carry, adding up stacks (`${FindItemCount[id]}`).
    pub fn count(&self, id: i32) -> i32 {
        self.slots.count(id)
    }

    /// Empty slots inside your bags (`${Me.FreeInventory}` counts empty bag
    /// slots in the inventory too).
    pub fn free_bag_slots(&self) -> usize {
        self.bags().filter(|(_, bag)| bag.is_container()).map(|(_, bag)| bag.contents().free()).sum()
    }
}

impl fmt::Debug for Inventory<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.slots.iter().map(|(slot, item)| (InvSlot(slot as u8), item))).finish()
    }
}

/// `PcClient::GetCurrentPcProfile`: the profile list the manager marks current.
fn current_profile(pc: &PcClient) -> Option<&PcProfile> {
    let manager = pc.ProfileManager();
    let mut list = manager.pFirst();
    for _ in 0..MAX_PROFILE_LISTS {
        // SAFETY: the lists and their profiles live as long as the PcClient.
        let current = unsafe { list.as_ref() }?;
        if current.ListType() == manager.CurProfileList() {
            return unsafe { current.pFirst().as_ref() };
        }
        list = current.pNext();
    }
    None
}

impl<P: Plugin> Mq<P> {
    /// Your character's data (`pLocalPC`), if in game.
    pub(crate) fn local_pc(&self) -> Option<&PcClient> {
        unsafe { (self.host().local_pc)().as_ref() }
    }

    /// Your current profile: inventory, buffs, spells, stats.
    pub(crate) fn profile(&self) -> Option<&PcProfile> {
        current_profile(self.local_pc()?)
    }

    /// Your inventory, if in game.
    pub fn inventory(&self) -> Option<Inventory<'_>> {
        let profile = self.profile()?;
        Some(Inventory { slots: unsafe { ItemSlots::from_raw(profile.InventoryContainer()) } })
    }

    /// Your bank. The client only knows its contents once you've been to a
    /// banker this session; before that it reads as empty.
    pub fn bank(&self) -> Option<ItemSlots<'_>> {
        Some(unsafe { ItemSlots::from_raw(self.local_pc()?.BankItems()) })
    }

    /// Your shared bank, with the same caveat as [`bank`](Mq::bank).
    pub fn shared_bank(&self) -> Option<ItemSlots<'_>> {
        Some(unsafe { ItemSlots::from_raw(self.local_pc()?.SharedBankItems()) })
    }
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::layouts::{ProfileList, eProfileListType};

    use super::*;
    use crate::items::tests::Fake;

    #[test]
    fn slot_names_and_ranges_match_macroquest() {
        assert_eq!(InvSlot::all().count(), 34);
        assert_eq!(InvSlot::from_name("MainHand"), Some(InvSlot::PRIMARY));
        assert_eq!(InvSlot::from_name("pack10"), Some(InvSlot::BAG10));
        assert_eq!(InvSlot::from_name("cursor"), Some(InvSlot::CURSOR));
        assert_eq!(InvSlot::from_name("pack11"), None, "no Merchant's perk bags in RoF2");
        assert_eq!(InvSlot::SECONDARY.to_string(), "offhand");
        assert_eq!(InvSlot(200).to_string(), "slot 200");
        assert_eq!(InvSlot::worn().count(), 23);
        assert_eq!(InvSlot::bags().collect::<Vec<_>>().len(), 10);
        assert!(InvSlot::AMMO.is_worn() && !InvSlot::BAG1.is_worn() && InvSlot::BAG1.is_bag() && !InvSlot::CURSOR.is_bag());
        for slot in InvSlot::all() {
            assert_eq!(InvSlot::from_name(slot.name().unwrap()), Some(slot));
        }
    }

    #[test]
    fn the_current_profile_is_the_one_its_list_type_names() {
        let pc = Fake::<PcClient>::new();
        assert!(current_profile(pc.get()).is_none(), "no lists");

        let (normal, other) = (Fake::<ProfileList>::new(), Fake::<ProfileList>::new());
        let (normal_profile, other_profile) = (Fake::<PcProfile>::new(), Fake::<PcProfile>::new());
        normal.get().set_ListType(eProfileListType(0));
        normal.get().set_pFirst(normal_profile.ptr());
        normal.get().set_pNext(other.ptr());
        other.get().set_ListType(eProfileListType(1));
        other.get().set_pFirst(other_profile.ptr());
        let manager = pc.get().ProfileManager();
        manager.set_pFirst(normal.ptr());

        manager.set_CurProfileList(eProfileListType(1));
        assert_eq!(current_profile(pc.get()).map(|p| p as *const PcProfile), Some(other_profile.ptr().cast_const()));
        manager.set_CurProfileList(eProfileListType(0));
        assert_eq!(current_profile(pc.get()).map(|p| p as *const PcProfile), Some(normal_profile.ptr().cast_const()));
        manager.set_CurProfileList(eProfileListType(5));
        assert!(current_profile(pc.get()).is_none());
        // A cycle ends at the walk limit instead of hanging.
        other.get().set_pNext(normal.ptr());
        assert!(current_profile(pc.get()).is_none());
    }
}
