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

use crate::constants::{UI_HOT_BUTTON, UI_INV_SLOT, UI_SLIDER, UI_TAB_BOX, UI_UNKNOWN, XWM_CLOSE, XWM_HISTORY, XWM_HITENTER, XWM_LCLICK, XWM_LCLICKHOLD, XWM_LINK, XWM_LMOUSEUP, XWM_MAXIMIZEBOX, XWM_MENUSELECT, XWM_MOUSEOVER, XWM_NEWVALUE, XWM_QMARKBOX, XWM_RCLICK};
use crate::eqlib;
use crate::layouts::{CContextMenu, CHotButton, CInvSlotWnd, CListWnd, CParamScreenPiece, CSliderWnd, CTabWnd, CTextObjectInterface, CTextureAnimation, CXRect, CXStr, CXWnd, ItemClient};
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
    use crate::layouts::ControllerBase;

    type SetWindowText = unsafe extern "thiscall" fn(*mut CXWnd, *const CXStr);
    type ControllerNotification = unsafe extern "thiscall" fn(*mut ControllerBase, *mut CXWnd, u32, *mut c_void);

    unsafe {
        let Some(edit) = window.as_ref() else { return false };
        if text.is_null() || eqlib::CXWnd_GetType(window) != UI_EDIT_BOX {
            return false;
        }
        let text = OwnedCXStr::new(text);
        virtual_fn::<SetWindowText>(window.cast(), CXWnd::VTABLE_SetWindowText)(window, text.as_ptr());

        // CXWnd::ParentWndNotification, inline in eqlib.
        let controller = edit.pController();
        if !controller.is_null() {
            virtual_fn::<ControllerNotification>(controller.cast(), ControllerBase::VTABLE_WndNotification)(controller, window, XWM_NEWVALUE, core::ptr::null_mut());
        }
        let parent = edit.ParentWindow();
        if !parent.is_null() {
            virtual_fn::<WndNotification>(parent.cast(), CXWnd::VTABLE_WndNotification)(parent, window, XWM_NEWVALUE, core::ptr::null_mut());
        }
        crate::mq2main::WeDidStuff();
    }
    true
}

/// `CXWnd::WndNotification`, a virtual.
type WndNotification = unsafe extern "thiscall" fn(*mut CXWnd, *mut CXWnd, u32, *mut c_void) -> i32;

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

unsafe extern "C" {
    /// The vtable entry (counted in pointers from the first) a subclass
    /// ([`mqemu_window_subclass`]) replaces for hook kind `kind`; -1 for none.
    /// For tests; it imports nothing, so it links anywhere.
    pub fn mqemu_window_hook_slot(kind: i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

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
