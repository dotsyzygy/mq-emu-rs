//! EverQuest's own UI windows (`CXWnd` and its subclasses), from
//! `shim/windows.cpp`: finding them as MacroQuest does, what only the game's
//! functions can say about them (their kind, XML names, tooltip, list text,
//! the item in an item slot or behind a hot button), clicking and notifying
//! them as `/notify` and `${Window}` do, a plugin's own windows, made from
//! screen pieces in UI XML it adds (`AddXMLFile`), subclassing the game's
//! windows (as MacroQuest's `WindowOverride` does), drawing on windows as
//! the game draws them, what `/itemnotify` does with an item it finds no
//! slot window for (picking it up or dropping it, `/useitem`, opening bags),
//! picking up part of a stack, and buying and selling at a merchant.
//!
//! These import `MQ2Main.dll` and `eqlib.dll`, so they only run inside the
//! game; call them through [`API`] (see [`function_table!`]). Text comes back
//! NUL-terminated in the caller's buffer, cut to fit, and the functions
//! return its whole length in bytes (so a caller can retry with a bigger
//! buffer), or -1 for none. Every function checks its window for null and the
//! kind of window it needs, and answers "nothing" (null, -1, false) otherwise.

use core::ffi::{CStr, c_char, c_void};

use crate::constants::{UI_COMBOBOX, UI_HOT_BUTTON, UI_INV_SLOT, UI_LISTBOX, UI_SCREEN, UI_SLIDER, UI_TAB_BOX, UI_TREE_VIEW, UI_UNKNOWN, XWM_CLOSE, XWM_HISTORY, XWM_HITENTER, XWM_LCLICK, XWM_LCLICKHOLD, XWM_LINK, XWM_LMOUSEUP, XWM_MAXIMIZEBOX, XWM_MENUSELECT, XWM_MOUSEOVER, XWM_NEWVALUE, XWM_QMARKBOX, XWM_RCLICK};
use crate::eqlib;
use crate::layouts::{
    CComboWnd, CContainerMgr, CContainerWnd, CContextMenu, CHotButton, CHotButtonWnd, CInvSlotWnd, CListWnd, CParamScreenPiece, CSliderWnd, CTabWnd, CTextObjectInterface, CTextureAnimation, CTextureFont, CXPoint, CXRect, CXStr, CXWnd, CXWndManager, ItemClient, ItemContainerInstance, VePointer_ItemClient,
};
use crate::types::{OwnedCXStr, foreign, virtual_fn};

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

/// [`mqemu_window_event`] kinds: a notification, from one of the window's
/// controls or `/notify` (`WndNotification`).
pub const HOOK_NOTIFICATION: i32 = 0;
/// The window has drawn (`PostDraw`); the plugin may draw on it now.
pub const HOOK_DRAW: i32 = 1;
/// A mouse button went down or up over the window (`HandleLButtonDown`...).
pub const HOOK_LBUTTON_DOWN: i32 = 2;
pub const HOOK_LBUTTON_UP: i32 = 3;
pub const HOOK_RBUTTON_DOWN: i32 = 4;
pub const HOOK_RBUTTON_UP: i32 = 5;
/// The mouse moved over the window (`HandleMouseMove`).
pub const HOOK_MOUSE_MOVE: i32 = 6;
/// The mouse wheel turned over the window (`HandleWheelMove`).
pub const HOOK_WHEEL: i32 = 7;
/// How many kinds there are; a hook's mask has bit `1 << kind` for each it wants.
pub const HOOK_KINDS: i32 = 8;

/// Something a window got, as a plugin's window ([`mqemu_plugin_window_create`])
/// or a subclassed one ([`mqemu_window_subclass`]) passes it on.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct mqemu_window_event {
    /// A `HOOK_*` kind.
    pub kind: i32,
    /// For a notification: the control that sent it (or null), the message
    /// (`XWM_*`) and its data (a number, or for `XWM_LINK` the link's text).
    pub sender: *mut CXWnd,
    pub message: u32,
    pub data: *mut c_void,
    /// For the mouse: where it is on screen, and the keyboard flags.
    pub x: i32,
    pub y: i32,
    pub flags: u32,
    /// For the wheel: how far it turned (up is positive).
    pub scroll: i32,
}

/// What a hooked window calls with each event its mask asks for: `user` as
/// given, the window and the event. True if the plugin handled it, so the
/// window doesn't (ignored for [`HOOK_DRAW`], which comes after the window
/// drew).
pub type mqemu_window_hook_fn = unsafe extern "C" fn(user: *mut c_void, window: *mut CXWnd, event: *const mqemu_window_event) -> bool;

function_table! {
    /// MacroQuest's `AddXMLFile`. False only for an empty name.
    pub fn mqemu_add_xml_file(filename: *const c_char) -> bool;
    /// MacroQuest's `RemoveXMLFile`.
    pub fn mqemu_remove_xml_file(filename: *const c_char) -> bool;
    /// MacroQuest's `IsScreenPieceLoaded`.
    pub fn mqemu_screen_piece_loaded(name: *const c_char) -> bool;
    /// Makes a top-level window from the screen piece `screen` (and its
    /// controls), shown, as MacroQuest's `CCustomWnd` does, passing the
    /// events `mask` asks for to `hook` first. Null if the loaded UI has no
    /// such screen piece. The window's code is in the plugin: it must be
    /// destroyed ([`mqemu_plugin_window_destroy`]) before the UI is cleaned
    /// up and before the plugin unloads.
    pub fn mqemu_plugin_window_create(screen: *const c_char, hook: mqemu_window_hook_fn, user: *mut c_void, mask: u32) -> *mut CXWnd;
    /// Deletes a window [`mqemu_plugin_window_create`] made.
    pub fn mqemu_plugin_window_destroy(window: *mut CXWnd) -> bool;
    /// Subclasses one of the game's windows, as MacroQuest's `WindowOverride`
    /// does: gives it a copy of its vtable whose notification, `PostDraw` and
    /// mouse entries pass the events `mask` asks for to `hook` first. False
    /// for null, or a window this plugin already subclassed. Its code is in
    /// the plugin: take it off ([`mqemu_window_unsubclass`]) before the
    /// window is deleted (before the UI is cleaned up) and before the plugin
    /// unloads.
    pub fn mqemu_window_subclass(window: *mut CXWnd, hook: mqemu_window_hook_fn, user: *mut c_void, mask: u32) -> bool;
    /// Takes a subclass off. True if the window has its old vtable back;
    /// false if it wasn't subclassed, or if another hook went on after this
    /// one, which then passes everything on to the window's own functions
    /// for good (and never calls into the plugin again). A window that isn't
    /// `live` (the game deleted it) isn't touched; its subclass only stops.
    pub fn mqemu_window_unsubclass(window: *mut CXWnd, live: bool) -> bool;
    /// Picks up the item at a place onto the cursor, as `/itemnotify` does
    /// when it finds no slot window for it. `container` is an
    /// `ItemContainerInstance`, `in_bag` -1 for none. Outside bags (and with
    /// the merchant window open, where it selects the item instead) this is
    /// MacroQuest's `PickupItem` (false, said in chat, if it can't). Inside
    /// a bag, where `PickupItem` corrupts the stack on this client (eqlib's
    /// `AttachToCursor` takes more arguments than the client's), the shim
    /// does what it does (`MoveItem`, then the cursor's icon) with the
    /// client's own `AttachToCursor`; false with something on the cursor,
    /// nothing there, or a bank place without the bank window.
    pub fn mqemu_pick_up_item(container: i32, slot: i32, in_bag: i32) -> bool;
    /// Puts the item on the cursor at a place, swapping with what's there:
    /// MacroQuest's `DropItem` outside bags, the shim's own inside one (see
    /// [`mqemu_pick_up_item`]). False with nothing on the cursor.
    pub fn mqemu_drop_item(container: i32, slot: i32, in_bag: i32) -> bool;
    /// Uses (clicks) the item at a place in your inventory, as `/useitem`
    /// does (the game's command). False if there's no item there.
    pub fn mqemu_use_item(slot: i32, in_bag: i32) -> bool;
    /// Opens (or closes) the bag at a place, as `/itemnotify` does for a bag
    /// right-clicked without a slot window (`OpenContainer`, `CloseContainer`).
    /// Opening: true if it's open after; it needs the bag's slot window.
    /// Closing: true if it was open.
    pub fn mqemu_open_container(container: i32, slot: i32, in_bag: i32, open: bool) -> bool;
    /// Picks up `count` of the stack at a place onto the cursor, as accepting
    /// the quantity window does (`CInvSlot::SliderComplete`, on the place's
    /// slot window; a closed bag is opened for it and closed again). False
    /// with something on the cursor, the merchant window open, no stack
    /// there or no slot window.
    pub fn mqemu_split_stack(container: i32, slot: i32, in_bag: i32, count: i32) -> bool;
    /// Shows the cursor's item on the cursor's icon if it isn't already: the
    /// game's `/autoinventory` does nothing while the icon doesn't show it.
    /// False with nothing on the cursor.
    pub fn mqemu_attach_cursor_item() -> bool;
    /// Selects an item in the open merchant window
    /// (`CMerchantWnd::SelectBuySellSlot`): yours at a place in your
    /// inventory, or the merchant's at a merchant slot (also selecting its
    /// row). False without the window or an item there.
    pub fn mqemu_merchant_select(container: i32, slot: i32, in_bag: i32) -> bool;
    /// Buys `count` of the merchant's selected item (`buy`) or sells `count`
    /// of yours (`RequestGetItem`, `RequestPutItem`). False without the window
    /// or a selected item.
    pub fn mqemu_merchant_trade(buy: bool, count: i32) -> bool;

    rust {
        /// The window `name` names, as `${Window[name]}` finds it: a top-level
        /// window by name (a shown one first, if several share it), or a bag's
        /// window as `pack1`..., `bank1`... or `enviro`. Null if none. Paths
        /// (`a/b`) aren't taken here; the caller walks children itself.
        pub fn mqemu_find_window(name: *const c_char) -> *mut CXWnd;
        /// Clicks the window's center as `/notify` does (`SendWndClick2`):
        /// `click` below [`CLICK_KINDS`] names `leftmouse`, `leftmouseup`,
        /// `leftmouseheld`, `leftmouseheldup`, then the same for the right button.
        pub fn mqemu_window_click(window: *mut CXWnd, click: i32) -> bool;
        /// Selects a row of a list or a combo box and tells its parent, as
        /// `/notify ... listselect` does (`SendListSelect2`). False for null,
        /// another kind of window or a row that isn't there.
        pub fn mqemu_list_select(window: *mut CXWnd, row: i32) -> bool;
        /// The item an item slot shows (`${Window.InvSlot.Item}`): the one it's
        /// linked to (a hot button's), else the one at its location. Null for
        /// none, or a window that isn't an item slot.
        pub fn mqemu_inv_slot_item(slot: *mut CInvSlotWnd) -> *mut ItemClient;
        /// Presses a hot button as its bar does when its key is pressed
        /// (`${Window.HotButton.Activate}`). False if its bar isn't found.
        pub fn mqemu_hot_button_activate(button: *mut CHotButton) -> bool;
        /// Where the window's client area is on screen, as far as it shows
        /// (`GetClientClipRect`): where drawing on it shows.
        pub fn mqemu_window_client_clip_rect(window: *mut CXWnd, out: *mut mqemu_rect) -> bool;
        /// Fills `rect` (screen coordinates) with `color` (ARGB), clipped to
        /// `clip` (`CXWnd::DrawColoredRect`). Only while the game draws its UI.
        pub fn mqemu_draw_fill_rect(rect: *const mqemu_rect, color: u32, clip: *const mqemu_rect) -> bool;
        /// Draws `text` in EQ font `font` in `rect`, clipped to `clip`, laid out
        /// as `flags` (eqlib's `DrawTextFlags`) say (`CTextureFont::DrawWrappedText`).
        /// False if there's no such font.
        pub fn mqemu_draw_text(font: i32, text: *const c_char, rect: *const mqemu_rect, clip: *const mqemu_rect, color: u32, flags: u32) -> bool;
        /// How wide `text` is in EQ font `font`, in pixels; -1 without the font.
        pub fn mqemu_text_width(font: i32, text: *const c_char) -> i32;
        /// How tall a line of EQ font `font` is, in pixels; -1 without the font.
        pub fn mqemu_font_height(font: i32) -> i32;
        /// Draws a UI texture in `rect`, clipped to `clip`, tinted `color` (and
        /// `color2`, for animations drawn with two) (`CTextureAnimation::Draw`).
        pub fn mqemu_draw_texture(animation: *mut CTextureAnimation, rect: *const mqemu_rect, clip: *const mqemu_rect, color: u32, color2: u32) -> bool;
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
        /// The text of a list's cell (`CListWnd::GetItemText`); -1 if the row or
        /// column isn't there.
        pub fn mqemu_list_item_text(list: *mut CListWnd, row: i32, column: i32, out: *mut c_char, cap: usize) -> i32;
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
        /// The text an item slot draws over its item (`${Window.InvSlot.Text}`).
        pub fn mqemu_inv_slot_text(slot: *mut CInvSlotWnd, out: *mut c_char, cap: usize) -> i32;
        /// A UI texture's name (`CTextureAnimation::GetName`).
        pub fn mqemu_texture_animation_name(animation: *mut CTextureAnimation, out: *mut c_char, cap: usize) -> i32;
        /// The item a hot button holds (`GetHotButtonData()->Item`), if any.
        pub fn mqemu_hot_button_item(button: *mut CHotButton) -> *mut ItemClient;
        /// Sets an edit box's text and tells its parent, as `${Window.SetText}`
        /// does. False for null or a window that isn't an edit box.
        pub fn mqemu_window_set_text(window: *mut CXWnd, text: *const c_char) -> bool;
    }
}

/// [`Api::mqemu_window_set_text`]: the virtual `SetWindowText`, then
/// `ParentWndNotification(XWM_NEWVALUE)`, calling eqlib and the game directly.
///
/// # Safety
/// In game, on the game thread; `window` null or a live window, `text` null
/// or NUL-terminated.
pub unsafe extern "C" fn mqemu_window_set_text(window: *mut CXWnd, text: *const c_char) -> bool {
    use crate::constants::{UI_EDIT_BOX, XWM_NEWVALUE};

    type SetWindowText = unsafe extern "thiscall" fn(*mut CXWnd, *const CXStr);

    unsafe {
        let Some(edit) = window.as_ref() else { return false };
        if text.is_null() || eqlib::CXWnd_GetType(window) != UI_EDIT_BOX {
            return false;
        }
        let text = OwnedCXStr::new(text);
        virtual_fn::<SetWindowText>(window.cast(), CXWnd::VTABLE_SetWindowText)(window, text.as_ptr());
        parent_notify(edit, XWM_NEWVALUE, core::ptr::null_mut());
        crate::mq2main::WeDidStuff();
    }
    true
}

/// `CXWnd::WndNotification`, a virtual.
type WndNotification = unsafe extern "thiscall" fn(*mut CXWnd, *mut CXWnd, u32, *mut c_void) -> i32;

/// `window.ParentWndNotification(window, message, data)`, inline in eqlib:
/// tells the window's controller, then its parent, as if `window` sent it.
///
/// # Safety
/// In game, on the game thread; `window` live.
unsafe fn parent_notify(window: &CXWnd, message: u32, data: *mut c_void) {
    use crate::layouts::ControllerBase;
    type ControllerNotification = unsafe extern "thiscall" fn(*mut ControllerBase, *mut CXWnd, u32, *mut c_void);

    let sender = core::ptr::from_ref(window).cast_mut();
    unsafe {
        let controller = window.pController();
        if !controller.is_null() {
            virtual_fn::<ControllerNotification>(controller.cast(), ControllerBase::VTABLE_WndNotification)(controller, sender, message, data);
        }
        let parent = window.ParentWindow();
        if !parent.is_null() {
            virtual_fn::<WndNotification>(parent.cast(), CXWnd::VTABLE_WndNotification)(parent, sender, message, data);
        }
    }
}

/// The NUL-terminated text of a CXStr, valid while the CXStr lives.
///
/// # Safety
/// In game; `text` points to a live CXStr.
unsafe fn cxstr_bytes<'a>(text: *const CXStr) -> &'a [u8] {
    unsafe { CStr::from_ptr(eqlib::CXStr_c_str(text)).to_bytes() }
}

/// [`crate::game::copy_out`] as this module's text functions answer: the
/// text's whole length, or -1 for none.
///
/// # Safety
/// `out` must be writable for `cap` bytes.
unsafe fn copy_len(text: Option<&[u8]>, out: *mut c_char, cap: usize) -> i32 {
    unsafe { crate::game::copy_out(text, out, cap) };
    text.map_or(-1, |text| i32::try_from(text.len()).unwrap_or(i32::MAX))
}

/// Whether `index` is a valid index into an eqlib array of `count`.
fn in_range(index: i32, count: i32) -> bool {
    (0..count).contains(&index)
}

/// The notifications `/notify` sends by name (MQ2Windows.cpp's
/// `szWndNotification`), except `link`, whose data is text.
fn is_notify_message(message: u32) -> bool {
    matches!(
        message,
        XWM_LCLICK | XWM_LMOUSEUP | XWM_RCLICK | XWM_HITENTER | XWM_QMARKBOX | XWM_CLOSE | XWM_NEWVALUE | XWM_MENUSELECT | XWM_MOUSEOVER | XWM_HISTORY | XWM_LCLICKHOLD | XWM_MAXIMIZEBOX
    )
}

/// Whether the UI's XML manager exists; before it does, windows have no
/// kind or XML data.
///
/// # Safety
/// In game.
unsafe fn sidl_loaded() -> bool {
    unsafe { !foreign(&raw const eqlib::pSidlMgr).is_null() }
}

/// [`Api::mqemu_window_type`]: `CXWnd::GetType`.
///
/// # Safety
/// In game, on the game thread; `window` null or a live window.
pub unsafe extern "C" fn mqemu_window_type(window: *mut CXWnd) -> i32 {
    unsafe {
        if window.is_null() || !sidl_loaded() {
            return UI_UNKNOWN;
        }
        eqlib::CXWnd_GetType(window)
    }
}

/// [`Api::mqemu_window_xml_text`]: a name from `CXWnd::GetXMLData`, as
/// `${Window.Name}`, `.ScreenID` and `.Type` read it (none when empty).
///
/// # Safety
/// In game, on the game thread; `window` null or a live window, `out`
/// writable for `cap` bytes.
pub unsafe extern "C" fn mqemu_window_xml_text(window: *mut CXWnd, which: i32, out: *mut c_char, cap: usize) -> i32 {
    unsafe {
        if window.is_null() || !sidl_loaded() {
            return copy_len(None, out, cap);
        }
        let xml = eqlib::CXWnd_GetXMLData(window);
        let Some(data) = xml.as_ref() else { return copy_len(None, out, cap) };
        let text: *const CXStr = match which {
            XML_NAME => data.sItemName(),
            // CXMLData::ScreenID, which eqlib reads as a screen piece's
            // whatever the data is.
            XML_SCREEN_ID => (*xml.cast::<CParamScreenPiece>()).sScreenID(),
            XML_TYPE_NAME => data.sClassName(),
            _ => return copy_len(None, out, cap),
        };
        let text = cxstr_bytes(text);
        copy_len((!text.is_empty()).then_some(text), out, cap)
    }
}

/// [`Api::mqemu_window_tooltip`]: the virtual `GetTooltip`.
///
/// # Safety
/// In game, on the game thread; `window` null or a live window, `out`
/// writable for `cap` bytes.
pub unsafe extern "C" fn mqemu_window_tooltip(window: *mut CXWnd, out: *mut c_char, cap: usize) -> i32 {
    type GetTooltip = unsafe extern "thiscall" fn(*const CXWnd, *mut CXStr) -> *mut CXStr;
    unsafe {
        if window.is_null() {
            return copy_len(None, out, cap);
        }
        let tooltip = OwnedCXStr::returned(|tooltip| {
            virtual_fn::<GetTooltip>(window.cast(), CXWnd::VTABLE_GetTooltip)(window, tooltip);
        });
        copy_len(Some(cxstr_bytes(tooltip.as_ptr())), out, cap)
    }
}

const _: () = assert!(size_of::<CXRect>() == size_of::<mqemu_rect>());

/// [`Api::mqemu_window_screen_rect`]: `CXWnd::GetScreenRect`, constructed
/// straight into `out` (`mqemu_rect` is laid out as `CXRect`).
///
/// # Safety
/// In game, on the game thread; `window` null or a live window, `out` writable.
pub unsafe extern "C" fn mqemu_window_screen_rect(window: *mut CXWnd, out: *mut mqemu_rect) -> bool {
    unsafe {
        *out = mqemu_rect::default();
        if window.is_null() {
            return false;
        }
        eqlib::CXWnd_GetScreenRect(window, out.cast());
    }
    true
}

/// [`Api::mqemu_window_notify`]: the virtual `WndNotification`, setting a
/// slider first for `XWM_NEWVALUE` as `/notify` does.
///
/// # Safety
/// In game, on the game thread; `window` null or a live window, `sender`
/// null or a live window.
pub unsafe extern "C" fn mqemu_window_notify(window: *mut CXWnd, sender: *mut CXWnd, message: u32, data: i32) -> bool {
    unsafe {
        if window.is_null() || !is_notify_message(message) {
            return false;
        }
        // /notify casts any control to a slider here; only a slider is one.
        if message == XWM_NEWVALUE && !sender.is_null() && eqlib::CXWnd_GetType(sender) == UI_SLIDER {
            eqlib::CSliderWnd_SetValue(sender.cast::<CSliderWnd>(), data);
        }
        virtual_fn::<WndNotification>(window.cast(), CXWnd::VTABLE_WndNotification)(window, sender, message, data as usize as *mut c_void);
        crate::mq2main::WeDidStuff();
    }
    true
}

/// [`Api::mqemu_window_show`]: the virtual `Show(show, true, true)`.
///
/// # Safety
/// In game, on the game thread; `window` null or a live window.
pub unsafe extern "C" fn mqemu_window_show(window: *mut CXWnd, show: bool) -> bool {
    type Show = unsafe extern "thiscall" fn(*mut CXWnd, bool, bool, bool) -> i32;
    if window.is_null() {
        return false;
    }
    unsafe { virtual_fn::<Show>(window.cast(), CXWnd::VTABLE_Show)(window, show, true, true) };
    true
}

/// [`Api::mqemu_window_move`]: the virtual `UpdateGeometry(rect, true, true,
/// true, true)` (eqlib's inline `Move`).
///
/// # Safety
/// In game, on the game thread; `window` null or a live window, `rect` null
/// or readable.
pub unsafe extern "C" fn mqemu_window_move(window: *mut CXWnd, rect: *const mqemu_rect) -> bool {
    type UpdateGeometry = unsafe extern "thiscall" fn(*mut CXWnd, *const CXRect, bool, bool, bool, bool) -> i32;
    if window.is_null() || rect.is_null() {
        return false;
    }
    unsafe { virtual_fn::<UpdateGeometry>(window.cast(), CXWnd::VTABLE_UpdateGeometry)(window, rect.cast(), true, true, true, true) };
    true
}

/// [`Api::mqemu_window_refade`]: `CXWnd::Refade`.
///
/// # Safety
/// In game, on the game thread; `window` null or a live window.
pub unsafe extern "C" fn mqemu_window_refade(window: *mut CXWnd) -> bool {
    if window.is_null() {
        return false;
    }
    unsafe { eqlib::CXWnd_Refade(window) };
    true
}

/// [`Api::mqemu_list_item_text`]: `CListWnd::GetItemText`, for a row and
/// column the list has.
///
/// # Safety
/// In game, on the game thread; `list` null or a live list, `out` writable
/// for `cap` bytes.
pub unsafe extern "C" fn mqemu_list_item_text(list: *mut CListWnd, row: i32, column: i32, out: *mut c_char, cap: usize) -> i32 {
    unsafe {
        let Some(lines) = list.as_ref() else { return copy_len(None, out, cap) };
        if !in_range(row, lines.ItemsArray().m_length()) || !in_range(column, lines.Columns().m_length()) {
            return copy_len(None, out, cap);
        }
        let text = OwnedCXStr::returned(|text| {
            eqlib::CListWnd_GetItemText(list, text, row, column);
        });
        copy_len(Some(cxstr_bytes(text.as_ptr())), out, cap)
    }
}

/// [`Api::mqemu_tab_select`]: `CTabWnd::SetPage(page, true)`.
///
/// # Safety
/// In game, on the game thread; `tab` null or a live window.
pub unsafe extern "C" fn mqemu_tab_select(tab: *mut CTabWnd, page: i32) -> bool {
    unsafe {
        let Some(pages) = tab.as_ref() else { return false };
        if eqlib::CXWnd_GetType(pages.as_CXWnd()) != UI_TAB_BOX || !in_range(page, pages.PageArray().m_length()) {
            return false;
        }
        eqlib::CTabWnd_SetPage(tab, page, true);
        crate::mq2main::WeDidStuff();
    }
    true
}

/// [`Api::mqemu_menu_select`]: the context menu manager's `WndNotification`
/// of `XWM_LMOUSEUP` on the item, as clicking it does.
///
/// # Safety
/// In game, on the game thread; `menu` null or a live context menu.
pub unsafe extern "C" fn mqemu_menu_select(menu: *mut CContextMenu, item: i32) -> bool {
    unsafe {
        let manager = foreign(&raw const eqlib::pContextMenuManager);
        let Some(items) = menu.as_ref() else { return false };
        if manager.is_null() || !in_range(item, items.ItemsArray().m_length()) {
            return false;
        }
        let manager = (*manager).as_CXWnd();
        virtual_fn::<WndNotification>(manager.cast(), CXWnd::VTABLE_WndNotification)(manager, items.as_CXWnd(), XWM_LMOUSEUP, item as usize as *mut c_void);
    }
    true
}

/// [`Api::mqemu_menus_close`]: `CContextMenuManager::Flush`.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_menus_close() -> bool {
    unsafe {
        let manager = foreign(&raw const eqlib::pContextMenuManager);
        if manager.is_null() {
            return false;
        }
        eqlib::CContextMenuManager_Flush(manager);
    }
    true
}

/// [`Api::mqemu_window_notify_link`]: the virtual `WndNotification` of
/// `XWM_LINK` with the text itself, as `/notify ... link` sends it.
///
/// # Safety
/// In game, on the game thread; `window` null or a live window, `sender`
/// null or a live window, `text` null or NUL-terminated.
pub unsafe extern "C" fn mqemu_window_notify_link(window: *mut CXWnd, sender: *mut CXWnd, text: *const c_char) -> bool {
    if window.is_null() || text.is_null() {
        return false;
    }
    unsafe {
        virtual_fn::<WndNotification>(window.cast(), CXWnd::VTABLE_WndNotification)(window, sender, XWM_LINK, text.cast_mut().cast());
        crate::mq2main::WeDidStuff();
    }
    true
}

/// [`Api::mqemu_inv_slot_text`]: the virtual `GetText` of the slot's own
/// text object (`CInvSlotWnd::pTextObject`, not `CXWnd`'s).
///
/// # Safety
/// In game, on the game thread; `slot` null or a live window, `out` writable
/// for `cap` bytes.
pub unsafe extern "C" fn mqemu_inv_slot_text(slot: *mut CInvSlotWnd, out: *mut c_char, cap: usize) -> i32 {
    type GetText = unsafe extern "thiscall" fn(*mut CTextObjectInterface) -> *mut CXStr;
    unsafe {
        let Some(window) = slot.as_ref() else { return copy_len(None, out, cap) };
        let object = window.CInvSlotWnd_pTextObject();
        if eqlib::CXWnd_GetType(window.as_CXWnd()) != UI_INV_SLOT || object.is_null() {
            return copy_len(None, out, cap);
        }
        let text = virtual_fn::<GetText>(object.cast(), CTextObjectInterface::VTABLE_GetText)(object);
        copy_len(Some(cxstr_bytes(text)), out, cap)
    }
}

/// [`Api::mqemu_texture_animation_name`]: its `Name` (eqlib's inline `GetName`).
///
/// # Safety
/// In game; `animation` null or live, `out` writable for `cap` bytes.
pub unsafe extern "C" fn mqemu_texture_animation_name(animation: *mut CTextureAnimation, out: *mut c_char, cap: usize) -> i32 {
    unsafe {
        let Some(animation) = animation.as_ref() else { return copy_len(None, out, cap) };
        copy_len(Some(cxstr_bytes(animation.Name())), out, cap)
    }
}

/// [`Api::mqemu_hot_button_item`]: the item in `CHotButton::GetHotButtonData`.
///
/// # Safety
/// In game, on the game thread; `button` null or a live window.
pub unsafe extern "C" fn mqemu_hot_button_item(button: *mut CHotButton) -> *mut ItemClient {
    unsafe {
        let Some(window) = button.as_ref() else { return core::ptr::null_mut() };
        if eqlib::CXWnd_GetType(window.as_CXWnd()) != UI_HOT_BUTTON {
            return core::ptr::null_mut();
        }
        eqlib::CHotButton_GetHotButtonData(button).as_ref().map_or(core::ptr::null_mut(), |data| data.Item().m_pObject())
    }
}

// ----- finding windows (FindMQ2Window) -----

/// [`Api::mqemu_find_window`]: MacroQuest's `FindMQ2Window`, without paths.
///
/// # Safety
/// In game, on the game thread; `name` null or NUL-terminated.
pub unsafe extern "C" fn mqemu_find_window(name: *const c_char) -> *mut CXWnd {
    if name.is_null() {
        return core::ptr::null_mut();
    }
    let name = unsafe { CStr::from_ptr(name) }.to_bytes();
    if name.is_empty() || name.contains(&b'/') {
        return core::ptr::null_mut();
    }
    unsafe { find_window(name) }
}

/// `FindMQ2Window(name)` for a name without `/`. MacroQuest looks in a map
/// it keeps from detours (windows the UI's XML made, and every screen
/// `CSidlScreenWnd::Init` makes later), each under its XML name; this reads
/// the same windows from the window manager, as MacroQuest fills its map at
/// load. Else the bag names.
///
/// # Safety
/// In game, on the game thread.
unsafe fn find_window(name: &[u8]) -> *mut CXWnd {
    unsafe {
        if let Some(window) = screen_window(name) {
            return window;
        }
        let Some(bag) = bag_name(name) else { return core::ptr::null_mut() };
        foreign(&raw const eqlib::pContainerMgr).as_ref().map_or(core::ptr::null_mut(), |manager| bag_window(manager, bag).cast())
    }
}

/// The screen (`UI_Screen`) whose XML name is `name`, ignoring ASCII case: a
/// shown one first, else the first.
///
/// # Safety
/// In game, on the game thread.
unsafe fn screen_window(name: &[u8]) -> Option<*mut CXWnd> {
    unsafe {
        let manager = foreign(&raw const eqlib::pWndMgr).as_ref()?;
        if !sidl_loaded() {
            return None;
        }
        let windows = manager.pWindows();
        let windows = game_slice(windows.m_array(), windows.m_length());
        let named = |window: *mut CXWnd| {
            !window.is_null()
                && eqlib::CXWnd_GetXMLData(window).as_ref().is_some_and(|xml| xml.nClassIdx() == UI_SCREEN && cxstr_bytes(xml.sItemName()).eq_ignore_ascii_case(name))
        };
        first_shown(windows.iter().copied().filter(|window| named(*window)).map(|window| (window, (*window).dShow())))
    }
}

/// The first candidate that's shown, else the first; as MacroQuest picks
/// among windows that share a name.
fn first_shown<T: Copy>(candidates: impl Iterator<Item = (T, bool)>) -> Option<T> {
    let mut first = None;
    for (candidate, shown) in candidates {
        if shown {
            return Some(candidate);
        }
        first.get_or_insert(candidate);
    }
    first
}

/// The game's array of `len` at `data`; empty for null or a length below one.
///
/// # Safety
/// `data` null or valid for `len` elements, for `'a`.
unsafe fn game_slice<'a, T>(data: *const T, len: i32) -> &'a [T] {
    match usize::try_from(len) {
        Ok(len) if len > 0 && !data.is_null() => unsafe { core::slice::from_raw_parts(data, len) },
        _ => &[],
    }
}

/// The inventory slot of the first bag (`InvSlot_FirstBagSlot`, `pack1`).
const FIRST_BAG_SLOT: i16 = 23;

/// A bag window name `FindMQ2Window` takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BagName {
    /// `enviro`: the world container (a forge, a tradeskill container) open.
    World,
    /// `pack<n>`, `bank<n>`: the bag at a slot of a container.
    At(ItemContainerInstance, i16),
}

/// The bag `name` names (ASCII case ignored): `pack<n>` for inventory bag
/// slot n, `bank<n>` for bank slot n (counted from 1), or `enviro`.
/// MacroQuest's `bank<n>` asks the bank for slot n-2, which is a bug.
fn bag_name(name: &[u8]) -> Option<BagName> {
    if name.eq_ignore_ascii_case(b"enviro") {
        return Some(BagName::World);
    }
    let (prefix, number) = name.split_at_checked(4)?;
    if number.is_empty() || !number.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let n = core::str::from_utf8(number).ok()?.parse::<i16>().ok().filter(|n| *n >= 1)?;
    if prefix.eq_ignore_ascii_case(b"pack") {
        Some(BagName::At(ItemContainerInstance::eItemContainerPossessions, FIRST_BAG_SLOT.checked_add(n - 1)?))
    } else if prefix.eq_ignore_ascii_case(b"bank") {
        Some(BagName::At(ItemContainerInstance::eItemContainerBank, n - 1))
    } else {
        None
    }
}

/// The open window of bag `bag`, or null. MacroQuest finds the bag's item
/// and asks for the window holding it (`GetWindowForItem`); a bag's window
/// also knows where its bag is (`Location`, set with it), so this looks
/// for that. A window holds a bag while it's open (`Container`).
///
/// # Safety
/// `manager`'s window pointers null or live, as their bag pointers.
unsafe fn bag_window(manager: &CContainerMgr, bag: BagName) -> *mut CContainerWnd {
    let holds = |window: &CContainerWnd| {
        let item = window.Container().m_pObject();
        !item.is_null()
            && match bag {
                BagName::World => item == manager.WorldContainer().m_pObject(),
                BagName::At(kind, slot) => {
                    let location = window.CContainerWnd_Location();
                    location.Location() == kind && location.Index().m_slots() == [slot, -1, -1]
                }
            }
    };
    manager.pContainerWnds().into_iter().find(|window| unsafe { window.as_ref() }.is_some_and(holds)).unwrap_or(core::ptr::null_mut())
}

// ----- clicking (SendWndClick2) -----

/// The handlers [`mqemu_window_click`] calls for each click (MacroQuest's
/// click names, in order): vtable offsets of `CXWnd`'s mouse virtuals, far
/// before `CButtonWnd`'s undeclared one, so right for every window.
const CLICKS: [&[usize]; CLICK_KINDS] = [
    &[CXWnd::VTABLE_HandleLButtonDown],
    &[CXWnd::VTABLE_HandleLButtonDown, CXWnd::VTABLE_HandleLButtonUp],
    &[CXWnd::VTABLE_HandleLButtonDown, CXWnd::VTABLE_HandleLButtonHeld],
    &[CXWnd::VTABLE_HandleLButtonDown, CXWnd::VTABLE_HandleLButtonHeld, CXWnd::VTABLE_HandleLButtonUpAfterHeld],
    &[CXWnd::VTABLE_HandleRButtonDown],
    &[CXWnd::VTABLE_HandleRButtonDown, CXWnd::VTABLE_HandleRButtonUp],
    &[CXWnd::VTABLE_HandleRButtonDown, CXWnd::VTABLE_HandleRButtonHeld],
    &[CXWnd::VTABLE_HandleRButtonDown, CXWnd::VTABLE_HandleRButtonHeld, CXWnd::VTABLE_HandleRButtonUpAfterHeld],
];

/// A point on screen, laid out as eqlib's `CXPoint`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Point {
    x: i32,
    y: i32,
}

const _: () = assert!(size_of::<CXPoint>() == size_of::<Point>());

/// `CXRect::CenterPoint`.
fn center(rect: &mqemu_rect) -> Point {
    Point {
        x: rect.left.wrapping_add(rect.right.wrapping_sub(rect.left) / 2),
        y: rect.top.wrapping_add(rect.bottom.wrapping_sub(rect.top) / 2),
    }
}

/// [`Api::mqemu_window_click`]: MacroQuest's `SendWndClick2`: the click's
/// handlers, at the window's center on screen, with no keys down.
///
/// # Safety
/// In game, on the game thread; `window` null or a live window.
pub unsafe extern "C" fn mqemu_window_click(window: *mut CXWnd, click: i32) -> bool {
    type Handler = unsafe extern "thiscall" fn(*mut CXWnd, *const CXPoint, u32) -> i32;
    let Some(handlers) = usize::try_from(click).ok().and_then(|click| CLICKS.get(click)) else { return false };
    if window.is_null() {
        return false;
    }
    unsafe {
        let mut rect = mqemu_rect::default();
        eqlib::CXWnd_GetScreenRect(window, (&raw mut rect).cast());
        let point = center(&rect);
        for &handler in *handlers {
            virtual_fn::<Handler>(window.cast(), handler)(window, (&raw const point).cast(), 0);
        }
        crate::mq2main::WeDidStuff();
    }
    true
}

// ----- lists (SendListSelect2) -----

/// The list a `listselect` acts on in a window of kind `kind`: a list or
/// tree view itself, or a combo box's drop-down; and whether it's a combo
/// box's.
///
/// # Safety
/// `window` live, of kind `kind`.
unsafe fn select_list(kind: i32, window: *mut CXWnd) -> Option<(*mut CListWnd, bool)> {
    match kind {
        UI_LISTBOX | UI_TREE_VIEW => Some((window.cast(), false)),
        UI_COMBOBOX => {
            let list = unsafe { (*window.cast::<CComboWnd>()).pListWnd() };
            (!list.is_null()).then_some((list, true))
        }
        _ => None,
    }
}

/// [`Api::mqemu_list_select`]: MacroQuest's `SendListSelect2`, which
/// selects the row (`SetCurSel`, or a combo box's `SetChoice`), then tells
/// the list's parent of a click on the row now selected (and scrolls a list
/// to it). Rows below 0 are refused too, and nothing is said in chat.
///
/// # Safety
/// In game, on the game thread; `window` null or a live window.
pub unsafe extern "C" fn mqemu_list_select(window: *mut CXWnd, row: i32) -> bool {
    unsafe {
        if window.is_null() || row < 0 {
            return false;
        }
        let Some((list, combo)) = select_list(eqlib::CXWnd_GetType(window), window) else { return false };
        if !in_range(row, (*list).ItemsArray().m_length()) {
            return false;
        }
        if combo {
            eqlib::CComboWnd_SetChoice(window.cast(), row);
        } else {
            eqlib::CListWnd_SetCurSel(list, row);
        }
        let selected = eqlib::CListWnd_GetCurSel(list);
        parent_notify(&*(*list).as_CXWnd(), XWM_LCLICK, selected as usize as *mut c_void);
        if !combo {
            eqlib::CListWnd_EnsureVisible(list, selected);
        }
        crate::mq2main::WeDidStuff();
    }
    true
}

// ----- item slots and hot buttons -----

/// [`Api::mqemu_inv_slot_item`]: the slot's `LinkedItem`, else
/// `pLocalPC->GetItemByGlobalIndex(ItemLocation)`. That returns a
/// `VePointer` holding a reference, dropped here; the item lives on in its
/// container, as the C++ shim's `.get()` on the temporary relied on.
///
/// # Safety
/// In game, on the game thread; `slot` null or a live window.
pub unsafe extern "C" fn mqemu_inv_slot_item(slot: *mut CInvSlotWnd) -> *mut ItemClient {
    unsafe {
        let Some(window) = slot.as_ref() else { return core::ptr::null_mut() };
        if eqlib::CXWnd_GetType(window.as_CXWnd()) != UI_INV_SLOT {
            return core::ptr::null_mut();
        }
        let linked = window.LinkedItem().m_pObject();
        if !linked.is_null() {
            return linked;
        }
        let Some(pc) = foreign(&raw const eqlib::pLocalPC).as_ref() else { return core::ptr::null_mut() };
        let mut held = [0usize; size_of::<VePointer_ItemClient>().div_ceil(size_of::<usize>())];
        let held = held.as_mut_ptr().cast::<VePointer_ItemClient>();
        eqlib::CharacterBase_GetItemByGlobalIndex(pc.as_CharacterBase(), held, window.ItemLocation());
        let item = (*held).m_pObject();
        if let Some(reference) = item.as_ref() {
            eqlib::VeBaseReferenceCount_DecrementRefCount(reference.as_VeBaseReferenceCount());
        }
        item
    }
}

/// The name of hot bar `bar` (0-based) into `out`: `HotButtonWnd`, then
/// `HotButtonWnd2`...
fn hot_bar_name(bar: i32, out: &mut [u8; 24]) -> &[u8] {
    const BASE: &[u8] = b"HotButtonWnd";
    out[..BASE.len()].copy_from_slice(BASE);
    let mut len = BASE.len();
    if bar > 0 {
        let mut number = bar.unsigned_abs() + 1;
        let mut digits = [0u8; 10];
        let mut count = 0;
        loop {
            digits[count] = b'0' + (number % 10) as u8;
            count += 1;
            number /= 10;
            if number == 0 {
                break;
            }
        }
        for digit in digits[..count].iter().rev() {
            out[len] = *digit;
            len += 1;
        }
    }
    &out[..len]
}

/// [`Api::mqemu_hot_button_activate`]: as `${Window.HotButton.Activate}`,
/// the button's bar (found by name) presses it: `DoHotButton(index, false)`.
///
/// # Safety
/// In game, on the game thread; `button` null or a live window.
pub unsafe extern "C" fn mqemu_hot_button_activate(button: *mut CHotButton) -> bool {
    unsafe {
        let Some(window) = button.as_ref() else { return false };
        if eqlib::CXWnd_GetType(window.as_CXWnd()) != UI_HOT_BUTTON {
            return false;
        }
        let mut name = [0; 24];
        let bar = find_window(hot_bar_name(window.BarIndex(), &mut name));
        if bar.is_null() {
            return false;
        }
        eqlib::CHotButtonWnd_DoHotButton(bar.cast::<CHotButtonWnd>(), window.ButtonIndex(), 0);
    }
    true
}

// ----- drawing on a window -----

/// `pWndMgr->GetFont(font)`, inline in eqlib: font `font` of the manager's
/// `FontsArray`, or null. eqlib checks only the top and throws below 0 (as
/// for a null array); this answers null.
///
/// # Safety
/// `manager`'s font array null or valid for its length.
unsafe fn font_at(manager: &CXWndManager, font: i32) -> *mut CTextureFont {
    let fonts = manager.FontsArray();
    let fonts = unsafe { game_slice(fonts.m_array(), fonts.m_length()) };
    usize::try_from(font).ok().and_then(|font| fonts.get(font)).copied().unwrap_or(core::ptr::null_mut())
}

/// The window manager's font `font`, or null (as the C++ shim's `eq_font`).
///
/// # Safety
/// In game, on the game thread.
unsafe fn eq_font(font: i32) -> *mut CTextureFont {
    unsafe { foreign(&raw const eqlib::pWndMgr).as_ref().map_or(core::ptr::null_mut(), |manager| font_at(manager, font)) }
}

/// [`Api::mqemu_window_client_clip_rect`]: the virtual `GetClientClipRect`
/// (a list has its own), constructed straight into `out`.
///
/// # Safety
/// In game, on the game thread; `window` null or a live window, `out` null
/// or writable.
pub unsafe extern "C" fn mqemu_window_client_clip_rect(window: *mut CXWnd, out: *mut mqemu_rect) -> bool {
    type GetClientClipRect = unsafe extern "thiscall" fn(*const CXWnd, *mut CXRect) -> *mut CXRect;
    if window.is_null() || out.is_null() {
        return false;
    }
    unsafe { virtual_fn::<GetClientClipRect>(window.cast(), CXWnd::VTABLE_GetClientClipRect)(window, out.cast()) };
    true
}

/// [`Api::mqemu_draw_fill_rect`]: the game's static `CXWnd::DrawColoredRect`,
/// through its address (eqlib doesn't define it).
///
/// # Safety
/// In game, on the game thread, while the UI draws; `rect` and `clip` null
/// or readable.
pub unsafe extern "C" fn mqemu_draw_fill_rect(rect: *const mqemu_rect, color: u32, clip: *const mqemu_rect) -> bool {
    type DrawColoredRect = unsafe extern "C" fn(*const CXRect, u32, *const CXRect);
    unsafe {
        let address = eqlib::CXWnd__DrawColoredRect;
        if rect.is_null() || clip.is_null() || address == 0 {
            return false;
        }
        core::mem::transmute::<usize, DrawColoredRect>(address)(rect.cast(), color, clip.cast());
    }
    true
}

/// [`Api::mqemu_draw_text`]: `CTextureFont::DrawWrappedText(text, rect,
/// clip, color, flags, 0)`, flags cut to 16 bits as the C++ took them.
///
/// # Safety
/// In game, on the game thread, while the UI draws; `text` null or
/// NUL-terminated, `rect` and `clip` null or readable.
pub unsafe extern "C" fn mqemu_draw_text(font: i32, text: *const c_char, rect: *const mqemu_rect, clip: *const mqemu_rect, color: u32, flags: u32) -> bool {
    unsafe {
        let font = eq_font(font);
        if font.is_null() || text.is_null() || rect.is_null() || clip.is_null() {
            return false;
        }
        let text = OwnedCXStr::new(text);
        eqlib::CTextureFont_DrawWrappedText(font, text.as_ptr(), rect.cast(), clip.cast(), color, flags as u16, 0);
    }
    true
}

/// [`Api::mqemu_text_width`]: `CTextureFont::GetTextExtent`.
///
/// # Safety
/// In game, on the game thread; `text` null or NUL-terminated.
pub unsafe extern "C" fn mqemu_text_width(font: i32, text: *const c_char) -> i32 {
    unsafe {
        let font = eq_font(font);
        if font.is_null() || text.is_null() {
            return -1;
        }
        let text = OwnedCXStr::new(text);
        eqlib::CTextureFont_GetTextExtent(font, text.as_ptr())
    }
}

/// [`Api::mqemu_font_height`]: `CTextureFont::GetHeight`.
///
/// # Safety
/// In game, on the game thread.
pub unsafe extern "C" fn mqemu_font_height(font: i32) -> i32 {
    unsafe {
        let font = eq_font(font);
        if font.is_null() { -1 } else { eqlib::CTextureFont_GetHeight(font) }
    }
}

/// [`Api::mqemu_draw_texture`]: `CTextureAnimation::Draw`.
///
/// # Safety
/// In game, on the game thread, while the UI draws; `animation` null or
/// live, `rect` and `clip` null or readable.
pub unsafe extern "C" fn mqemu_draw_texture(animation: *mut CTextureAnimation, rect: *const mqemu_rect, clip: *const mqemu_rect, color: u32, color2: u32) -> bool {
    if animation.is_null() || rect.is_null() || clip.is_null() {
        return false;
    }
    unsafe { eqlib::CTextureAnimation_Draw(animation, rect.cast(), clip.cast(), color, color2) };
    true
}

unsafe extern "C" {
    /// The vtable entry (counted in pointers from the first) a subclass
    /// ([`mqemu_window_subclass`]) replaces for hook kind `kind`; -1 for none.
    /// For tests; it imports nothing, so it links anywhere.
    pub fn mqemu_window_hook_slot(kind: i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;
    use std::{vec, vec::Vec};

    /// Zeroed storage for a game object of type `T`.
    fn zeroed<T>() -> (Vec<u64>, *mut T) {
        let mut storage = vec![0u64; size_of::<T>().div_ceil(8)];
        let ptr = storage.as_mut_ptr().cast();
        (storage, ptr)
    }

    #[test]
    fn first_shown_prefers_a_shown_window_else_the_first() {
        assert_eq!(first_shown([(1, false), (2, true), (3, true)].into_iter()), Some(2));
        assert_eq!(first_shown([(1, false), (2, false)].into_iter()), Some(1));
        assert_eq!(first_shown(core::iter::empty::<(i32, bool)>()), None);
    }

    #[test]
    fn game_slice_is_empty_for_null_or_no_length() {
        let data = [1, 2, 3];
        assert_eq!(unsafe { game_slice(data.as_ptr(), 2) }, [1, 2]);
        assert!(unsafe { game_slice(data.as_ptr(), 0) }.is_empty());
        assert!(unsafe { game_slice(data.as_ptr(), -1) }.is_empty());
        assert!(unsafe { game_slice(core::ptr::null::<i32>(), 3) }.is_empty());
    }

    #[test]
    fn font_at_reads_the_font_array_in_range() {
        let (_manager_storage, manager) = zeroed::<CXWndManager>();
        let manager = unsafe { &*manager };
        assert!(unsafe { font_at(manager, 0) }.is_null(), "no fonts");
        let (_a_storage, a) = zeroed::<CTextureFont>();
        let (_b_storage, b) = zeroed::<CTextureFont>();
        let mut fonts = [a, b];
        manager.FontsArray().set_m_array(fonts.as_mut_ptr());
        assert!(unsafe { font_at(manager, 0) }.is_null(), "no length");
        manager.FontsArray().set_m_length(2);
        assert_eq!(unsafe { font_at(manager, 0) }, a);
        assert_eq!(unsafe { font_at(manager, 1) }, b);
        assert!(unsafe { font_at(manager, 2) }.is_null());
        assert!(unsafe { font_at(manager, -1) }.is_null(), "where eqlib throws");
        manager.FontsArray().set_m_array(core::ptr::null_mut());
        assert!(unsafe { font_at(manager, 0) }.is_null(), "a null array");
    }

    #[test]
    fn bag_names_are_pack_bank_and_enviro() {
        use ItemContainerInstance as Kind;
        assert_eq!(bag_name(b"pack1"), Some(BagName::At(Kind::eItemContainerPossessions, 23)));
        assert_eq!(bag_name(b"PACK10"), Some(BagName::At(Kind::eItemContainerPossessions, 32)));
        assert_eq!(bag_name(b"bank1"), Some(BagName::At(Kind::eItemContainerBank, 0)), "not MacroQuest's slot -1");
        assert_eq!(bag_name(b"Bank24"), Some(BagName::At(Kind::eItemContainerBank, 23)));
        assert_eq!(bag_name(b"Enviro"), Some(BagName::World));
        for name in [&b"pack"[..], b"pack0", b"pack-1", b"pack+1", b"pack1x", b"pak1", b"bank", b"enviro1", b"pack99999", b"", b"InventoryWindow"] {
            assert_eq!(bag_name(name), None, "{}", std::string::String::from_utf8_lossy(name));
        }
    }

    #[test]
    fn bag_window_finds_the_open_window_on_the_bag() {
        use ItemContainerInstance as Kind;
        let (_manager_storage, manager) = zeroed::<CContainerMgr>();
        let (_a_storage, a) = zeroed::<CContainerWnd>();
        let (_b_storage, b) = zeroed::<CContainerWnd>();
        let (_bag_storage, bag) = zeroed::<ItemClient>();
        let (_world_storage, world) = zeroed::<ItemClient>();
        let (manager_ref, a_ref, b_ref) = unsafe { (&*manager, &*a, &*b) };
        let pack1 = BagName::At(Kind::eItemContainerPossessions, 23);
        let find = |name| unsafe { bag_window(manager_ref, name) };
        assert!(find(pack1).is_null(), "no windows");

        // Window 3 was on pack1 but is closed; window 7 is on bank slot 1.
        manager_ref.set_pContainerWnds_at(3, a);
        manager_ref.set_pContainerWnds_at(7, b);
        a_ref.CContainerWnd_Location().set_Location(Kind::eItemContainerPossessions);
        a_ref.CContainerWnd_Location().Index().set_m_slots([23, -1, -1]);
        b_ref.CContainerWnd_Location().set_Location(Kind::eItemContainerBank);
        b_ref.CContainerWnd_Location().Index().set_m_slots([0, -1, -1]);
        b_ref.Container().set_m_pObject(bag);
        assert!(find(pack1).is_null(), "closed");
        a_ref.Container().set_m_pObject(bag);
        assert_eq!(find(pack1), a);
        assert_eq!(find(BagName::At(Kind::eItemContainerBank, 0)), b);
        assert!(find(BagName::At(Kind::eItemContainerPossessions, 24)).is_null());
        assert!(find(BagName::At(Kind::eItemContainerBank, 23)).is_null(), "the bank's slot, not pack1's");

        // A bag in a bag isn't pack1.
        a_ref.CContainerWnd_Location().Index().set_m_slots([23, 0, -1]);
        assert!(find(pack1).is_null());

        assert!(find(BagName::World).is_null(), "no world container");
        manager_ref.WorldContainer().set_m_pObject(world);
        assert!(find(BagName::World).is_null(), "its window isn't open");
        b_ref.Container().set_m_pObject(world);
        assert_eq!(find(BagName::World), b);
    }

    #[test]
    fn clicks_press_then_release_or_hold_as_their_names_say() {
        use CXWnd as W;
        assert_eq!(CLICKS[0], [W::VTABLE_HandleLButtonDown]);
        assert_eq!(CLICKS[1], [W::VTABLE_HandleLButtonDown, W::VTABLE_HandleLButtonUp]);
        assert_eq!(CLICKS[3], [W::VTABLE_HandleLButtonDown, W::VTABLE_HandleLButtonHeld, W::VTABLE_HandleLButtonUpAfterHeld]);
        assert_eq!(CLICKS[5], [W::VTABLE_HandleRButtonDown, W::VTABLE_HandleRButtonUp]);
        assert_eq!(CLICKS[7], [W::VTABLE_HandleRButtonDown, W::VTABLE_HandleRButtonHeld, W::VTABLE_HandleRButtonUpAfterHeld]);
        // Below CButtonWnd's undeclared virtual at 0x164, where the PDB's
        // offsets go wrong.
        assert!(CLICKS.iter().flat_map(|c| c.iter()).all(|offset| *offset < 0x164));
    }

    #[test]
    fn center_is_the_rects_middle_rounded_toward_its_corner() {
        assert_eq!(center(&mqemu_rect { left: 10, top: 20, right: 31, bottom: 40 }), Point { x: 20, y: 30 });
        assert_eq!(center(&mqemu_rect::default()), Point { x: 0, y: 0 });
    }

    #[test]
    fn listselect_acts_on_a_list_or_a_combo_boxs_drop_down() {
        let (_combo_storage, combo) = zeroed::<CComboWnd>();
        let (_list_storage, list) = zeroed::<CListWnd>();
        let window = combo.cast::<CXWnd>();
        unsafe {
            assert_eq!(select_list(UI_LISTBOX, window), Some((window.cast(), false)));
            assert_eq!(select_list(UI_TREE_VIEW, window), Some((window.cast(), false)));
            assert_eq!(select_list(UI_COMBOBOX, window), None, "no drop-down");
            (*combo).set_pListWnd(list);
            assert_eq!(select_list(UI_COMBOBOX, window), Some((list, true)));
            assert_eq!(select_list(UI_TAB_BOX, window), None);
        }
    }

    #[test]
    fn hot_bars_are_named_from_hot_button_wnd() {
        let mut out = [0; 24];
        assert_eq!(hot_bar_name(0, &mut out), b"HotButtonWnd");
        assert_eq!(hot_bar_name(-1, &mut out), b"HotButtonWnd");
        assert_eq!(hot_bar_name(1, &mut out), b"HotButtonWnd2");
        assert_eq!(hot_bar_name(9, &mut out), b"HotButtonWnd10");
        assert_eq!(hot_bar_name(i32::MAX, &mut out), b"HotButtonWnd2147483648");
    }

    #[test]
    fn notify_takes_the_messages_notify_names_but_not_link() {
        for message in [XWM_LCLICK, XWM_LMOUSEUP, XWM_RCLICK, XWM_HITENTER, XWM_QMARKBOX, XWM_CLOSE, XWM_NEWVALUE, XWM_MENUSELECT, XWM_MOUSEOVER, XWM_HISTORY, XWM_LCLICKHOLD, XWM_MAXIMIZEBOX] {
            assert!(is_notify_message(message), "{message}");
        }
        for message in [0, XWM_LINK, 4, 100] {
            assert!(!is_notify_message(message), "{message}");
        }
    }

    #[test]
    fn in_range_is_zero_to_count() {
        assert!(in_range(0, 1));
        assert!(in_range(2, 3));
        assert!(!in_range(3, 3));
        assert!(!in_range(-1, 3));
        assert!(!in_range(0, 0));
        assert!(!in_range(0, -1));
    }

    #[test]
    fn copy_len_returns_the_whole_length_or_minus_one() {
        let mut out = [b'x' as c_char; 4];
        assert_eq!(unsafe { copy_len(Some(b"hello"), out.as_mut_ptr(), out.len()) }, 5);
        assert_eq!(out.map(|c| c as u8), *b"hel\0");
        assert_eq!(unsafe { copy_len(Some(b""), out.as_mut_ptr(), out.len()) }, 0);
        assert_eq!(out[0], 0);
        out[0] = b'x' as c_char;
        assert_eq!(unsafe { copy_len(None, out.as_mut_ptr(), out.len()) }, -1);
        assert_eq!(out[0], 0);
        assert_eq!(unsafe { copy_len(Some(b"abc"), out.as_mut_ptr(), 0) }, 3);
    }
}
