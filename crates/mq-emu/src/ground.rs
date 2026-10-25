//! Items on the ground: dropped items and the zone's ground spawns (forage
//! spots aside, anything you can pick up with a click).

use std::fmt;
use std::marker::PhantomData;

use mq_emu_sys::layouts::{EQGroundItem, EQGroundItemListManager};

use crate::game::{Heading, Position};
use crate::items::Item;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::from_c_array;

/// The most items [`GroundItems`] will walk; like [`MAX_SPAWNS`](crate::MAX_SPAWNS),
/// only a guard against a damaged list.
pub const MAX_GROUND_ITEMS: usize = 10_000;

/// An item on the ground, for the duration of the current callback. Keep its
/// [`id`](GroundItem::id) to find it again.
#[derive(Clone, Copy)]
pub struct GroundItem<'a> {
    raw: &'a EQGroundItem,
}

impl<'a> GroundItem<'a> {
    /// # Safety
    /// `raw` must be null or point to a live ground item for `'a`, which must
    /// not extend past the current callback.
    pub unsafe fn from_raw(raw: *const EQGroundItem) -> Option<GroundItem<'a>> {
        unsafe { raw.as_ref() }.map(|raw| GroundItem { raw })
    }

    /// The underlying game object, for fields this API doesn't wrap yet.
    pub fn raw(&self) -> &'a EQGroundItem {
        self.raw
    }

    /// The drop's ID, unique in the zone (`${Ground.ID}`).
    pub fn id(&self) -> i32 {
        self.raw.DropID()
    }

    /// The model's actor tag, e.g. "IT63_ACTORDEF" (`${Ground.Name}`). The
    /// item's own name, when the client has it, is on [`item`](GroundItem::item).
    pub fn name(&self) -> String {
        from_c_array(&self.raw.Name())
    }

    /// The item, if the client knows what it is (it does for dropped items).
    pub fn item(&self) -> Option<Item<'a>> {
        unsafe { Item::from_raw(self.raw.Item().m_pObject()) }
    }

    pub fn position(&self) -> Position {
        Position { x: self.raw.X(), y: self.raw.Y(), z: self.raw.Z() }
    }

    pub fn heading(&self) -> Heading {
        Heading::from_units(self.raw.Heading())
    }

    /// The item's weight, or -1 for items that can't be picked up.
    pub fn weight(&self) -> i32 {
        self.raw.Weight()
    }
}

impl fmt::Debug for GroundItem<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GroundItem").field("id", &self.id()).field("name", &self.name()).finish()
    }
}

/// Iterates over the zone's ground items in the client's list order.
pub struct GroundItems<'a> {
    next: *mut EQGroundItem,
    walked: usize,
    _borrow: PhantomData<&'a ()>,
}

impl<'a> GroundItems<'a> {
    /// # Safety
    /// `manager` must be null or the game's ground item list, and its items
    /// must stay alive for `'a`, which must not extend past the current callback.
    pub(crate) unsafe fn new(manager: *const EQGroundItemListManager) -> GroundItems<'a> {
        let first = unsafe { manager.as_ref() }.map_or(std::ptr::null_mut(), |m| m.Top());
        GroundItems { next: first, walked: 0, _borrow: PhantomData }
    }
}

impl<'a> Iterator for GroundItems<'a> {
    type Item = GroundItem<'a>;

    fn next(&mut self) -> Option<GroundItem<'a>> {
        if self.walked >= MAX_GROUND_ITEMS {
            return None;
        }
        // SAFETY: list nodes are live ground items for the current callback (see `new`).
        let item = unsafe { GroundItem::from_raw(self.next) }?;
        self.next = item.raw.pNext();
        self.walked += 1;
        Some(item)
    }
}

impl<P: Plugin> Mq<P> {
    /// Every item on the ground in the zone.
    pub fn ground_items(&self) -> GroundItems<'_> {
        unsafe { GroundItems::new((self.host().ground_items)()) }
    }

    /// The ground item with this ID, if it's still there.
    pub fn ground_item(&self, id: i32) -> Option<GroundItem<'_>> {
        self.ground_items().find(|item| item.id() == id)
    }

    /// The ground item closest to you that `filter` accepts. Distance is 3D.
    pub fn nearest_ground_item(&self, filter: impl Fn(&GroundItem<'_>) -> bool) -> Option<GroundItem<'_>> {
        let here = self.me()?.position();
        self.ground_items()
            .filter(|item| filter(item))
            .map(|item| (item.position().distance(&here), item))
            .min_by(|(a, _), (b, _)| a.total_cmp(b))
            .map(|(_, item)| item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::tests::{Fake, FakeItem, set_name};

    #[test]
    fn ground_items_walk_the_list_and_know_their_item() {
        let manager = Fake::<EQGroundItemListManager>::new();
        assert_eq!(unsafe { GroundItems::new(manager.ptr()) }.count(), 0);

        let (first, second) = (Fake::<EQGroundItem>::new(), Fake::<EQGroundItem>::new());
        first.get().set_DropID(5);
        first.get().set_Name(set_name("IT63_ACTORDEF"));
        first.get().set_X(1.0);
        first.get().set_pNext(second.ptr());
        second.get().set_DropID(6);
        let sword = FakeItem::new(1001, "Rusty Sword");
        second.get().Item().set_m_pObject(sword.item.ptr());
        manager.get().set_Top(first.ptr());

        let found: Vec<(i32, Option<String>)> =
            unsafe { GroundItems::new(manager.ptr()) }.map(|item| (item.id(), item.item().map(|i| i.name()))).collect();
        assert_eq!(found, [(5, None), (6, Some("Rusty Sword".to_owned()))]);
        let first = unsafe { GroundItem::from_raw(first.ptr()) }.unwrap();
        assert_eq!((first.name().as_str(), first.position().x), ("IT63_ACTORDEF", 1.0));

        // A cycle stops at the walk limit.
        unsafe { (*second.ptr()).set_pNext(manager.get().Top()) };
        assert_eq!(unsafe { GroundItems::new(manager.ptr()) }.count(), MAX_GROUND_ITEMS);
        assert_eq!(unsafe { GroundItems::new(std::ptr::null()) }.count(), 0);
    }
}
