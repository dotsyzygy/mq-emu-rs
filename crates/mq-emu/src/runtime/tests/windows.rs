//! EQ UI windows through `Mq`, against fake windows: finding them by name
//! and path, the window manager's list and focus, and the open context menu.

use mq_emu_sys::constants::{UI_BUTTON, UI_LISTBOX, UI_SCREEN};
use mq_emu_sys::layouts::{CContextMenu, CContextMenuManager, CXWnd, CXWndManager};

use super::*;
use crate::items::tests::Fake;
use crate::Window;
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
