//! Item slot windows: what one shows (`${Window.InvSlot}`), and finding the
//! one for a place in your inventory, bags, bank, a corpse, the trade window
//! or a world container, as `/itemnotify` does.

use std::fmt;

use mq_emu_sys::constants::{NUM_BANK_SLOTS, NUM_SHAREDBANK_SLOTS};
use mq_emu_sys::layouts::{CInvSlotMgr, CInvSlotWnd, ItemContainerInstance, ItemGlobalIndex};

use super::{Window, WindowKind, long_text};
use crate::inventory::InvSlot;
use crate::items::{Item, ItemLocation};
use crate::mq::Mq;
use crate::plugin::Plugin;

/// Which of the game's item containers a place is in (`ItemContainerInstance`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ItemContainerKind {
    /// What you carry: worn slots, bags and their contents, the cursor.
    Inventory,
    Bank,
    SharedBank,
    /// The trade window: your offer in slots 0 to 7, the other trader's after.
    Trade,
    /// An open world container, such as a forge (`/itemnotify enviro1`).
    World,
    Limbo,
    Merchant,
    /// The corpse you're looting (`/itemnotify loot1`).
    Corpse,
    Bazaar,
    Inspect,
    /// Another `ItemContainerInstance` value.
    Other(i32),
}

impl ItemContainerKind {
    pub fn from_raw(raw: i32) -> ItemContainerKind {
        use ItemContainerKind::*;
        match ItemContainerInstance(raw) {
            ItemContainerInstance::eItemContainerPossessions => Inventory,
            ItemContainerInstance::eItemContainerBank => Bank,
            ItemContainerInstance::eItemContainerSharedBank => SharedBank,
            ItemContainerInstance::eItemContainerTrade => Trade,
            ItemContainerInstance::eItemContainerWorld => World,
            ItemContainerInstance::eItemContainerLimbo => Limbo,
            ItemContainerInstance::eItemContainerMerchant => Merchant,
            ItemContainerInstance::eItemContainerCorpse => Corpse,
            ItemContainerInstance::eItemContainerBazaar => Bazaar,
            ItemContainerInstance::eItemContainerInspect => Inspect,
            _ => Other(raw),
        }
    }

    /// The `ItemContainerInstance` value (`${Window.InvSlot.ItemLocation}`).
    pub fn raw(self) -> i32 {
        use ItemContainerKind::*;
        let raw = match self {
            Inventory => ItemContainerInstance::eItemContainerPossessions,
            Bank => ItemContainerInstance::eItemContainerBank,
            SharedBank => ItemContainerInstance::eItemContainerSharedBank,
            Trade => ItemContainerInstance::eItemContainerTrade,
            World => ItemContainerInstance::eItemContainerWorld,
            Limbo => ItemContainerInstance::eItemContainerLimbo,
            Merchant => ItemContainerInstance::eItemContainerMerchant,
            Corpse => ItemContainerInstance::eItemContainerCorpse,
            Bazaar => ItemContainerInstance::eItemContainerBazaar,
            Inspect => ItemContainerInstance::eItemContainerInspect,
            Other(raw) => return raw,
        };
        raw.0
    }
}

/// A place an item can be, as an item slot window shows it: a container, a
/// slot in it and, for a place inside a bag, the slot in the bag.
///
/// For your inventory the slot is an [`InvSlot`] number (a bag's contents
/// are `in_bag` of the bag's slot, `InvSlot::BAG1` and on); for the bank,
/// the shared bank, a corpse, the trade window and a world container it
/// counts from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SlotLocation {
    pub container: ItemContainerKind,
    pub slot: usize,
    pub in_bag: Option<usize>,
}

impl SlotLocation {
    pub fn new(container: ItemContainerKind, slot: usize, in_bag: Option<usize>) -> SlotLocation {
        SlotLocation { container, slot, in_bag }
    }

    /// A place in your inventory, as [`Inventory::find`](crate::Inventory::find) gives it.
    pub fn inventory(location: ItemLocation) -> SlotLocation {
        SlotLocation { container: ItemContainerKind::Inventory, slot: location.slot, in_bag: location.in_bag }
    }

    /// A worn slot, a bag slot or the cursor.
    pub fn inv_slot(slot: InvSlot) -> SlotLocation {
        SlotLocation::new(ItemContainerKind::Inventory, usize::from(slot.0), None)
    }

    /// The place `/itemnotify` names with the slot part of its arguments
    /// (everything before the notification), ignoring case:
    ///
    /// - an inventory slot by number (0 to 32) or by MacroQuest's name
    ///   ("mainhand", "pack3", ...; see [`InvSlot::from_name`]);
    /// - `pack<n>`, `bank<n>`, `sharedbank<n>`, `loot<n>`, `trade<n>` and
    ///   `enviro<n>`, counting from 1;
    /// - `in <bag> <n>`, slot n (from 1) inside a bag in `pack<n>`,
    ///   `bank<n>` or `sharedbank<n>`.
    ///
    /// `/itemnotify` also takes an item's name or `#<id>` and clicks the
    /// slot it's in; look the item up instead ([`Inventory::find_by_name`](crate::Inventory::find_by_name))
    /// and use [`inventory`](Self::inventory).
    pub fn parse(text: &str) -> Option<SlotLocation> {
        let words: Vec<&str> = text.split_whitespace().collect();
        match words.as_slice() {
            [word, bag, slot] if word.eq_ignore_ascii_case("in") => SlotLocation::in_bag_named(bag, slot),
            [name] => SlotLocation::named(name),
            _ => None,
        }
    }

    fn named(name: &str) -> Option<SlotLocation> {
        if !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()) {
            // Numbers are inventory slots; the cursor isn't one.
            return name.parse::<u8>().ok().filter(|slot| *slot < InvSlot::CURSOR.0).map(|slot| SlotLocation::inv_slot(InvSlot(slot)));
        }
        if let Some(slot) = InvSlot::from_name(name) {
            return Some(SlotLocation::inv_slot(slot));
        }
        use ItemContainerKind::*;
        // `/itemnotify`'s order: "sharedbank1" doesn't start with "bank".
        let prefixes = [("loot", Corpse), ("enviro", World), ("pack", Inventory), ("bank", Bank), ("sharedbank", SharedBank), ("trade", Trade)];
        let (rest, container) = prefixes.iter().find_map(|(prefix, kind)| strip_prefix_ignore_case(name, prefix).map(|rest| (rest, *kind)))?;
        let slot = one_based(rest)?;
        // Bags are inventory slots from BAG1 on.
        let slot = if container == Inventory { usize::from(InvSlot::BAG1.0) + slot } else { slot };
        Some(SlotLocation::new(container, slot, None))
    }

    fn in_bag_named(bag: &str, slot: &str) -> Option<SlotLocation> {
        use ItemContainerKind::*;
        let in_bag = Some(one_based(slot)?);
        // `/itemnotify in` checks "bank" before "sharedbank", and only these three.
        let containers = [("bank", Bank, NUM_BANK_SLOTS), ("sharedbank", SharedBank, NUM_SHAREDBANK_SLOTS), ("pack", Inventory, InvSlot::bags().count())];
        let (rest, container, count) =
            containers.iter().find_map(|(prefix, kind, count)| strip_prefix_ignore_case(bag, prefix).map(|rest| (rest, *kind, *count)))?;
        let index = one_based(rest).filter(|index| *index < count)?;
        let slot = if container == Inventory { usize::from(InvSlot::BAG1.0) + index } else { index };
        Some(SlotLocation::new(container, slot, in_bag))
    }

    fn from_global(index: &ItemGlobalIndex) -> Option<SlotLocation> {
        let slots = index.Index().m_slots();
        let slot = usize::try_from(slots[0]).ok()?;
        Some(SlotLocation { container: ItemContainerKind::from_raw(index.Location().0), slot, in_bag: usize::try_from(slots[1]).ok() })
    }

    /// Whether `index` is this place, as the game's `FindInvSlot` compares
    /// them: container, slot and slot in the bag (not deeper).
    fn matches(&self, index: &ItemGlobalIndex) -> bool {
        let slots = index.Index().m_slots();
        let in_bag = self.in_bag.map_or(Some(-1), |slot| i16::try_from(slot).ok());
        index.Location().0 == self.container.raw() && i16::try_from(self.slot).ok() == Some(slots[0]) && in_bag == Some(slots[1])
    }
}

fn strip_prefix_ignore_case<'t>(text: &'t str, prefix: &str) -> Option<&'t str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| &text[prefix.len()..])
}

/// A 1-based number as a 0-based index.
fn one_based(text: &str) -> Option<usize> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<usize>().ok()?.checked_sub(1)
}

impl fmt::Display for SlotLocation {
    /// As `/itemnotify` takes it, where it has a name for it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ItemContainerKind::*;
        let bag_slot = usize::from(InvSlot::BAG1.0);
        let named = |f: &mut fmt::Formatter<'_>| -> fmt::Result {
            match self.container {
                Inventory if self.slot >= bag_slot && self.slot < bag_slot + InvSlot::bags().count() => {
                    write!(f, "pack{}", self.slot - bag_slot + 1)
                }
                Inventory => match u8::try_from(self.slot).ok().and_then(|slot| InvSlot(slot).name()) {
                    Some(name) => f.write_str(name),
                    None => write!(f, "{}", self.slot),
                },
                Bank => write!(f, "bank{}", self.slot + 1),
                SharedBank => write!(f, "sharedbank{}", self.slot + 1),
                Corpse => write!(f, "loot{}", self.slot + 1),
                Trade => write!(f, "trade{}", self.slot + 1),
                World => write!(f, "enviro{}", self.slot + 1),
                other => write!(f, "{other:?} {}", self.slot),
            }
        };
        match self.in_bag {
            None => named(f),
            Some(in_bag) => {
                f.write_str("in ")?;
                named(f)?;
                write!(f, " {}", in_bag + 1)
            }
        }
    }
}

/// An item slot window (`${Window.InvSlot}`): a place an item can be
/// (inventory, bag, bank, loot, trade...) or, on a hot button, a linked item.
#[derive(Clone, Copy)]
pub struct ItemSlotWindow<'a> {
    window: Window<'a>,
    slot: &'a CInvSlotWnd,
}

impl<'a> ItemSlotWindow<'a> {
    pub fn window(&self) -> Window<'a> {
        self.window
    }

    pub fn raw(&self) -> &'a CInvSlotWnd {
        self.slot
    }

    fn ptr(&self) -> *mut CInvSlotWnd {
        std::ptr::from_ref(self.slot).cast_mut()
    }

    /// The place it shows (`${Window.InvSlot.ItemLocation}` is its
    /// container); `None` for a slot with no place, such as a hot button's.
    pub fn location(&self) -> Option<SlotLocation> {
        SlotLocation::from_global(self.slot.ItemLocation())
    }

    /// The item it shows (`${Window.InvSlot.Item}`): the item it's linked
    /// to, or the one at its place.
    pub fn item(&self) -> Option<Item<'a>> {
        // SAFETY: the item is held by its container or by the slot, which
        // outlive the callback.
        unsafe { Item::from_raw((self.window.api.mqemu_inv_slot_item)(self.ptr())) }
    }

    /// The stack count it draws (`${Window.InvSlot.Quantity}`).
    pub fn quantity(&self) -> i32 {
        self.slot.Quantity()
    }

    /// Whether it's the selected slot, as when an item is picked for a
    /// merchant (`${Window.InvSlot.Selected}`).
    pub fn is_selected(&self) -> bool {
        self.slot.bSelected()
    }

    /// Whether the find item window highlights it (`${Window.InvSlot.FindSelected}`).
    pub fn is_find_selected(&self) -> bool {
        self.slot.bFindSelected()
    }

    /// Whether it's a hot button's slot (`${Window.InvSlot.HotButton}`).
    pub fn is_hot_button(&self) -> bool {
        self.slot.bHotButton()
    }

    /// `${Window.InvSlot.InventorySlotLinked}`.
    pub fn is_inventory_slot_linked(&self) -> bool {
        self.slot.bInventorySlotLinked()
    }

    /// `${Window.InvSlot.Mode}`.
    pub fn mode(&self) -> i32 {
        self.slot.Mode()
    }

    /// Where it draws its item, from its top left corner
    /// (`${Window.InvSlot.ItemOffsetX}` and `.ItemOffsetY`).
    pub fn item_offset(&self) -> (i32, i32) {
        (self.slot.ItemOffsetX(), self.slot.ItemOffsetY())
    }

    /// The text it draws over its item (`${Window.InvSlot.Text}`).
    pub fn text(&self) -> String {
        long_text(|out, cap| unsafe { (self.window.api.mqemu_inv_slot_text)(self.ptr(), out, cap) }).unwrap_or_default()
    }

    /// The name of the texture it draws when empty (`${Window.InvSlot.Background}`).
    pub fn background(&self) -> Option<String> {
        let texture = self.slot.pBackground();
        long_text(|out, cap| unsafe { (self.window.api.mqemu_texture_animation_name)(texture, out, cap) })
    }

    /// The name of its item's icon texture.
    pub fn item_texture(&self) -> Option<String> {
        let texture = self.slot.ptItem();
        long_text(|out, cap| unsafe { (self.window.api.mqemu_texture_animation_name)(texture, out, cap) })
    }
}

impl fmt::Debug for ItemSlotWindow<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ItemSlotWindow").field("location", &self.location()).field("item", &self.item()).finish()
    }
}

impl<'a> Window<'a> {
    /// An item slot (`${Window.InvSlot}`).
    pub fn as_item_slot(&self) -> Option<ItemSlotWindow<'a>> {
        (self.kind() == WindowKind::InvSlot).then(|| ItemSlotWindow { window: *self, slot: self.cast::<CInvSlotWnd>() })
    }
}

impl<P: Plugin> Mq<P> {
    fn item_slot_manager(&self) -> Option<&CInvSlotMgr> {
        unsafe { (self.host().inv_slot_manager)().as_ref() }
    }

    /// Every item slot window the game has made, shown or not, as its slot
    /// manager lists them.
    pub fn item_slot_windows(&self) -> impl Iterator<Item = ItemSlotWindow<'_>> + '_ {
        let api = self.window_api();
        let manager = self.item_slot_manager();
        let count = manager.map_or(0, |manager| usize::try_from(manager.TotalSlots()).unwrap_or(0).min(mq_emu_sys::constants::MAX_INV_SLOTS));
        (0..count).filter_map(move |i| {
            // SAFETY: the manager's slots, and their windows, are live or null.
            let slot = unsafe { manager?.SlotArray_at(i).as_ref() }?;
            unsafe { Window::from_raw(slot.pInvSlotWnd().cast(), api) }?.as_item_slot()
        })
    }

    /// The item slot window for a place, as `/itemnotify` finds it (the
    /// game's `FindInvSlot`), leaving out hot buttons' slots: the one to
    /// [`click`](Window::click) to pick up, drop or use what's there. `None`
    /// if no window shows the place, such as a slot in a closed bag.
    pub fn item_slot_window(&self, location: SlotLocation) -> Option<ItemSlotWindow<'_>> {
        self.item_slot_windows().find(|slot| !slot.is_hot_button() && location.matches(slot.slot.ItemLocation()))
    }
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::constants::{UI_BUTTON, UI_INV_SLOT};
    use mq_emu_sys::layouts::CTextureAnimation;

    use super::*;
    use crate::items::tests::{Fake, FakeItem};
    use crate::windows::fake;

    fn at(container: ItemContainerKind, slot: usize, in_bag: Option<usize>) -> Option<SlotLocation> {
        Some(SlotLocation::new(container, slot, in_bag))
    }

    #[test]
    fn container_kinds_round_trip() {
        for raw in -2..45 {
            assert_eq!(ItemContainerKind::from_raw(raw).raw(), raw);
        }
        assert_eq!(ItemContainerKind::from_raw(0), ItemContainerKind::Inventory);
        assert_eq!(ItemContainerKind::from_raw(11), ItemContainerKind::Corpse);
        assert_eq!(ItemContainerKind::from_raw(-1), ItemContainerKind::Other(-1), "eItemContainerInvalid");
    }

    #[test]
    fn itemnotify_slots_parse_as_it_reads_them() {
        use ItemContainerKind::*;
        let cases = [
            ("13", at(Inventory, 13, None)),
            ("0", at(Inventory, 0, None)),
            ("33", None),
            ("MainHand", at(Inventory, 13, None)),
            ("cursor", at(Inventory, 33, None)),
            ("pack1", at(Inventory, 23, None)),
            ("PACK10", at(Inventory, 32, None)),
            ("pack11", at(Inventory, 33, None)),
            ("bank3", at(Bank, 2, None)),
            ("sharedbank1", at(SharedBank, 0, None)),
            ("loot31", at(Corpse, 30, None)),
            ("trade9", at(Trade, 8, None)),
            ("enviro2", at(World, 1, None)),
            ("in pack2 5", at(Inventory, 24, Some(4))),
            ("IN Bank24 1", at(Bank, 23, Some(0))),
            ("in sharedbank2 10", at(SharedBank, 1, Some(9))),
        ];
        for (text, expected) in cases {
            assert_eq!(SlotLocation::parse(text), expected, "{text:?}");
        }
    }

    #[test]
    fn what_itemnotify_finds_no_slot_for_doesnt_parse() {
        for text in [
            "", "enviro", "loot0", "bankx", "bank-1", "pack", "#1234", "Rusty Sword", "in pack1", "in pack11 1", "in bank25 1",
            "in sharedbank3 1", "in loot1 1", "in pack1 0", "out pack1 1", "in pack1 1 2",
        ] {
            assert_eq!(SlotLocation::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn locations_display_as_itemnotify_names() {
        for text in ["mainhand", "charm", "cursor", "pack3", "bank24", "sharedbank2", "loot5", "trade16", "enviro1", "in pack10 40", "in bank1 2"] {
            let location = SlotLocation::parse(text).unwrap();
            assert_eq!(location.to_string(), text);
        }
        assert_eq!(SlotLocation::new(ItemContainerKind::Inventory, 99, None).to_string(), "99");
        assert_eq!(SlotLocation::new(ItemContainerKind::Bazaar, 4, None).to_string(), "Bazaar 4");
        assert_eq!(SlotLocation::inventory(ItemLocation { slot: 24, in_bag: Some(1) }).to_string(), "in pack2 2");
    }

    fn set_location(slot: &CInvSlotWnd, container: i32, slots: [i16; 3]) {
        slot.ItemLocation().set_Location(ItemContainerInstance(container));
        slot.ItemLocation().Index().set_m_slots(slots);
    }

    #[test]
    fn a_slot_matches_places_as_find_inv_slot_compares_them() {
        let slot = Fake::<CInvSlotWnd>::new();
        let index = slot.get().ItemLocation();
        set_location(slot.get(), 0, [24, 3, 7]);
        assert!(SlotLocation::new(ItemContainerKind::Inventory, 24, Some(3)).matches(index), "not deeper than the bag");
        assert!(!SlotLocation::new(ItemContainerKind::Inventory, 24, None).matches(index));
        assert!(!SlotLocation::new(ItemContainerKind::Bank, 24, Some(3)).matches(index));
        set_location(slot.get(), 1, [5, -1, -1]);
        assert!(SlotLocation::new(ItemContainerKind::Bank, 5, None).matches(index));
        assert!(!SlotLocation::new(ItemContainerKind::Bank, 5, Some(0)).matches(index));
        assert!(!SlotLocation::new(ItemContainerKind::Bank, 70000, None).matches(index), "past what the game counts");
    }

    #[test]
    fn an_item_slot_reads_what_it_shows() {
        fake::reset();
        let slot = Fake::<CInvSlotWnd>::new();
        let background = Fake::<CTextureAnimation>::new();
        let item = FakeItem::new(1001, "Rusty Sword");
        fake::add(slot.ptr().cast(), UI_INV_SLOT, None, Some("InvSlot13"));
        fake::with(|w| {
            w.items.insert(slot.ptr() as usize, item.item.ptr() as usize);
            w.texts.insert(slot.ptr() as usize, "20".into());
            w.texts.insert(background.ptr() as usize, "A_InvPrimary".into());
        });
        let raw = slot.get();
        set_location(raw, 0, [13, -1, -1]);
        raw.set_Quantity(20);
        raw.set_bSelected(true);
        raw.set_ItemOffsetX(2);
        raw.set_ItemOffsetY(3);
        raw.set_Mode(1);
        raw.set_pBackground(background.ptr());

        let window = unsafe { Window::from_raw(slot.ptr().cast(), &fake::API) }.unwrap();
        let slot_window = window.as_item_slot().unwrap();
        assert_eq!(slot_window.location(), Some(SlotLocation::inv_slot(InvSlot::PRIMARY)));
        assert_eq!(slot_window.item().map(|i| i.name()).as_deref(), Some("Rusty Sword"));
        assert_eq!((slot_window.quantity(), slot_window.item_offset(), slot_window.mode()), (20, (2, 3), 1));
        assert!(slot_window.is_selected() && !slot_window.is_find_selected() && !slot_window.is_hot_button() && !slot_window.is_inventory_slot_linked());
        assert_eq!(slot_window.text(), "20");
        assert_eq!((slot_window.background().as_deref(), slot_window.item_texture()), (Some("A_InvPrimary"), None));
        set_location(raw, -1, [-1, -1, -1]);
        assert_eq!(slot_window.location(), None, "a hot button's slot has no place");
        fake::add(slot.ptr().cast(), UI_BUTTON, None, None);
        assert!(window.as_item_slot().is_none(), "only item slots");
    }
}
