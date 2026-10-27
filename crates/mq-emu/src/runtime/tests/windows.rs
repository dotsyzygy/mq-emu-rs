//! EQ UI windows through `Mq`, against fake windows: finding them by name
//! and path, the window manager's list and focus, and the open context menu.

use mq_emu_sys::constants::{UI_BUTTON, UI_HOT_BUTTON, UI_INV_SLOT, UI_LISTBOX, UI_SCREEN};
use mq_emu_sys::layouts::{
    CContextMenu, CContextMenuManager, CHotButton, CHotButtonWnd, CInvSlot, CInvSlotMgr, CInvSlotWnd, CTradeWnd, CXWnd, CXWndManager,
    ItemContainerInstance,
};

use super::*;
use crate::items::tests::Fake;
use crate::{InvSlot, SlotLocation, Window};
use crate::windows::fake;

/// Runs one pulse with a fresh runtime and returns what `log` wrote.
fn pulse_log(log: impl Fn(&Mq<Recorder>) -> String + 'static) -> Vec<String> {
    log_of(in_pulse(move |plugin, mq| plugin.log.push(log(mq))))[2..].to_vec()
}

fn leak<T>() -> &'static Fake<T> {
    Box::leak(Box::new(Fake::new()))
}

/// A top-level "Inventory" with a page holding a "Done" button, and a
/// hidden "Bags" window; both found by name. Returns (inventory, page, done, bags).
fn inventory() -> [*mut CXWnd; 4] {
    fake::reset();
    let [inventory, page, done, bags] = [leak::<CXWnd>(), leak(), leak(), leak()].map(Fake::ptr);
    fake::add(inventory, UI_SCREEN, Some("Inventory"), Some("Inventory"));
    fake::add(page, UI_BUTTON, None, Some("IW_Page"));
    fake::add(done, UI_BUTTON, None, Some("IW_Done"));
    fake::add(bags, UI_SCREEN, Some("Bags"), None);
    unsafe {
        (*inventory).set_m_pFirstNode(page);
        (*page).set_ParentWindow(inventory);
        (*page).set_m_pFirstNode(done);
        (*done).set_ParentWindow(page);
    }
    fake::with(|w| {
        w.by_name.insert("inventory".into(), inventory as usize);
        w.by_name.insert("bags".into(), bags as usize);
    });
    [inventory, page, done, bags]
}

fn id(window: Option<Window<'_>>) -> String {
    window.map_or("None".into(), |w| w.screen_id().or(w.name()).unwrap_or_default())
}

#[test]
fn windows_are_found_by_name_and_by_path() {
    inventory();
    let log = pulse_log(|mq| {
        [
            mq.window("Inventory"),
            mq.window("Inventory/IW_Done"),
            mq.window("Inventory/IW_Page/IW_Done"),
            mq.window("/Inventory//IW_Done/"),
            mq.window("Inventory/Nope"),
            mq.window("Nope/IW_Done"),
            mq.window("Bags/IW_Done"),
            mq.window(""),
            mq.window("/"),
        ]
        .map(id)
        .join(" ")
    });
    assert_eq!(log, ["Inventory IW_Done IW_Done IW_Done None None None None None"], "empty steps are skipped, as MacroQuest's path splitting skips them");
}

#[test]
fn the_window_list_has_the_top_level_screens() {
    let [inventory, page, _, bags] = inventory();
    let manager = leak::<CXWndManager>();
    let list = Box::leak(Box::new([inventory, std::ptr::null_mut(), page, bags]));
    manager.get().pWindows().set_m_array(list.as_mut_ptr());
    manager.get().pWindows().set_m_length(4);
    let log = |mq: &Mq<Recorder>| format!("{:?} {}", mq.windows().map(|w| w.name().unwrap_or_default()).collect::<Vec<_>>(), id(mq.focused_window()));

    assert_eq!(pulse_log(log), ["[] None"], "no window manager");
    set_windows(|w| w.window_manager = manager.ptr());
    assert_eq!(pulse_log(log), ["[\"Inventory\", \"Bags\"] None"], "null entries and controls are skipped");
    manager.get().set_FocusWindow(page);
    assert_eq!(pulse_log(log), ["[\"Inventory\", \"Bags\"] IW_Page"]);
}

#[test]
fn the_context_menu_is_read_found_in_and_picked_from() {
    fake::reset();
    let manager = leak::<CContextMenuManager>();
    let menu = leak::<CContextMenu>();
    fake::add(menu.ptr().cast(), UI_LISTBOX, None, Some("Menu"));
    fake::with(|w| {
        for (item, text) in ["Invite", "", "Trade with", "Follow"].iter().enumerate() {
            w.cells.insert((menu.ptr() as usize, item as i32, 1), (*text).into());
        }
    });
    let lines = Box::leak(vec![0u64; (size_of::<mq_emu_sys::layouts::SListWndLine>() * 4).div_ceil(8)].into_boxed_slice());
    let list = unsafe { &*menu.ptr().cast::<mq_emu_sys::layouts::CListWnd>() };
    // Each row has the menu's two cells (an icon, then the text).
    let first = lines.as_mut_ptr().cast::<mq_emu_sys::layouts::SListWndLine>();
    for row in 0..4 {
        let cells = Box::leak(vec![0u64; (size_of::<mq_emu_sys::layouts::SListWndCell>() * 2).div_ceil(8)].into_boxed_slice());
        let line = unsafe { &*first.add(row) };
        line.Cells().set_m_array(cells.as_mut_ptr().cast());
        line.Cells().set_m_length(2);
    }
    list.ItemsArray().set_m_array(lines.as_mut_ptr().cast());
    list.ItemsArray().set_m_length(4);
    manager.get().set_pCurrMenus_at(0, menu.ptr());
    set_windows(|w| w.context_menu_manager = manager.ptr());

    let log = |mq: &Mq<Recorder>| {
        let Some(menu) = mq.context_menu() else { return "None".into() };
        let picked = menu.find("TRADE").is_some_and(|item| menu.select(item));
        format!("{:?} {:?} {:?} {picked}", menu.items().collect::<Vec<_>>(), menu.find("w"), menu.find("Nope"))
    };
    assert_eq!(pulse_log(log), ["None"], "no menu open");
    manager.get().set_NumVisibleMenus(1);
    assert_eq!(pulse_log(log), ["[\"Invite\", \"\", \"Trade with\", \"Follow\"] Some(2) None true"], "separators never match");
    assert_eq!(fake::log(), ["menu_select Menu 2"]);
    assert_eq!(pulse_log(|mq| mq.context_menu().unwrap().close().to_string()), ["true"]);
    assert_eq!(fake::log(), ["menus_close"]);
}

/// Item slot windows as the slot manager lists them: (container, slots,
/// hot button) each, as "Slot<n>".
fn slot_manager(slots: &[(i32, [i16; 3], bool)]) -> Vec<*mut CInvSlotWnd> {
    fake::reset();
    let manager = leak::<CInvSlotMgr>();
    let windows: Vec<*mut CInvSlotWnd> = slots
        .iter()
        .enumerate()
        .map(|(i, (container, index, hot_button))| {
            let window = leak::<CInvSlotWnd>();
            let slot = leak::<CInvSlot>();
            fake::add(window.ptr().cast(), UI_INV_SLOT, None, Some(&format!("Slot{i}")));
            window.get().ItemLocation().set_Location(ItemContainerInstance(*container));
            window.get().ItemLocation().Index().set_m_slots(*index);
            window.get().set_bHotButton(*hot_button);
            slot.get().set_pInvSlotWnd(window.ptr());
            manager.get().set_SlotArray_at(i + 1, slot.ptr());
            window.ptr()
        })
        .collect();
    // Slot 0 is empty, as unused entries are.
    manager.get().set_TotalSlots(slots.len() as i32 + 1);
    set_windows(|w| w.inv_slot_manager = manager.ptr());
    windows
}

#[test]
fn item_slot_windows_are_found_as_itemnotify_finds_them() {
    slot_manager(&[(-1, [13, -1, -1], true), (0, [13, -1, -1], false), (0, [24, 2, -1], false), (1, [3, -1, -1], false)]);
    let log = pulse_log(|mq| {
        let find = |text: &str| mq.item_slot_window(SlotLocation::parse(text).unwrap()).and_then(|s| s.window().screen_id()).unwrap_or("None".into());
        let all = mq.item_slot_windows().count();
        format!("{all} {} {} {} {} {}", find("mainhand"), find("in pack2 3"), find("pack2"), find("bank4"), find("bank3"))
    });
    assert_eq!(log, ["4 Slot1 Slot2 None Slot3 None"]);
}

#[test]
fn hot_button_slots_are_left_out_of_item_slot_lookups() {
    slot_manager(&[(0, [13, -1, -1], true), (0, [13, -1, -1], false)]);
    let log = pulse_log(|mq| format!("{:?}", mq.item_slot_window(SlotLocation::inv_slot(InvSlot::PRIMARY)).and_then(|s| s.window().screen_id())));
    assert_eq!(log, ["Some(\"Slot1\")"], "a hot button's slot can show the same place");
}

#[test]
fn no_slot_manager_no_item_slots() {
    fake::reset();
    set_windows(|w| w.inv_slot_manager = std::ptr::null_mut());
    let log = pulse_log(|mq| format!("{} {}", mq.item_slot_windows().count(), mq.item_slot_window(SlotLocation::inv_slot(InvSlot::PRIMARY)).is_some()));
    assert_eq!(log, ["0 false"]);
}

#[test]
fn hot_bars_are_found_by_number_with_their_page_and_buttons() {
    fake::reset();
    let bar = leak::<CHotButtonWnd>();
    let button = leak::<CHotButton>();
    fake::add(bar.ptr().cast(), UI_SCREEN, Some("HotButtonWnd2"), None);
    fake::add(button.ptr().cast(), UI_HOT_BUTTON, None, Some("HB_Button3"));
    bar.get().set_Buttons_at(2, button.ptr());
    fake::with(|w| {
        w.by_name.insert("hotbuttonwnd2".into(), bar.ptr() as usize);
    });
    let info = unsafe { &*INFO.with(FakeObject::ptr) };
    info.set_hotBank_at(1, 4);
    let log = pulse_log(|mq| {
        let Some(bar) = mq.hot_bar(1) else { return "None".into() };
        let buttons: Vec<_> = bar.buttons().map(|b| b.window().screen_id().unwrap_or_default()).collect();
        format!("{} {buttons:?} {} {} {}", bar.page(), bar.button(12).is_some(), mq.hot_bar(0).is_some(), mq.hot_bar(10).is_some())
    });
    assert_eq!(log, ["4 [\"HB_Button3\"] false false false"]);
    info.set_hotBank_at(1, 0);
}

#[test]
fn the_trade_window_reads_both_sides() {
    fake::reset();
    let trade = leak::<CTradeWnd>();
    let slot = leak::<CInvSlotWnd>();
    fake::add(trade.ptr().cast(), UI_SCREEN, Some("TradeWnd"), None);
    fake::add(slot.ptr().cast(), UI_INV_SLOT, None, Some("TRDW_TradeSlot8"));
    trade.get().set_pInvSlotWnd_at(8, slot.ptr());
    trade.get().set_bHisReadyTrade(true);
    trade.get().set_bIsTrading(true);
    let log = |mq: &Mq<Recorder>| {
        let Some(trade) = mq.trade_window() else { return "None".into() };
        let slots = [0, 8, 16].map(|i| trade.slot_window(i).and_then(|s| s.window().screen_id()));
        format!("{} {} {} {} {slots:?}", trade.is_trading(), trade.my_ready(), trade.their_ready(), trade.items().capacity())
    };
    set_windows(|w| w.trade_window = std::ptr::null_mut());
    assert_eq!(pulse_log(log), ["None"]);
    set_windows(|w| w.trade_window = trade.ptr());
    assert_eq!(pulse_log(log), ["true false true 0 [None, Some(\"TRDW_TradeSlot8\"), None]"]);
}

#[test]
fn links_are_sent_with_their_text() {
    let [inventory, _, done, _] = inventory();
    let log = pulse_log(move |mq| {
        let window = mq.window("Inventory").unwrap();
        let done = mq.window("Inventory/IW_Done").unwrap();
        format!("{} {}", window.notify_link(Some(done), "0012345"), window.notify_link(None, "abc"))
    });
    assert_eq!(log, ["true true"]);
    assert_eq!(fake::log(), ["notify_link Inventory IW_Done 0012345", "notify_link Inventory 0 abc"]);
    let _ = (inventory, done);
}
