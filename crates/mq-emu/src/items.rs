//! Items: what's in your inventory, bags, bank and on the ground.
//!
//! An [`Item`] is one instance (a stack of arrows, a sword); what it *is*
//! (name, stats, effects) comes from its definition, shared by every
//! instance. Like a [`Spawn`](crate::Spawn), an `Item` borrows from the
//! callback's [`Mq`](crate::Mq) and can't be kept past it; keep the item's
//! [`id`](Item::id) or its location instead.

use std::fmt;

use mq_emu_sys::constants::ITEMTYPE_PACK;
use mq_emu_sys::layouts::{ItemClient, ItemContainer, ItemDefinition, ItemSpellData_SpellData};

use crate::spells::SpellId;
use crate::text::from_c_array;

/// The most slots [`ItemSlots`] reads from one container. Far above any real
/// container (the inventory has 34); it only guarantees a damaged size can't
/// send a walk through all of memory.
pub const MAX_CONTAINER_SLOTS: usize = 1024;

/// One of an item's spell effects (`ItemDefinition::SpellData`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemEffect {
    /// Right-click effect.
    Clicky,
    /// Weapon proc.
    Proc,
    /// Always-on worn effect.
    Worn,
    /// Focus effect.
    Focus,
    /// The spell a scroll scribes.
    Scroll,
    /// Second focus effect.
    Focus2,
}

impl ItemEffect {
    pub const ALL: [ItemEffect; 6] =
        [ItemEffect::Clicky, ItemEffect::Proc, ItemEffect::Worn, ItemEffect::Focus, ItemEffect::Scroll, ItemEffect::Focus2];

    /// Index into `ItemSpellData::Spells` (`ItemSpellTypes`).
    fn index(self) -> usize {
        self as usize
    }
}

/// A spell effect on an item, e.g. its clicky.
#[derive(Clone, Copy)]
pub struct ItemSpell<'a> {
    raw: &'a ItemSpellData_SpellData,
}

impl<'a> ItemSpell<'a> {
    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a ItemSpellData_SpellData {
        self.raw
    }

    pub fn spell_id(&self) -> SpellId {
        SpellId(self.raw.SpellID())
    }

    /// Level needed to use the effect.
    pub fn required_level(&self) -> u8 {
        self.raw.RequiredLevel()
    }

    /// Cast time in milliseconds.
    pub fn cast_time_ms(&self) -> i32 {
        self.raw.CastTime()
    }

    /// Recast time in seconds.
    pub fn recast_time(&self) -> i32 {
        self.raw.RecastTime()
    }

    /// Effects that share a recast timer have the same type.
    pub fn recast_type(&self) -> i32 {
        self.raw.RecastType()
    }

    /// Charges a new item has; -1 for unlimited.
    pub fn max_charges(&self) -> i32 {
        self.raw.MaxCharges()
    }
}

impl fmt::Debug for ItemSpell<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ItemSpell").field("spell_id", &self.spell_id()).finish()
    }
}

/// An item instance for the duration of the current callback.
#[derive(Clone, Copy)]
pub struct Item<'a> {
    raw: &'a ItemClient,
    definition: &'a ItemDefinition,
}

impl<'a> Item<'a> {
    /// `None` if `raw` is null or has no definition (the game hasn't loaded it).
    ///
    /// # Safety
    /// `raw` must be null or point to a live item for `'a`, which must not
    /// extend past the current callback.
    pub unsafe fn from_raw(raw: *const ItemClient) -> Option<Item<'a>> {
        let raw = unsafe { raw.as_ref() }?;
        // The virtual GetItemDefinition, as MacroQuest calls it: for items the
        // game created, the game's implementation, which doesn't read the
        // Item1/SharedItemDef fields eqlib's inline version does.
        let definition = unsafe { mq_emu_sys::shim::mqemu_item_definition(raw).as_ref() }?;
        Some(Item { raw, definition })
    }

    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a ItemClient {
        self.raw
    }

    /// The item's definition, for fields this API doesn't wrap yet.
    pub fn definition(&self) -> &'a ItemDefinition {
        self.definition
    }

    /// The item number, the same for every instance (`${Item.ID}`).
    pub fn id(&self) -> i32 {
        self.definition.ItemNumber()
    }

    pub fn name(&self) -> String {
        from_c_array(&self.definition.Name())
    }

    /// How many are in this stack: 1 for items that don't stack (`${Item.Stack}`).
    pub fn stack(&self) -> i32 {
        if self.is_stackable() { self.raw.StackCount() } else { 1 }
    }

    /// The most that fit in one stack.
    pub fn max_stack(&self) -> i32 {
        self.definition.StackSize()
    }

    pub fn is_stackable(&self) -> bool {
        self.definition.StackSize() > 1
    }

    /// Charges left on its effect; -1 for unlimited.
    pub fn charges(&self) -> i32 {
        self.raw.Charges()
    }

    /// The raw item type (`ItemDefinition::Type`): 0 normal, 1 container, 2 book.
    pub fn item_type(&self) -> u8 {
        self.definition.Type()
    }

    /// The raw item class (`eItemClass`): weapon kind, armor, food...
    pub fn item_class(&self) -> u8 {
        self.definition.ItemClass()
    }

    /// True for bags and other containers.
    pub fn is_container(&self) -> bool {
        self.item_type() == ITEMTYPE_PACK
    }

    /// Slots inside, for containers.
    pub fn container_slots(&self) -> u8 {
        self.definition.Slots()
    }

    /// The largest item size a container holds (0 tiny .. 4 giant).
    pub fn container_capacity(&self) -> u8 {
        self.definition.SizeCapacity()
    }

    /// What's inside: a container's slots, or for other items their augment
    /// sockets. Empty slots are skipped; each comes with its slot index.
    pub fn contents(&self) -> ItemSlots<'a> {
        ItemSlots { raw: Some(self.raw.Contents()) }
    }

    pub fn icon(&self) -> i32 {
        self.definition.IconNumber()
    }

    /// Weight in tenths of a pound.
    pub fn weight(&self) -> i32 {
        self.definition.Weight()
    }

    /// Size: 0 tiny, 1 small, 2 medium, 3 large, 4 giant.
    pub fn size(&self) -> u8 {
        self.definition.Size()
    }

    /// Base merchant value, in copper.
    pub fn value(&self) -> i32 {
        self.definition.Cost()
    }

    pub fn is_lore(&self) -> bool {
        self.definition.Lore() != 0
    }

    /// Can't be traded or dropped.
    pub fn is_no_drop(&self) -> bool {
        !self.definition.IsDroppable()
    }

    /// Disappears when you log out.
    pub fn is_no_rent(&self) -> bool {
        !self.definition.NoRent()
    }

    pub fn is_magic(&self) -> bool {
        self.definition.Magic()
    }

    pub fn required_level(&self) -> i32 {
        self.definition.RequiredLevel()
    }

    pub fn recommended_level(&self) -> i32 {
        self.definition.RecommendedLevel()
    }

    /// Bit `n` set: class `n + 1` can use it (1 = warrior ... 16 = berserker).
    pub fn classes(&self) -> u32 {
        self.definition.Classes() as u32
    }

    /// Bit `n` set: it can be worn in inventory slot `n` (see [`InvSlot`]).
    ///
    /// [`InvSlot`]: crate::InvSlot
    pub fn equip_slots(&self) -> u32 {
        self.definition.EquipSlots() as u32
    }

    pub fn ac(&self) -> i32 {
        self.definition.AC()
    }

    pub fn hp(&self) -> i32 {
        self.definition.HP()
    }

    pub fn mana(&self) -> i32 {
        self.definition.Mana()
    }

    pub fn endurance(&self) -> i32 {
        self.definition.Endurance()
    }

    /// Weapon damage.
    pub fn damage(&self) -> i32 {
        self.definition.Damage()
    }

    /// Weapon delay.
    pub fn delay(&self) -> u8 {
        self.definition.Delay()
    }

    /// Attack speed bonus, in percent.
    pub fn haste(&self) -> i32 {
        self.definition.Haste()
    }

    /// One of the item's spell effects, if it has that one.
    pub fn effect(&self, which: ItemEffect) -> Option<ItemSpell<'a>> {
        let raw = self.definition.SpellData().Spells(which.index());
        // eqlib leaves unused effects at -1 (and zeroed records at 0).
        (raw.SpellID() > 0).then_some(ItemSpell { raw })
    }
}

impl fmt::Debug for Item<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Item").field("id", &self.id()).field("name", &self.name()).field("stack", &self.stack()).finish()
    }
}

/// The items in a container (your inventory, a bag, the bank), each with its
/// slot index. Empty slots are skipped.
#[derive(Clone, Copy)]
pub struct ItemSlots<'a> {
    /// None for a missing container: no slots.
    raw: Option<&'a ItemContainer>,
}

impl<'a> ItemSlots<'a> {
    /// # Safety
    /// `raw` must be null or point to a live container for `'a`, which must
    /// not extend past the current callback.
    pub unsafe fn from_raw(raw: *const ItemContainer) -> ItemSlots<'a> {
        ItemSlots { raw: unsafe { raw.as_ref() } }
    }

    /// How many slots the container has, empty or not.
    pub fn capacity(&self) -> usize {
        self.raw.map_or(0, |raw| (raw.m_size() as usize).min(raw.m_items().m_size() as usize).min(MAX_CONTAINER_SLOTS))
    }

    /// The item in `slot`, if any.
    pub fn get(&self, slot: usize) -> Option<Item<'a>> {
        let raw = self.raw?;
        if slot >= self.capacity() {
            return None;
        }
        // SAFETY: slot is within the container's array, whose entries are
        // null or live items for as long as the container is.
        unsafe { Item::from_raw((*raw.m_items().m_data().add(slot)).m_pObject()) }
    }

    /// Every item, with its slot.
    pub fn iter(self) -> impl Iterator<Item = (usize, Item<'a>)> + 'a {
        (0..self.capacity()).filter_map(move |slot| self.get(slot).map(|item| (slot, item)))
    }

    /// Empty slots.
    pub fn free(&self) -> usize {
        self.capacity() - self.iter().count()
    }

    /// Every item, then (right after each container) what's inside it, with
    /// where each is. Augments aren't included.
    pub fn all(self) -> impl Iterator<Item = (ItemLocation, Item<'a>)> + 'a {
        self.iter().flat_map(|(slot, item)| {
            let inside = item.is_container().then(|| item.contents().iter()).into_iter().flatten();
            std::iter::once((ItemLocation { slot, in_bag: None }, item))
                .chain(inside.map(move |(bag_slot, inner)| (ItemLocation { slot, in_bag: Some(bag_slot) }, inner)))
        })
    }

    /// The first item (bag contents included) that `filter` accepts.
    pub fn find(&self, filter: impl Fn(&Item<'_>) -> bool) -> Option<(ItemLocation, Item<'a>)> {
        self.all().find(|(_, item)| filter(item))
    }

    /// The first item with this exact name, ignoring case (`${FindItem[=name]}`).
    pub fn find_by_name(&self, name: &str) -> Option<(ItemLocation, Item<'a>)> {
        self.find(|item| item.name().eq_ignore_ascii_case(name))
    }

    pub fn find_by_id(&self, id: i32) -> Option<(ItemLocation, Item<'a>)> {
        self.find(|item| item.id() == id)
    }

    /// How many of this item there are, adding up stacks (`${FindItemCount[id]}`).
    pub fn count(&self, id: i32) -> i32 {
        self.all().filter(|(_, item)| item.id() == id).map(|(_, item)| item.stack()).sum()
    }
}

impl fmt::Debug for ItemSlots<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.iter()).finish()
    }
}

/// Where an item is in a container: its slot, and for an item inside a bag,
/// the slot within that bag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemLocation {
    pub slot: usize,
    pub in_bag: Option<usize>,
}

#[cfg(test)]
pub(crate) mod tests {
    use std::ffi::c_char;

    use mq_emu_sys::layouts::VePointer_ItemClient;

    use super::*;

    /// Zeroed storage for a game object.
    pub(crate) struct Fake<T> {
        storage: std::cell::UnsafeCell<Vec<u64>>,
        _type: std::marker::PhantomData<T>,
    }

    impl<T> Fake<T> {
        pub(crate) fn new() -> Fake<T> {
            Fake { storage: vec![0; size_of::<T>().div_ceil(8)].into(), _type: std::marker::PhantomData }
        }

        pub(crate) fn ptr(&self) -> *mut T {
            unsafe { (*self.storage.get()).as_mut_ptr() }.cast()
        }

        pub(crate) fn get(&self) -> &T {
            unsafe { &*self.ptr() }
        }
    }

    pub(crate) fn set_name<const N: usize>(name: &str) -> [c_char; N] {
        let mut out = [0 as c_char; N];
        for (d, s) in out.iter_mut().zip(name.bytes()) {
            *d = s as c_char;
        }
        out
    }

    /// Plays the game's ItemClient::GetItemDefinition for fake items: returns Item1.
    unsafe extern "thiscall" fn get_item_definition(this: *const ItemClient) -> *mut ItemDefinition {
        unsafe { (*this).Item1() }
    }

    unsafe extern "thiscall" fn unused(_this: *const ItemClient) {}

    /// A vtable laid out like ItemClient's first three virtuals (getUnknown,
    /// the destructor, GetItemDefinition).
    fn vtable() -> *mut std::ffi::c_void {
        static VTABLE: std::sync::OnceLock<[usize; 3]> = std::sync::OnceLock::new();
        VTABLE.get_or_init(|| [unused as *const () as usize, unused as *const () as usize, get_item_definition as *const () as usize]).as_ptr().cast_mut().cast()
    }

    /// An item and its definition; `items` keeps a container's slot array.
    pub(crate) struct FakeItem {
        pub item: Fake<ItemClient>,
        pub definition: Fake<ItemDefinition>,
        slots: Vec<VePointer_ItemClient>,
    }

    impl FakeItem {
        pub(crate) fn new(id: i32, name: &str) -> FakeItem {
            let fake = FakeItem { item: Fake::new(), definition: Fake::new(), slots: Vec::new() };
            let def = fake.definition.get();
            def.set_ItemNumber(id);
            def.set_Name(set_name(name));
            for effect in ItemEffect::ALL {
                def.SpellData().Spells(effect.index()).set_SpellID(-1);
            }
            fake.item.get().set___vfptr(vtable());
            fake.item.get().set_Item1(fake.definition.ptr());
            fake
        }

        pub(crate) fn stack(self, count: i32, max: i32) -> FakeItem {
            self.definition.get().set_StackSize(max);
            self.item.get().set_StackCount(count);
            self
        }

        /// Makes this a container holding `contents` at their slots.
        pub(crate) fn bag(mut self, size: usize, contents: &[(usize, &FakeItem)]) -> FakeItem {
            self.definition.get().set_Type(ITEMTYPE_PACK);
            self.definition.get().set_Slots(size as u8);
            fill(self.item.get().Contents(), &mut self.slots, size, contents);
            self
        }
    }

    /// Points `container` at `slots`, sized `size`, holding `contents`.
    pub(crate) fn fill(container: &ItemContainer, slots: &mut Vec<VePointer_ItemClient>, size: usize, contents: &[(usize, &FakeItem)]) {
        *slots = (0..size).map(|_| unsafe { std::mem::zeroed() }).collect();
        for (slot, item) in contents {
            slots[*slot].set_m_pObject(item.item.ptr());
        }
        container.set_m_size(size as u32);
        container.m_items().set_m_size(size as u32);
        container.m_items().set_m_capacity(size as u32);
        container.m_items().set_m_data(slots.as_mut_ptr());
    }

    fn item(fake: &FakeItem) -> Item<'_> {
        unsafe { Item::from_raw(fake.item.ptr()) }.unwrap()
    }

    #[test]
    fn items_read_their_definition() {
        let sword = FakeItem::new(1001, "Rusty Sword");
        let def = sword.definition.get();
        def.set_Damage(8);
        def.set_Delay(30);
        def.set_IsDroppable(true);
        def.set_NoRent(true);
        let sword = item(&sword);
        assert_eq!((sword.id(), sword.name().as_str()), (1001, "Rusty Sword"));
        assert_eq!((sword.damage(), sword.delay()), (8, 30));
        assert!(!sword.is_no_drop() && !sword.is_no_rent() && !sword.is_container());
        assert_eq!(sword.stack(), 1, "items that don't stack count as one");
    }

    #[test]
    fn items_without_a_definition_are_none() {
        // The fake game answers from Item1; the item's other fields don't matter.
        let fake = FakeItem::new(7, "Shared");
        fake.item.get().set_Item1(std::ptr::null_mut());
        fake.item.get().SharedItemDef().set_m_ptr(fake.definition.ptr());
        assert!(unsafe { Item::from_raw(fake.item.ptr()) }.is_none());
        assert!(unsafe { Item::from_raw(std::ptr::null()) }.is_none());
    }

    #[test]
    fn stacks_and_effects() {
        let arrows = FakeItem::new(8005, "Arrow").stack(37, 100);
        assert_eq!((item(&arrows).stack(), item(&arrows).max_stack()), (37, 100));
        let def = arrows.definition.get();
        let clicky = def.SpellData().Spells(ItemEffect::Clicky.index());
        clicky.set_SpellID(2558);
        clicky.set_RecastTime(30);
        let effect = item(&arrows).effect(ItemEffect::Clicky).unwrap();
        assert_eq!((effect.spell_id(), effect.recast_time()), (SpellId(2558), 30));
        assert!(item(&arrows).effect(ItemEffect::Worn).is_none());
    }

    #[test]
    fn containers_walk_their_slots_and_bags() {
        let ration = FakeItem::new(13005, "Ration").stack(5, 20);
        let more = FakeItem::new(13005, "Ration").stack(15, 20);
        let water = FakeItem::new(13006, "Water Flask");
        let bag = FakeItem::new(17005, "Backpack").bag(8, &[(0, &ration), (6, &water)]);
        let mut slots = Vec::new();
        let container = Fake::<ItemContainer>::new();
        fill(container.get(), &mut slots, 4, &[(1, &bag), (3, &more)]);
        let inventory = unsafe { ItemSlots::from_raw(container.ptr()) };

        assert_eq!(inventory.capacity(), 4);
        assert_eq!(inventory.free(), 2);
        let found: Vec<(ItemLocation, String)> = inventory.all().map(|(at, item)| (at, item.name())).collect();
        let at = |slot, in_bag| ItemLocation { slot, in_bag };
        assert_eq!(
            found,
            [
                (at(1, None), "Backpack".to_owned()),
                (at(1, Some(0)), "Ration".to_owned()),
                (at(1, Some(6)), "Water Flask".to_owned()),
                (at(3, None), "Ration".to_owned()),
            ]
        );
        assert_eq!(inventory.find_by_name("WATER FLASK").map(|(at, _)| at), Some(at(1, Some(6))));
        assert_eq!(inventory.find_by_id(99).map(|(at, _)| at), None);
        assert_eq!(inventory.count(13005), 20);
        assert_eq!(inventory.get(1).unwrap().contents().free(), 6);
        assert!(inventory.get(0).is_none() && inventory.get(4).is_none() && inventory.get(usize::MAX).is_none());
    }

    #[test]
    fn missing_or_damaged_containers_are_bounded() {
        let none = unsafe { ItemSlots::from_raw(std::ptr::null()) };
        assert_eq!((none.capacity(), none.all().count()), (0, 0));
        // A size bigger than the array behind it reads only the array.
        let mut slots = Vec::new();
        let container = Fake::<ItemContainer>::new();
        fill(container.get(), &mut slots, 2, &[]);
        container.get().set_m_size(u32::MAX);
        assert_eq!(unsafe { ItemSlots::from_raw(container.ptr()) }.capacity(), 2);
    }
}
