//! EverQuest's own UI windows (`CXWnd` and its subclasses), from
//! `shim/windows.cpp`: finding them as MacroQuest does, what only the game's
//! functions can say about them (their kind, XML names, tooltip, list text,
//! the item in an item slot or behind a hot button), clicking and notifying
//! them as `/notify` and `${Window}` do, and a plugin's own windows, made
//! from screen pieces in UI XML it adds (`AddXMLFile`).
//!
//! These import `MQ2Main.dll` and `eqlib.dll`, so they only run inside the
//! game; call them through [`API`] (see [`function_table!`]). Text comes back
//! NUL-terminated in the caller's buffer, cut to fit, and the functions
//! return its whole length in bytes (so a caller can retry with a bigger
//! buffer), or -1 for none. Every function checks its window for null and the
//! kind of window it needs, and answers "nothing" (null, -1, false) otherwise.

use core::ffi::{c_char, c_void};

use crate::layouts::{CContextMenu, CHotButton, CInvSlotWnd, CListWnd, CTabWnd, CTextureAnimation, CXWnd, ItemClient};

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

/// What a plugin's window ([`mqemu_plugin_window_create`]) calls with each
/// notification it gets, before handling it as any screen does: `user` as
/// given, the window, the control that sent it (or null), the message
/// (`XWM_*`) and its data (a number, or for `XWM_LINK` the link's text).
pub type mqemu_window_notification_fn =
    unsafe extern "C" fn(user: *mut c_void, window: *mut CXWnd, sender: *mut CXWnd, message: u32, data: *mut c_void);

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
    /// Sends `window` a `link` notification (`XWM_LINK`) from `sender`,
    /// whose data is the link's text, as `/notify <window> <control> link
    /// <text>` does. False for null.
    pub fn mqemu_window_notify_link(window: *mut CXWnd, sender: *mut CXWnd, text: *const c_char) -> bool;
    /// The item an item slot shows (`${Window.InvSlot.Item}`): the one it's
    /// linked to (a hot button's), else the one at its location. Null for
    /// none, or a window that isn't an item slot.
    pub fn mqemu_inv_slot_item(slot: *mut CInvSlotWnd) -> *mut ItemClient;
    /// The text an item slot draws over its item (`${Window.InvSlot.Text}`).
    pub fn mqemu_inv_slot_text(slot: *mut CInvSlotWnd, out: *mut c_char, cap: usize) -> i32;
    /// A UI texture's name (`CTextureAnimation::GetName`).
    pub fn mqemu_texture_animation_name(animation: *mut CTextureAnimation, out: *mut c_char, cap: usize) -> i32;
    /// The item a hot button holds (`GetHotButtonData()->Item`), if any.
    pub fn mqemu_hot_button_item(button: *mut CHotButton) -> *mut ItemClient;
    /// Presses a hot button as its bar does when its key is pressed
    /// (`${Window.HotButton.Activate}`). False if its bar isn't found.
    pub fn mqemu_hot_button_activate(button: *mut CHotButton) -> bool;
    /// MacroQuest's `AddXMLFile`. False only for an empty name.
    pub fn mqemu_add_xml_file(filename: *const c_char) -> bool;
    /// MacroQuest's `RemoveXMLFile`.
    pub fn mqemu_remove_xml_file(filename: *const c_char) -> bool;
    /// MacroQuest's `IsScreenPieceLoaded`.
    pub fn mqemu_screen_piece_loaded(name: *const c_char) -> bool;
    /// Makes a top-level window from the screen piece `screen` (and its
    /// controls), shown, as MacroQuest's `CCustomWnd` does, passing each
    /// notification it gets to `notify` first. Null if the loaded UI has no
    /// such screen piece. The window's code is in the plugin: it must be
    /// destroyed ([`mqemu_plugin_window_destroy`]) before the UI is cleaned
    /// up and before the plugin unloads.
    pub fn mqemu_plugin_window_create(screen: *const c_char, notify: mqemu_window_notification_fn, user: *mut c_void) -> *mut CXWnd;
    /// Deletes a window [`mqemu_plugin_window_create`] made.
    pub fn mqemu_plugin_window_destroy(window: *mut CXWnd) -> bool;
}
