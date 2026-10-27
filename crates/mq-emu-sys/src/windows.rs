//! EverQuest's own UI windows (`CXWnd` and its subclasses), from
//! `shim/windows.cpp`: finding them as MacroQuest does, what only the game's
//! functions can say about them (their kind, XML names, tooltip, list text),
//! and clicking and notifying them as `/notify` and `${Window}` do.
//!
//! These import `MQ2Main.dll` and `eqlib.dll`, so they only run inside the
//! game; call them through [`API`] (see [`function_table!`]). Text comes back
//! NUL-terminated in the caller's buffer, cut to fit, and the functions
//! return its whole length in bytes (so a caller can retry with a bigger
//! buffer), or -1 for none. Every function checks its window for null and the
//! kind of window it needs, and answers "nothing" (null, -1, false) otherwise.

use core::ffi::c_char;

use crate::layouts::{CContextMenu, CListWnd, CTabWnd, CXWnd};

/// A rectangle in screen or parent coordinates, as eqlib's `CXRect`: right
/// and bottom are one past the last pixel.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct mqemu_rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// [`mqemu_window_xml_text`]: the window's name in the UI's XML (`${Window.Name}`).
pub const XML_NAME: i32 = 0;
/// [`mqemu_window_xml_text`]: its screen ID (`${Window.ScreenID}`).
pub const XML_SCREEN_ID: i32 = 1;
/// [`mqemu_window_xml_text`]: the name of its XML type (`${Window.Type}`).
pub const XML_TYPE_NAME: i32 = 2;

/// [`mqemu_window_click`]s, in the order of MacroQuest's click names
/// (`leftmouse` to `rightmouseheldup`).
pub const CLICK_KINDS: usize = 8;

function_table! {
    /// The window `name` names, as `${Window[name]}` finds it: a top-level
    /// window by name (a shown one first, if several share it), or a bag's
    /// window as `pack1`..., `bank1`... or `enviro`. Null if none. Paths
    /// (`a/b`) aren't taken here; the caller walks children itself.
    pub fn mqemu_find_window(name: *const c_char) -> *mut CXWnd;
    /// The window's kind (`CXWnd::GetType`, a `UI_*` constant); `UI_UNKNOWN`
    /// for null or a window the UI's XML didn't create.
    pub fn mqemu_window_type(window: *mut CXWnd) -> i32;
    /// One of the window's XML names: [`XML_NAME`], [`XML_SCREEN_ID`] or
    /// [`XML_TYPE_NAME`]. -1 if it has none.
    pub fn mqemu_window_xml_text(window: *mut CXWnd, which: i32, out: *mut c_char, cap: usize) -> i32;
    /// The window's tooltip (the virtual `GetTooltip`, `${Window.Tooltip}`).
    pub fn mqemu_window_tooltip(window: *mut CXWnd, out: *mut c_char, cap: usize) -> i32;
    /// Where the window is on screen (`GetScreenRect`). False for null.
    pub fn mqemu_window_screen_rect(window: *mut CXWnd, out: *mut mqemu_rect) -> bool;
    /// Clicks the window's center as `/notify` does (`SendWndClick2`):
    /// `click` below [`CLICK_KINDS`] names `leftmouse`, `leftmouseup`,
    /// `leftmouseheld`, `leftmouseheldup`, then the same for the right button.
    pub fn mqemu_window_click(window: *mut CXWnd, click: i32) -> bool;
    /// Sends `window` the notification `message` (`XWM_*`, one of those
    /// `/notify` names) as if `sender` (null, or one of its controls) sent it,
    /// with `data`: `window->WndNotification(sender, message, data)`. For
    /// `XWM_NEWVALUE` from a slider, sets the slider to `data` first, as
    /// `/notify` does. False for null or another message.
    pub fn mqemu_window_notify(window: *mut CXWnd, sender: *mut CXWnd, message: u32, data: i32) -> bool;
    /// Shows (and brings to the front) or hides the window, as
    /// `${Window.DoOpen}` and `${Window.DoClose}` do.
    pub fn mqemu_window_show(window: *mut CXWnd, show: bool) -> bool;
    /// Moves and sizes the window (`rect` in its parent's coordinates, as
    /// `${Window.X}` reads them), as `${Window.Move}` does.
    pub fn mqemu_window_move(window: *mut CXWnd, rect: *const mqemu_rect) -> bool;
    /// Redraws the window's background and fade after its `BGColor`, `Alpha`
    /// or `FadeToAlpha` changed (`CXWnd::Refade`).
    pub fn mqemu_window_refade(window: *mut CXWnd) -> bool;
    /// Sets an edit box's text and tells its parent, as `${Window.SetText}`
    /// does. False for null or a window that isn't an edit box.
    pub fn mqemu_window_set_text(window: *mut CXWnd, text: *const c_char) -> bool;
    /// The text of a list's cell (`CListWnd::GetItemText`); -1 if the row or
    /// column isn't there.
    pub fn mqemu_list_item_text(list: *mut CListWnd, row: i32, column: i32, out: *mut c_char, cap: usize) -> i32;
    /// Selects a row of a list or a combo box and tells its parent, as
    /// `/notify ... listselect` does (`SendListSelect2`). False for null,
    /// another kind of window or a row that isn't there.
    pub fn mqemu_list_select(window: *mut CXWnd, row: i32) -> bool;
    /// Shows page `page` of a tab box and tells its parent (`SetPage`), as
    /// `/notify ... tabselect` does. False for a page that isn't there.
    pub fn mqemu_tab_select(tab: *mut CTabWnd, page: i32) -> bool;
    /// Picks item `item` of an open context menu, as `/notify <text>
    /// menuselect` does. False if the item isn't there.
    pub fn mqemu_menu_select(menu: *mut CContextMenu, item: i32) -> bool;
    /// Closes every open context menu (`CContextMenuManager::Flush`).
    pub fn mqemu_menus_close() -> bool;
}
