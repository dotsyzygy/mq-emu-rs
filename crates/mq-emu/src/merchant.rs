//! The merchant window and the loot window: what's for sale, buying and
//! selling, and what's on the corpse you're looting.

use std::fmt;

use mq_emu_sys::layouts::{CLootWnd, CMerchantWnd, MerchantPageHandler};

use crate::game::Spawn;
use crate::items::{Item, ItemSlots};
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::windows::{ItemContainerKind, SlotLocation};

/// `MerchantPages::RegularMerchantPage`: what's for sale.
const PAGE_FOR_SALE: usize = 0;
/// `MerchantPages::BuybackMerchantPage`: what you sold, to buy back.
const PAGE_BUYBACK: usize = 1;

/// The open merchant window, for the duration of the current callback.
#[derive(Clone, Copy)]
pub struct Merchant<'a> {
    window: &'a CMerchantWnd,
    spawn: Spawn<'a>,
}

impl<'a> Merchant<'a> {
    /// The underlying window, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a CMerchantWnd {
        self.window
    }

    /// The merchant you're trading with.
    pub fn spawn(&self) -> Spawn<'a> {
        self.spawn
    }

    fn page(&self, page: usize) -> Option<&'a MerchantPageHandler> {
        let pages = self.window.PageHandlers();
        if page >= pages.m_size() as usize || pages.m_data().is_null() {
            return None;
        }
        // SAFETY: the window has m_size page pointers, each null or live.
        unsafe { (*pages.m_data().add(page)).as_ref() }
    }

    /// What's for sale, in the window's order: `${Merchant.Item[n]}` is slot
    /// n-1, and [`capacity`](ItemSlots::capacity) is `${Merchant.Items}`.
    /// The server fills it a moment after the window opens.
    pub fn items(&self) -> ItemSlots<'a> {
        unsafe { ItemSlots::from_raw(self.page(PAGE_FOR_SALE).map_or(std::ptr::null(), |page| page.Items())) }
    }

    /// What you sold here and can buy back.
    pub fn buyback(&self) -> ItemSlots<'a> {
        unsafe { ItemSlots::from_raw(self.page(PAGE_BUYBACK).map_or(std::ptr::null(), |page| page.Items())) }
    }

    /// Whether the merchant can't take any more items (`${Merchant.Full}`).
    pub fn is_full(&self) -> bool {
        self.page(PAGE_FOR_SALE).is_some_and(|page| self.items().capacity() >= page.MaxItems().max(0) as usize)
    }

    /// The multiplier on the merchant's prices (`${Merchant.Markup}`).
    pub fn markup(&self) -> f32 {
        self.window.MerchantGreed()
    }

    /// The item selected in the window: one for sale, or one of yours to
    /// sell (`${Merchant.SelectedItem}`).
    pub fn selected_item(&self) -> Option<Item<'a>> {
        unsafe { Item::from_raw(self.window.pSelectedItem().m_pObject()) }
    }
}

impl fmt::Debug for Merchant<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Merchant").field("spawn", &self.spawn).field("items", &self.items().capacity()).finish()
    }
}

/// The open loot window, for the duration of the current callback.
#[derive(Clone, Copy)]
pub struct LootWindow<'a> {
    window: &'a CLootWnd,
    corpse: Spawn<'a>,
}

impl<'a> LootWindow<'a> {
    /// The underlying window, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a CLootWnd {
        self.window
    }

    /// The corpse being looted.
    pub fn corpse(&self) -> Spawn<'a> {
        self.corpse
    }

    /// The corpse's items by loot slot: `${Corpse.Item[n]}` is slot n-1.
    pub fn items(&self) -> ItemSlots<'a> {
        unsafe { ItemSlots::from_raw(self.window.LootItems()) }
    }

    /// How many items are left on it (`${Corpse.Items}`).
    pub fn len(&self) -> usize {
        self.items().iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl fmt::Debug for LootWindow<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LootWindow").field("corpse", &self.corpse).field("items", &self.len()).finish()
    }
}

impl<P: Plugin> Mq<P> {
    /// The merchant window, while it's open (`${Merchant.Open}`).
    pub fn merchant(&self) -> Option<Merchant<'_>> {
        let window = unsafe { (self.host().merchant_window)().as_ref() }.filter(|w| w.dShow())?;
        let spawn = unsafe { Spawn::from_raw((self.host().active_merchant)()) }?;
        Some(Merchant { window, spawn })
    }

    /// The loot window, while you're looting a corpse (`${Corpse.Open}`).
    pub fn loot_window(&self) -> Option<LootWindow<'_>> {
        let window = unsafe { (self.host().loot_window)().as_ref() }?;
        let corpse = unsafe { Spawn::from_raw((self.host().active_corpse)()) }?;
        Some(LootWindow { window, corpse })
    }

    // ----- buying and selling -----

    /// Selects one of the merchant's items to [`buy`](Mq::buy), by its slot
    /// in [`Merchant::items`], as `${Merchant.SelectItem}` does (also
    /// selecting its row in the window's list). False without the merchant
    /// window or an item there.
    pub fn select_merchant_item(&self, slot: usize) -> bool {
        let Some(item) = self.merchant().and_then(|merchant| merchant.items().get(slot)) else { return false };
        // The game's own place for it, as ${Merchant.SelectItem} takes it.
        let Some(location) = SlotLocation::from_global(item.raw().GlobalIndex()) else { return false };
        let place = SlotLocation::new(ItemContainerKind::Merchant, location.slot, None);
        self.merchant_select(place)
    }

    /// Selects one of your items to [`sell`](Mq::sell), at a place in your
    /// inventory, as `/selectitem` does (and as clicking it does with the
    /// merchant window open). False without the merchant window, an item
    /// there, or for a place outside your inventory.
    pub fn select_item_to_sell(&self, location: SlotLocation) -> bool {
        location.container == ItemContainerKind::Inventory && self.merchant_select(location)
    }

    fn merchant_select(&self, location: SlotLocation) -> bool {
        let Some((container, slot, in_bag)) = location.raw() else { return false };
        self.merchant().is_some() && unsafe { (self.window_api().mqemu_merchant_select)(container, slot, in_bag) }
    }

    /// Buys `count` of the merchant's selected item, as `${Merchant.Buy}`
    /// does. False without the merchant window, or if the selected item
    /// isn't the merchant's ([`select_merchant_item`](Mq::select_merchant_item)).
    /// The server answers a moment later: the item arrives in your
    /// inventory, or the game says in chat why not.
    pub fn buy(&self, count: u32) -> bool {
        self.merchant_trade(true, count)
    }

    /// Sells `count` of your selected item, as `${Merchant.Sell}` and
    /// `/sellitem` do. False without the merchant window, or if the
    /// selected item isn't yours ([`select_item_to_sell`](Mq::select_item_to_sell)).
    pub fn sell(&self, count: u32) -> bool {
        self.merchant_trade(false, count)
    }

    fn merchant_trade(&self, buy: bool, count: u32) -> bool {
        let Some(selected) = self.merchant().and_then(|merchant| merchant.selected_item()) else { return false };
        let from = ItemContainerKind::from_raw(selected.raw().GlobalIndex().Location().0);
        let wanted = if buy { ItemContainerKind::Merchant } else { ItemContainerKind::Inventory };
        let Ok(count) = i32::try_from(count) else { return false };
        from == wanted && count > 0 && unsafe { (self.window_api().mqemu_merchant_trade)(buy, count) }
    }
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::layouts::{PlayerClient, VePointer_ItemClient};

    use super::*;
    use crate::items::tests::{Fake, FakeItem, fill};

    #[test]
    fn merchant_pages_and_selection() {
        let window = Fake::<CMerchantWnd>::new();
        let (sale, buyback) = (Fake::<MerchantPageHandler>::new(), Fake::<MerchantPageHandler>::new());
        let (bread, sword) = (FakeItem::new(13005, "Bread"), FakeItem::new(1001, "Sword"));
        let (mut sale_slots, mut buyback_slots) = (Vec::<VePointer_ItemClient>::new(), Vec::new());
        fill(sale.get().Items(), &mut sale_slots, 2, &[(0, &bread), (1, &sword)]);
        fill(buyback.get().Items(), &mut buyback_slots, 1, &[(0, &sword)]);
        sale.get().set_MaxItems(2);
        let mut pages = [sale.ptr(), buyback.ptr()];
        window.get().PageHandlers().set_m_data(pages.as_mut_ptr());
        window.get().PageHandlers().set_m_size(2);
        window.get().set_MerchantGreed(1.25);
        window.get().pSelectedItem().set_m_pObject(sword.item.ptr());
        let npc = Fake::<PlayerClient>::new();
        let merchant = Merchant { window: window.get(), spawn: unsafe { Spawn::from_raw(npc.ptr()) }.unwrap() };

        let names: Vec<String> = merchant.items().iter().map(|(_, item)| item.name()).collect();
        assert_eq!(names, ["Bread", "Sword"]);
        assert_eq!(merchant.buyback().get(0).map(|i| i.id()), Some(1001));
        assert_eq!((merchant.markup(), merchant.selected_item().map(|i| i.id())), (1.25, Some(1001)));
        assert!(merchant.is_full());
        sale.get().set_MaxItems(80);
        assert!(!merchant.is_full());

        window.get().PageHandlers().set_m_size(1);
        assert_eq!(merchant.buyback().capacity(), 0, "no buyback page");
        unsafe { *window.get().PageHandlers().m_data() = std::ptr::null_mut() };
        assert_eq!((merchant.items().capacity(), merchant.is_full()), (0, false), "a null page");
    }

    #[test]
    fn loot_counts_items_not_slots() {
        let window = Fake::<CLootWnd>::new();
        let bone = FakeItem::new(13073, "Bone Chips");
        let mut slots = Vec::new();
        fill(window.get().LootItems(), &mut slots, 4, &[(2, &bone)]);
        let corpse = Fake::<PlayerClient>::new();
        let loot = LootWindow { window: window.get(), corpse: unsafe { Spawn::from_raw(corpse.ptr()) }.unwrap() };
        assert_eq!((loot.len(), loot.items().capacity()), (1, 4));
        assert_eq!(loot.items().get(2).map(|i| i.name()), Some("Bone Chips".into()));
    }
}
