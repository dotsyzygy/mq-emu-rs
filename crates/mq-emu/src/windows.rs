//! EverQuest's own UI windows (`CXWnd` and its subclasses): finding them as
//! `${Window[...]}` does, reading them, and clicking and notifying them as
//! `/notify` does.
//!
//! A [`Window`] borrows from the callback's [`Mq`], like a [`Spawn`]: the
//! game may free windows between frames (when the UI reloads, say), so they
//! can't be kept. Keep the name instead and find the window again. Within a
//! callback a window stays valid even after clicking it: the game deletes
//! the windows it destroys later, after its own frame (`bMarkedForDelete`).
//!
//! [`Spawn`]: crate::Spawn

use std::ffi::c_char;
use std::fmt;

use mq_emu_sys::constants::*;
use mq_emu_sys::layouts::{
    ArrayClass2_CXWnd, CButtonWnd, CComboWnd, CContextMenu, CContextMenuManager, CGaugeWnd, CListWnd, CPageWnd, CStmlWnd, CTabWnd, CXRect, CXWnd,
    CXWndManager, SListWndLine,
};
use mq_emu_sys::windows::{Api, XML_NAME, XML_SCREEN_ID, XML_TYPE_NAME, mqemu_rect};

use crate::game_fns::game_array;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::{from_c_array, from_cxstr, to_c};

mod hot_buttons;
mod item_slots;
mod plugin_windows;
mod trade;

pub use hot_buttons::{HotBar, HotButtonType, HotButtonWindow};
pub use item_slots::{ItemContainerKind, ItemSlotWindow, SlotLocation};
pub(crate) use plugin_windows::{PluginWindowSlot, WindowNotice};
pub use plugin_windows::{PluginWindowId, WindowEvent};
pub use trade::TradeWindow;

/// The most windows [`Window::descendants`] walks, so a damaged window tree
/// (a loop in it) can't hang the game. Far above any real window's count.
pub const MAX_DESCENDANTS: usize = 65536;

/// Runs a shim function that writes NUL-terminated text into a buffer and
/// returns its whole length (or -1 for none), growing the buffer to fit.
pub(crate) fn long_text(f: impl Fn(*mut c_char, usize) -> i32) -> Option<String> {
    let mut buffer = vec![0 as c_char; 256];
    // The text can't change between calls (it's all on the game thread), so
    // the second call fits; the third only guards against a broken function.
    for _ in 0..3 {
        let len = usize::try_from(f(buffer.as_mut_ptr(), buffer.len())).ok()?;
        if len < buffer.len() {
            return Some(from_c_array(&buffer));
        }
        buffer.resize(len + 1, 0);
    }
    None
}

/// What kind of window a [`Window`] is (`CXWnd::GetType`, the UI's XML
/// type). [`name`](Self::name) is what `${Window.Type}` says for windows the
/// XML created.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowKind {
    /// A window the UI's XML didn't create (MacroQuest calls it "CXWnd").
    Unknown,
    StaticScreenPiece,
    StaticAnimation,
    StaticTintedBlendAnimation,
    StaticText,
    StaticFrame,
    StaticHeader,
    /// A list; see [`Window::as_list`].
    Listbox,
    /// A button or a checkbox; see [`Window::is_checked`].
    Button,
    /// A gauge such as an HP bar; see [`Window::gauge_value`].
    Gauge,
    SpellGem,
    HtmlComponent,
    /// An item slot (inventory, bags, the bank, loot, trade).
    InvSlot,
    /// A text input; see [`Window::set_text`].
    EditBox,
    Slider,
    Label,
    /// Rich text (STML); [`Window::text`] reads its markup.
    StmlBox,
    /// A tree of rows; see [`Window::as_list`].
    TreeView,
    /// A drop-down list; see [`Window::as_list`].
    Combobox,
    /// One page of a tab box; [`Window::text`] reads its tab's text.
    Page,
    /// Tabs; see [`Window::as_tabs`].
    TabBox,
    LayoutBox,
    HorizontalLayoutBox,
    VerticalLayoutBox,
    FinderBox,
    TileLayoutBox,
    /// A top-level window ("screen") such as the inventory window.
    Screen,
    /// A hot bar button.
    HotButton,
    /// Another `UIType` value.
    Other(i32),
}

impl WindowKind {
    pub fn from_raw(raw: i32) -> WindowKind {
        use WindowKind::*;
        match raw {
            UI_UNKNOWN => Unknown,
            UI_STATIC_SCREEN_PIECE => StaticScreenPiece,
            UI_STATIC_ANIMATION => StaticAnimation,
            UI_STATIC_TINTED_BLEND_ANIMATION => StaticTintedBlendAnimation,
            UI_STATIC_TEXT => StaticText,
            UI_STATIC_FRAME => StaticFrame,
            UI_STATIC_HEADER => StaticHeader,
            UI_LISTBOX => Listbox,
            UI_BUTTON => Button,
            UI_GAUGE => Gauge,
            UI_SPELL_GEM => SpellGem,
            UI_HTML_COMPONENT => HtmlComponent,
            UI_INV_SLOT => InvSlot,
            UI_EDIT_BOX => EditBox,
            UI_SLIDER => Slider,
            UI_LABEL => Label,
            UI_STML_BOX => StmlBox,
            UI_TREE_VIEW => TreeView,
            UI_COMBOBOX => Combobox,
            UI_PAGE => Page,
            UI_TAB_BOX => TabBox,
            UI_LAYOUT_BOX => LayoutBox,
            UI_HORIZONTAL_LAYOUT_BOX => HorizontalLayoutBox,
            UI_VERTICAL_LAYOUT_BOX => VerticalLayoutBox,
            UI_FINDER_BOX => FinderBox,
            UI_TILE_LAYOUT_BOX => TileLayoutBox,
            UI_SCREEN => Screen,
            UI_HOT_BUTTON => HotButton,
            other => Other(other),
        }
    }

    pub fn raw(self) -> i32 {
        use WindowKind::*;
        match self {
            Unknown => UI_UNKNOWN,
            StaticScreenPiece => UI_STATIC_SCREEN_PIECE,
            StaticAnimation => UI_STATIC_ANIMATION,
            StaticTintedBlendAnimation => UI_STATIC_TINTED_BLEND_ANIMATION,
            StaticText => UI_STATIC_TEXT,
            StaticFrame => UI_STATIC_FRAME,
            StaticHeader => UI_STATIC_HEADER,
            Listbox => UI_LISTBOX,
            Button => UI_BUTTON,
            Gauge => UI_GAUGE,
            SpellGem => UI_SPELL_GEM,
            HtmlComponent => UI_HTML_COMPONENT,
            InvSlot => UI_INV_SLOT,
            EditBox => UI_EDIT_BOX,
            Slider => UI_SLIDER,
            Label => UI_LABEL,
            StmlBox => UI_STML_BOX,
            TreeView => UI_TREE_VIEW,
            Combobox => UI_COMBOBOX,
            Page => UI_PAGE,
            TabBox => UI_TAB_BOX,
            LayoutBox => UI_LAYOUT_BOX,
            HorizontalLayoutBox => UI_HORIZONTAL_LAYOUT_BOX,
            VerticalLayoutBox => UI_VERTICAL_LAYOUT_BOX,
            FinderBox => UI_FINDER_BOX,
            TileLayoutBox => UI_TILE_LAYOUT_BOX,
            Screen => UI_SCREEN,
            HotButton => UI_HOT_BUTTON,
            Other(raw) => raw,
        }
    }

    /// The kind's name as eqlib's `UITypeToString` gives it ("Listbox",
    /// "Editbox", "STMLbox"; "CXWnd" for [`Unknown`](Self::Unknown)).
    pub fn name(self) -> &'static str {
        use WindowKind::*;
        match self {
            Unknown => "CXWnd",
            StaticScreenPiece => "StaticScreenPiece",
            StaticAnimation => "StaticAnimation",
            StaticTintedBlendAnimation => "StaticTintedBlendAnimation",
            StaticText => "StaticText",
            StaticFrame => "StaticFrame",
            StaticHeader => "StaticHeader",
            Listbox => "Listbox",
            Button => "Button",
            Gauge => "Gauge",
            SpellGem => "SpellGem",
            HtmlComponent => "HtmlComponent",
            InvSlot => "InvSlot",
            EditBox => "Editbox",
            Slider => "Slider",
            Label => "Label",
            StmlBox => "STMLbox",
            TreeView => "TreeView",
            Combobox => "Combobox",
            Page => "Page",
            TabBox => "TabBox",
            LayoutBox => "LayoutBox",
            HorizontalLayoutBox => "HorizontalLayoutBox",
            VerticalLayoutBox => "VerticalLayoutBox",
            FinderBox => "FinderBox",
            TileLayoutBox => "TileLayoutBox",
            Screen => "Screen",
            HotButton => "HotButton",
            Other(_) => "Unknown",
        }
    }
}

/// A rectangle in pixels: right and bottom are one past the last pixel, as
/// the game's `CXRect` has them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct WindowRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WindowRect {
    /// A rectangle `width` by `height` with its top left corner at (`x`, `y`).
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> WindowRect {
        WindowRect { left: x, top: y, right: x + width, bottom: y + height }
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }

    fn from_cxrect(rect: &CXRect) -> WindowRect {
        WindowRect { left: rect.left(), top: rect.top(), right: rect.right(), bottom: rect.bottom() }
    }
}

/// A window's scroll bar: where it is, out of how far it goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Scroll {
    pub position: i32,
    pub max: i32,
}

/// A simulated mouse click on a window's center, as `/notify <window>
/// <control> <click>` and `${Window.LeftMouseUp}` and the like send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Click {
    /// The left button going down only (`leftmouse`).
    LeftDown,
    /// A left click: down, then up (`leftmouseup`). What clicking a button takes.
    Left,
    /// The left button going down and being held (`leftmouseheld`).
    LeftHeld,
    /// Down, held, then up after holding (`leftmouseheldup`).
    LeftHeldUp,
    /// The right button going down only (`rightmouse`).
    RightDown,
    /// A right click: down, then up (`rightmouseup`).
    Right,
    /// The right button going down and being held (`rightmouseheld`).
    RightHeld,
    /// Down, held, then up after holding (`rightmouseheldup`).
    RightHeldUp,
}

impl Click {
    pub const ALL: [Click; 8] =
        [Click::LeftDown, Click::Left, Click::LeftHeld, Click::LeftHeldUp, Click::RightDown, Click::Right, Click::RightHeld, Click::RightHeldUp];

    /// `/notify`'s name for the click.
    pub fn name(self) -> &'static str {
        match self {
            Click::LeftDown => "leftmouse",
            Click::Left => "leftmouseup",
            Click::LeftHeld => "leftmouseheld",
            Click::LeftHeldUp => "leftmouseheldup",
            Click::RightDown => "rightmouse",
            Click::Right => "rightmouseup",
            Click::RightHeld => "rightmouseheld",
            Click::RightHeldUp => "rightmouseheldup",
        }
    }

    /// The click `/notify` names so, ignoring case.
    pub fn from_name(name: &str) -> Option<Click> {
        Click::ALL.into_iter().find(|click| click.name().eq_ignore_ascii_case(name))
    }

    fn index(self) -> i32 {
        Click::ALL.iter().position(|click| *click == self).expect("in ALL") as i32
    }
}

/// A notification a window gets from one of its controls (`XWM_*`), the
/// ones `/notify` sends by name. See [`Window::notify`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Notification {
    /// `leftmouse` (`XWM_LCLICK`): a control was clicked; for a list, the
    /// data is the row.
    LeftClick,
    /// `leftmouseup` (`XWM_LMOUSEUP`).
    LeftMouseUp,
    /// `rightmouse` (`XWM_RCLICK`).
    RightClick,
    /// `enter` (`XWM_HITENTER`): Enter was pressed in an edit box.
    Enter,
    /// `help` (`XWM_QMARKBOX`): the question mark box.
    Help,
    /// `close` (`XWM_CLOSE`): the close box.
    Close,
    /// `newvalue` (`XWM_NEWVALUE`): a control's value changed. From a
    /// slider, the data is its new value, and the slider is set to it first.
    NewValue,
    /// `contextmenu` (`XWM_MENUSELECT`).
    ContextMenu,
    /// `mouseover` (`XWM_MOUSEOVER`).
    MouseOver,
    /// `history` (`XWM_HISTORY`).
    History,
    /// `leftmousehold` (`XWM_LCLICKHOLD`).
    LeftHold,
    /// `resetdefaultposition` (`XWM_MAXIMIZEBOX`).
    ResetDefaultPosition,
}

impl Notification {
    pub const ALL: [Notification; 12] = [
        Notification::LeftClick,
        Notification::LeftMouseUp,
        Notification::RightClick,
        Notification::Enter,
        Notification::Help,
        Notification::Close,
        Notification::NewValue,
        Notification::ContextMenu,
        Notification::MouseOver,
        Notification::History,
        Notification::LeftHold,
        Notification::ResetDefaultPosition,
    ];

    /// `/notify`'s name for it.
    pub fn name(self) -> &'static str {
        match self {
            Notification::LeftClick => "leftmouse",
            Notification::LeftMouseUp => "leftmouseup",
            Notification::RightClick => "rightmouse",
            Notification::Enter => "enter",
            Notification::Help => "help",
            Notification::Close => "close",
            Notification::NewValue => "newvalue",
            Notification::ContextMenu => "contextmenu",
            Notification::MouseOver => "mouseover",
            Notification::History => "history",
            Notification::LeftHold => "leftmousehold",
            Notification::ResetDefaultPosition => "resetdefaultposition",
        }
    }

    /// The notification `/notify` names so, ignoring case.
    pub fn from_name(name: &str) -> Option<Notification> {
        Notification::ALL.into_iter().find(|n| n.name().eq_ignore_ascii_case(name))
    }

    /// The notification whose message (`XWM_*`) this is.
    pub fn from_raw(raw: u32) -> Option<Notification> {
        Notification::ALL.into_iter().find(|n| n.raw() == raw)
    }

    /// The message (`XWM_*`).
    pub fn raw(self) -> u32 {
        match self {
            Notification::LeftClick => XWM_LCLICK,
            Notification::LeftMouseUp => XWM_LMOUSEUP,
            Notification::RightClick => XWM_RCLICK,
            Notification::Enter => XWM_HITENTER,
            Notification::Help => XWM_QMARKBOX,
            Notification::Close => XWM_CLOSE,
            Notification::NewValue => XWM_NEWVALUE,
            Notification::ContextMenu => XWM_MENUSELECT,
            Notification::MouseOver => XWM_MOUSEOVER,
            Notification::History => XWM_HISTORY,
            Notification::LeftHold => XWM_LCLICKHOLD,
            Notification::ResetDefaultPosition => XWM_MAXIMIZEBOX,
        }
    }
}

/// One of EverQuest's UI windows or the controls in one, for the duration of
/// the current callback. Equal windows are the same window.
#[derive(Clone, Copy)]
pub struct Window<'a> {
    raw: &'a CXWnd,
    api: &'static Api,
}

impl<'a> Window<'a> {
    /// # Safety
    /// `raw` must be null or a live window that outlives `'a`.
    pub(crate) unsafe fn from_raw(raw: *mut CXWnd, api: &'static Api) -> Option<Window<'a>> {
        Some(Window { raw: unsafe { raw.as_ref() }?, api })
    }

    fn ptr(&self) -> *mut CXWnd {
        std::ptr::from_ref(self.raw).cast_mut()
    }

    fn related(&self, raw: *mut CXWnd) -> Option<Window<'a>> {
        // SAFETY: the game's windows point to live windows (or null).
        unsafe { Window::from_raw(raw, self.api) }
    }

    /// The underlying window, for fields this API doesn't wrap yet. Cast it
    /// to the subclass [`kind`](Self::kind) says it is.
    pub fn raw(&self) -> &'a CXWnd {
        self.raw
    }

    // ----- what it is -----

    pub fn kind(&self) -> WindowKind {
        WindowKind::from_raw(unsafe { (self.api.mqemu_window_type)(self.ptr()) })
    }

    fn xml_text(&self, which: i32) -> Option<String> {
        long_text(|out, cap| unsafe { (self.api.mqemu_window_xml_text)(self.ptr(), which, out, cap) })
    }

    /// Its name in the UI's XML (`${Window.Name}`): what [`Mq::window`]
    /// finds a top-level window by, such as "InventoryWindow".
    pub fn name(&self) -> Option<String> {
        self.xml_text(XML_NAME)
    }

    /// Its screen ID (`${Window.ScreenID}`): what a control is usually found
    /// by with [`child`](Self::child), such as "IW_DoneButton".
    pub fn screen_id(&self) -> Option<String> {
        self.xml_text(XML_SCREEN_ID)
    }

    /// The name of its XML type (`${Window.Type}`), such as "Button".
    pub fn type_name(&self) -> Option<String> {
        self.xml_text(XML_TYPE_NAME)
    }

    // ----- state -----

    /// Whether it's shown (`${Window.Open}`). A control can be shown inside
    /// a window that isn't.
    pub fn is_open(&self) -> bool {
        self.raw.dShow()
    }

    pub fn is_enabled(&self) -> bool {
        self.raw.Enabled()
    }

    pub fn is_minimized(&self) -> bool {
        self.raw.Minimized()
    }

    pub fn is_mouse_over(&self) -> bool {
        self.raw.MouseOver()
    }

    /// Where it is in its parent (on screen, for a top-level window):
    /// `${Window.X}`, `.Y`, `.Width` and `.Height`.
    pub fn location(&self) -> WindowRect {
        WindowRect::from_cxrect(self.raw.Location())
    }

    /// Where it is on screen.
    pub fn screen_rect(&self) -> WindowRect {
        let mut rect = mqemu_rect::default();
        unsafe { (self.api.mqemu_window_screen_rect)(self.ptr(), &mut rect) };
        WindowRect { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom }
    }

    /// Its text, as `${Window.Text}` reads it: a page's tab text, rich
    /// text's markup, and otherwise its window text (a label's or button's
    /// text, what's typed in an edit box, a window's title).
    pub fn text(&self) -> String {
        match self.kind() {
            WindowKind::StmlBox => from_cxstr(self.cast::<CStmlWnd>().STMLText()),
            WindowKind::Page => from_cxstr(self.cast::<CPageWnd>().TabText()),
            _ => from_cxstr(self.raw.WindowText()),
        }
    }

    /// Its tooltip (`${Window.Tooltip}`).
    pub fn tooltip(&self) -> String {
        long_text(|out, cap| unsafe { (self.api.mqemu_window_tooltip)(self.ptr(), out, cap) }).unwrap_or_default()
    }

    /// Its background color as ARGB (`${Window.BGColor}`).
    pub fn bg_color(&self) -> u32 {
        self.raw.BGColor()
    }

    pub fn alpha(&self) -> u8 {
        self.raw.Alpha()
    }

    /// The alpha it fades to when the mouse leaves it.
    pub fn fade_to_alpha(&self) -> u8 {
        self.raw.FadeToAlpha()
    }

    /// Its window style bits (`CWS_*`, `${Window.Style}`).
    pub fn style(&self) -> u32 {
        self.raw.WindowStyle()
    }

    /// `${Window.VScrollPos}` and `.VScrollMax`.
    pub fn vertical_scroll(&self) -> Scroll {
        Scroll { position: self.raw.VScrollPos(), max: self.raw.VScrollMax() }
    }

    /// `${Window.HScrollPos}` and `.HScrollMax`.
    pub fn horizontal_scroll(&self) -> Scroll {
        Scroll { position: self.raw.HScrollPos(), max: self.raw.HScrollMax() }
    }

    /// Whether a button, checkbox, item slot or spell gem is checked
    /// (`${Window.Checked}`); `None` for other windows.
    pub fn is_checked(&self) -> Option<bool> {
        matches!(self.kind(), WindowKind::Button | WindowKind::InvSlot | WindowKind::SpellGem)
            .then(|| self.cast::<CButtonWnd>().Checked())
    }

    /// A gauge's value, 0 to 1000 as the game draws it (`${Window.Value}`);
    /// `None` for other windows.
    pub fn gauge_value(&self) -> Option<f32> {
        (self.kind() == WindowKind::Gauge).then(|| self.cast::<CGaugeWnd>().LastFrameVal())
    }

    /// The window as the subclass its kind says it is.
    fn cast<T>(&self) -> &'a T {
        // SAFETY: only called for kinds whose windows are (or derive from) `T`.
        unsafe { &*std::ptr::from_ref(self.raw).cast::<T>() }
    }

    // ----- the window tree -----

    /// The window it's in (`${Window.Parent}`); `None` for a top-level window.
    pub fn parent(&self) -> Option<Window<'a>> {
        self.related(self.raw.ParentWindow())
    }

    /// Its first control (`${Window.FirstChild}`).
    pub fn first_child(&self) -> Option<Window<'a>> {
        self.related(self.raw.m_pFirstNode())
    }

    /// The next control in its parent (`${Window.Next}`).
    pub fn next_sibling(&self) -> Option<Window<'a>> {
        self.related(self.raw.m_pNext())
    }

    /// Its controls, in order.
    pub fn children(&self) -> impl Iterator<Item = Window<'a>> + use<'a> {
        std::iter::successors(self.first_child(), Window::next_sibling).take(MAX_DESCENDANTS)
    }

    /// Every window inside it, depth first: each control, then the controls
    /// in it, then its next sibling. At most [`MAX_DESCENDANTS`].
    pub fn descendants(&self) -> impl Iterator<Item = Window<'a>> + use<'a> {
        // The windows still to visit, the next one on top: a window's
        // controls come before its next sibling.
        let mut stack: Vec<Window<'a>> = self.first_child().into_iter().collect();
        std::iter::from_fn(move || {
            let current = stack.pop()?;
            stack.extend(current.next_sibling());
            stack.extend(current.first_child());
            Some(current)
        })
        .take(MAX_DESCENDANTS)
    }

    /// The first window inside it, depth first, whose name or screen ID is
    /// `name`, ignoring case (as `${Window[...].Child[name]}`).
    ///
    /// Unlike MacroQuest's `GetChildItem`, this only looks inside the window:
    /// MacroQuest's also checks the window itself and then carries on into
    /// the windows after it in its parent.
    pub fn child(&self, name: &str) -> Option<Window<'a>> {
        self.descendants().find(|window| window.is_named(name))
    }

    fn is_named(&self, name: &str) -> bool {
        self.name().is_some_and(|n| n.eq_ignore_ascii_case(name)) || self.screen_id().is_some_and(|id| id.eq_ignore_ascii_case(name))
    }

    /// A list, a tree view, or a combo box's drop-down list.
    pub fn as_list(&self) -> Option<ListWindow<'a>> {
        let list: *mut CListWnd = match self.kind() {
            WindowKind::Listbox | WindowKind::TreeView => self.ptr().cast(),
            WindowKind::Combobox => self.cast::<CComboWnd>().pListWnd(),
            _ => return None,
        };
        Some(ListWindow { window: *self, list: unsafe { list.as_ref() }? })
    }

    /// A tab box.
    pub fn as_tabs(&self) -> Option<TabWindow<'a>> {
        (self.kind() == WindowKind::TabBox).then(|| TabWindow { window: *self, tabs: self.cast::<CTabWnd>() })
    }

    // ----- acting on it -----

    /// Clicks its center, as `/notify <window> <control> <click>` does; to
    /// press a button, [`Click::Left`]. False if the game refused.
    pub fn click(&self, click: Click) -> bool {
        unsafe { (self.api.mqemu_window_click)(self.ptr(), click.index()) }
    }

    /// Sends it a notification as if `sender` (one of its controls, or
    /// none) sent it, with `data`: `/notify <window> <sender or 0>
    /// <notification> <data>`. The window must be the one that handles its
    /// controls' notifications, usually the top-level window.
    pub fn notify(&self, sender: Option<Window<'_>>, notification: Notification, data: i32) -> bool {
        let sender = sender.map_or(std::ptr::null_mut(), |sender| sender.ptr());
        unsafe { (self.api.mqemu_window_notify)(self.ptr(), sender, notification.raw(), data) }
    }

    /// Sends it a `link` notification as if `sender` sent it, as `/notify
    /// <window> <sender> link <code>` does: a link was clicked, its data the
    /// link's code. That's the link without its `\x12` markers, its tag
    /// (the digit after the first marker: `0` for an item) and its text, as
    /// eqlib's `ExecuteTextLink` sends it. The game's chat windows open the
    /// item it names (or answer the dialog, for a dialog response link).
    pub fn notify_link(&self, sender: Option<Window<'_>>, code: &str) -> bool {
        let sender = sender.map_or(std::ptr::null_mut(), |sender| sender.ptr());
        let code = to_c(code);
        unsafe { (self.api.mqemu_window_notify_link)(self.ptr(), sender, code.as_ptr()) }
    }

    /// Shows it and brings it to the front (`${Window.DoOpen}`).
    pub fn open(&self) -> bool {
        unsafe { (self.api.mqemu_window_show)(self.ptr(), true) }
    }

    /// Hides it (`${Window.DoClose}`).
    pub fn close(&self) -> bool {
        unsafe { (self.api.mqemu_window_show)(self.ptr(), false) }
    }

    /// Moves and sizes it, in its parent's coordinates as
    /// [`location`](Self::location) reads them (`${Window.Move}`).
    pub fn set_location(&self, rect: WindowRect) -> bool {
        let rect = mqemu_rect { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom };
        unsafe { (self.api.mqemu_window_move)(self.ptr(), &rect) }
    }

    /// `${Window.SetBGColor}`, as ARGB.
    pub fn set_bg_color(&self, argb: u32) {
        self.raw.set_BGColor(argb);
        unsafe { (self.api.mqemu_window_refade)(self.ptr()) };
    }

    /// `${Window.SetAlpha}`.
    pub fn set_alpha(&self, alpha: u8) {
        self.raw.set_Alpha(alpha);
        unsafe { (self.api.mqemu_window_refade)(self.ptr()) };
    }

    /// `${Window.SetFadeAlpha}`: the alpha it fades to when the mouse leaves.
    pub fn set_fade_to_alpha(&self, alpha: u8) {
        self.raw.set_FadeToAlpha(alpha);
        unsafe { (self.api.mqemu_window_refade)(self.ptr()) };
    }

    /// Sets an edit box's text and tells the window it's in, as if it had
    /// been typed (`${Window.SetText}`). False for other windows.
    pub fn set_text(&self, text: &str) -> bool {
        let text = to_c(text);
        unsafe { (self.api.mqemu_window_set_text)(self.ptr(), text.as_ptr()) }
    }
}

impl PartialEq for Window<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.raw, other.raw)
    }
}

impl Eq for Window<'_> {}

impl fmt::Debug for Window<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Window")
            .field("kind", &self.kind())
            .field("name", &self.name())
            .field("screen_id", &self.screen_id())
            .field("open", &self.is_open())
            .finish()
    }
}

/// A list, a tree view or a combo box's drop-down list: rows of cells.
#[derive(Clone, Copy)]
pub struct ListWindow<'a> {
    /// The list, or the combo box it's the drop-down of.
    window: Window<'a>,
    list: &'a CListWnd,
}

impl<'a> ListWindow<'a> {
    /// The list or combo box this is.
    pub fn window(&self) -> Window<'a> {
        self.window
    }

    pub fn raw(&self) -> &'a CListWnd {
        self.list
    }

    fn lines(&self) -> &'a [SListWndLine] {
        let items = self.list.ItemsArray();
        unsafe { game_array(items.m_array(), items.m_length()) }
    }

    /// Its rows (`${Window.Items}`).
    pub fn len(&self) -> usize {
        self.lines().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn columns(&self) -> usize {
        usize::try_from(self.list.Columns().m_length()).unwrap_or(0)
    }

    /// The selected row (`${Window.GetCurSel}` is it plus one).
    pub fn selected(&self) -> Option<usize> {
        usize::try_from(self.list.CurSel()).ok().filter(|row| *row < self.len())
    }

    /// The text of a cell (`${Window.List[row+1,column+1]}`); `None` if the
    /// cell isn't there. A row can have fewer cells than the list has
    /// columns, and the game reads the ones it lacks as empty; MacroQuest
    /// (and this) as none.
    pub fn text(&self, row: usize, column: usize) -> Option<String> {
        let cells = self.lines().get(row)?.Cells().m_length();
        if usize::try_from(cells).map_or(true, |cells| column >= cells) {
            return None;
        }
        let (row, column) = (i32::try_from(row).ok()?, i32::try_from(column).ok()?);
        let list = std::ptr::from_ref(self.list).cast_mut();
        long_text(|out, cap| unsafe { (self.window.api.mqemu_list_item_text)(list, row, column, out, cap) })
    }

    /// The window a cell shows instead of text, if it does: its first
    /// control, as `${Window.List[...]}` gives it.
    pub fn cell_window(&self, row: usize, column: usize) -> Option<Window<'a>> {
        let cells = self.lines().get(row)?.Cells();
        let cell = unsafe { game_array(cells.m_array(), cells.m_length()) }.get(column)?;
        self.window.related(cell.pWnd())?.first_child()
    }

    /// The first row whose cell in `column` matches: its text, or for a
    /// cell holding a window, that window's text (as `${Window.List[text]}`
    /// looks).
    pub fn find(&self, column: usize, mut matches: impl FnMut(&str) -> bool) -> Option<usize> {
        (0..self.len()).find(|row| match self.cell_window(*row, column) {
            Some(window) => matches(&window.text()),
            None => self.text(*row, column).is_some_and(|text| matches(&text)),
        })
    }

    /// Selects a row and tells the window it's in, as `/notify <window>
    /// <list> listselect <row+1>` does. False if the row isn't there.
    pub fn select(&self, row: usize) -> bool {
        let Ok(row) = i32::try_from(row) else { return false };
        unsafe { (self.window.api.mqemu_list_select)(self.window.ptr(), row) }
    }
}

impl fmt::Debug for ListWindow<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ListWindow").field("window", &self.window).field("rows", &self.len()).field("selected", &self.selected()).finish()
    }
}

/// A tab box: pages, one shown at a time.
#[derive(Clone, Copy)]
pub struct TabWindow<'a> {
    window: Window<'a>,
    tabs: &'a CTabWnd,
}

impl<'a> TabWindow<'a> {
    pub fn window(&self) -> Window<'a> {
        self.window
    }

    pub fn raw(&self) -> &'a CTabWnd {
        self.tabs
    }

    fn page_ptrs(&self) -> &'a [*mut CPageWnd] {
        let pages = self.tabs.PageArray();
        unsafe { game_array(pages.m_array(), pages.m_length()) }
    }

    /// Its pages (`${Window.TabCount}`).
    pub fn len(&self) -> usize {
        self.page_ptrs().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The page shown (`${Window.CurrentTabIndex}` is it plus one).
    pub fn current(&self) -> Option<usize> {
        usize::try_from(self.tabs.CurTabIndex()).ok().filter(|page| *page < self.len())
    }

    /// A page; its [`text`](Window::text) is its tab's (`${Window.Tab[n+1]}`).
    pub fn page(&self, page: usize) -> Option<Window<'a>> {
        self.window.related(self.page_ptrs().get(page)?.cast())
    }

    pub fn pages(&self) -> impl Iterator<Item = Window<'a>> + use<'a> {
        let tabs = *self;
        (0..self.len()).filter_map(move |page| tabs.page(page))
    }

    /// The page whose tab says `text`, ignoring case (`${Window.Tab[text]}`).
    pub fn find(&self, text: &str) -> Option<usize> {
        (0..self.len()).find(|page| self.page(*page).is_some_and(|p| p.text().eq_ignore_ascii_case(text)))
    }

    /// Shows a page and tells the window it's in, as `/notify <window> <tabs>
    /// tabselect <page+1>` does. False if the page isn't there.
    pub fn select(&self, page: usize) -> bool {
        let Ok(page) = i32::try_from(page) else { return false };
        unsafe { (self.window.api.mqemu_tab_select)(std::ptr::from_ref(self.tabs).cast_mut(), page) }
    }
}

impl fmt::Debug for TabWindow<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TabWindow").field("window", &self.window).field("pages", &self.len()).field("current", &self.current()).finish()
    }
}

/// The open context menu (the one a right click opened), for the duration
/// of the current callback.
///
/// Its items are the rows of the list it is. MacroQuest's `${Menu.NumItems}`,
/// `${Menu.Items[n]}`, `${Menu.Select}` and `/notify <text> menuselect` go by
/// `CContextMenu::NumItems` instead, which on this client isn't the menu's
/// count (in game it read 0 in one session and 678130844 in another), so
/// on emu they find nothing or go past the menu's rows.
#[derive(Clone, Copy)]
pub struct ContextMenu<'a> {
    menu: &'a CContextMenu,
    api: &'static Api,
}

/// The column of a context menu's rows that has their text.
const MENU_TEXT_COLUMN: usize = 1;

impl<'a> ContextMenu<'a> {
    pub fn raw(&self) -> &'a CContextMenu {
        self.menu
    }

    /// The menu as the list it is.
    pub fn list(&self) -> ListWindow<'a> {
        let window = Window { raw: unsafe { &*std::ptr::from_ref(self.menu).cast::<CXWnd>() }, api: self.api };
        ListWindow { window, list: unsafe { &*std::ptr::from_ref(self.menu).cast::<CListWnd>() } }
    }

    pub fn len(&self) -> usize {
        self.list().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// An item's text; separators read as empty.
    pub fn item(&self, item: usize) -> Option<String> {
        self.list().text(item, MENU_TEXT_COLUMN)
    }

    pub fn items(&self) -> impl Iterator<Item = String> + use<'a> {
        let menu = *self;
        (0..self.len()).map(move |item| menu.item(item).unwrap_or_default())
    }

    /// The first item whose text contains `text`, ignoring case, as `/notify
    /// <text> menuselect` looks for it.
    pub fn find(&self, text: &str) -> Option<usize> {
        let text = text.to_lowercase();
        self.items().position(|item| !item.is_empty() && item.to_lowercase().contains(&text))
    }

    /// Picks an item, as `/notify <text> menuselect` does. False if the item
    /// isn't there.
    pub fn select(&self, item: usize) -> bool {
        let Ok(item) = i32::try_from(item) else { return false };
        unsafe { (self.api.mqemu_menu_select)(std::ptr::from_ref(self.menu).cast_mut(), item) }
    }

    /// Closes it, and any other open context menu, without picking anything.
    pub fn close(self) -> bool {
        unsafe { (self.api.mqemu_menus_close)() }
    }
}

impl fmt::Debug for ContextMenu<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.items()).finish()
    }
}

/// The windows in an `ArrayClass2` (eqlib's array of bins: entry `i` is in
/// bin `i >> m_binShift` at `i & m_slotMask`). Nothing past what its
/// counts allow is read, so a damaged one gives what can be read safely.
fn binned_windows(array: &ArrayClass2_CXWnd) -> Vec<*mut CXWnd> {
    // eqlib's bins hold at most a few thousand entries.
    let Some(shift) = u32::try_from(array.m_binShift()).ok().filter(|shift| *shift < 16) else { return Vec::new() };
    let bins = unsafe { game_array(array.m_array().cast_const(), array.m_binCount()) };
    let length = usize::try_from(array.m_length()).unwrap_or(0).min(MAX_DESCENDANTS);
    let mask = usize::try_from(array.m_slotMask()).unwrap_or(0);
    let per_bin = 1usize << shift;
    (0..length)
        .map_while(|i| {
            let bin = *bins.get(i >> shift)?;
            let slot = i & mask;
            // A slot past its bin's size would be outside it.
            (!bin.is_null() && slot < per_bin).then(|| unsafe { *bin.add(slot) })
        })
        .collect()
}

/// The open context menu: the innermost of those open, as the context menu
/// manager tracks them.
fn open_context_menu(manager: &CContextMenuManager) -> *mut CContextMenu {
    let current = usize::try_from(manager.CurrMenu()).ok().filter(|menu| *menu < MAX_CONTEXT_MENU_DEPTH);
    match current {
        Some(current) if manager.NumVisibleMenus() > 0 => manager.pCurrMenus_at(current),
        _ => std::ptr::null_mut(),
    }
}

impl<P: Plugin> Mq<P> {
    fn window_api(&self) -> &'static Api {
        self.host().windows
    }

    fn window_manager(&self) -> Option<&CXWndManager> {
        unsafe { (self.host().window_manager)().as_ref() }
    }

    /// The window `name` names, as `${Window[name]}` finds it: a top-level
    /// window by its XML name, ignoring case ("InventoryWindow"; a shown one
    /// if several share the name); a bag's window as "pack1" to "pack10",
    /// "bank1"... or "enviro"; or a path to a control inside one,
    /// "Window/Control/...", each step found with [`Window::child`].
    pub fn window(&self, name: &str) -> Option<Window<'_>> {
        let mut steps = name.split('/').filter(|step| !step.is_empty());
        let top = to_c(steps.next()?);
        let api = self.window_api();
        let mut window = unsafe { Window::from_raw((api.mqemu_find_window)(top.as_ptr()), api) }?;
        for step in steps {
            window = window.child(step)?;
        }
        Some(window)
    }

    /// Every top-level window the game has made (shown or not): the ones the
    /// window manager lists, then the item and spell display windows, which
    /// the game makes as they're needed and keeps apart. These are the
    /// windows `${Window[name]}` finds by name.
    pub fn windows(&self) -> impl Iterator<Item = Window<'_>> + '_ {
        let api = self.window_api();
        let listed = self.window_manager().map_or(&[][..], |manager| {
            let list = manager.pWindows();
            unsafe { game_array(list.m_array(), list.m_length()) }
        });
        let host = self.host();
        let items = unsafe { (host.item_display_manager)().as_ref() }.map(|m| binned_windows(m.m_windows()));
        let spells = unsafe { (host.spell_display_manager)().as_ref() }.map(|m| binned_windows(m.m_windows()));
        let displays: Vec<*mut CXWnd> =
            items.into_iter().chain(spells).flatten().filter(|raw| !listed.contains(raw)).collect();
        listed
            .iter()
            .copied()
            .chain(displays)
            .filter_map(move |raw| unsafe { Window::from_raw(raw, api) })
            .filter(|window| window.kind() == WindowKind::Screen)
    }

    /// The window with the keyboard focus (`${Window.Highlighted}` is
    /// whether a window is it).
    pub fn focused_window(&self) -> Option<Window<'_>> {
        unsafe { Window::from_raw(self.window_manager()?.FocusWindow(), self.window_api()) }
    }

    /// The open context menu, if any.
    pub fn context_menu(&self) -> Option<ContextMenu<'_>> {
        let manager = unsafe { (self.host().context_menu_manager)().as_ref() }?;
        let menu = unsafe { open_context_menu(manager).as_ref() }?;
        Some(ContextMenu { menu, api: self.window_api() })
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! Stand-ins for the shim's window functions, answering from a
    //! thread-local a test fills in, and recording what was done.

    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::{CStr, c_char, c_void};

    use mq_emu_sys::constants::UI_UNKNOWN;
    use mq_emu_sys::layouts::{CContextMenu, CHotButton, CInvSlotWnd, CListWnd, CTabWnd, CTextureAnimation, CXWnd, ItemClient};
    use mq_emu_sys::windows::{Api, mqemu_rect, mqemu_window_notification_fn};

    /// A window as the fake game knows it.
    #[derive(Debug, Clone, Default)]
    pub(crate) struct FakeWindow {
        pub kind: i32,
        /// Name, screen ID, type name.
        pub xml: [Option<String>; 3],
        pub tooltip: String,
        pub screen_rect: mqemu_rect,
    }

    #[derive(Default)]
    pub(crate) struct FakeWindows {
        pub windows: HashMap<usize, FakeWindow>,
        /// What `FindMQ2Window` finds, by lowercase name.
        pub by_name: HashMap<String, usize>,
        /// List cell texts by (list, row, column).
        pub cells: HashMap<(usize, i32, i32), String>,
        /// Everything done to windows, in order.
        pub log: Vec<String>,
        /// The item each item slot or hot button holds.
        pub items: HashMap<usize, usize>,
        /// Item slots' texts, and texture names, by pointer.
        pub texts: HashMap<usize, String>,
        /// The screen pieces the UI has, by lowercase name.
        pub screen_pieces: Vec<String>,
        /// What `mqemu_plugin_window_create` makes, by screen piece: a
        /// window the test owns. Each is made at most once at a time.
        pub plugin_windows: HashMap<String, usize>,
        /// The windows made, with what they notify.
        pub made: Vec<(usize, mqemu_window_notification_fn, usize)>,
    }

    thread_local! {
        static WINDOWS: RefCell<FakeWindows> = RefCell::new(FakeWindows::default());
    }

    pub(crate) fn with<R>(f: impl FnOnce(&mut FakeWindows) -> R) -> R {
        WINDOWS.with(|windows| f(&mut windows.borrow_mut()))
    }

    /// Forgets every window, so a test starts from nothing.
    pub(crate) fn reset() {
        with(|windows| *windows = FakeWindows::default());
    }

    /// Records a window the fake game has.
    pub(crate) fn add(window: *mut CXWnd, kind: i32, name: Option<&str>, screen_id: Option<&str>) {
        let xml = [name.map(str::to_owned), screen_id.map(str::to_owned), None];
        with(|windows| windows.windows.insert(window as usize, FakeWindow { kind, xml, ..FakeWindow::default() }));
    }

    pub(crate) fn log() -> Vec<String> {
        with(|windows| std::mem::take(&mut windows.log))
    }

    fn write(text: Option<&str>, out: *mut c_char, cap: usize) -> i32 {
        let Some(text) = text else { return -1 };
        let len = text.len().min(cap - 1);
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr().cast(), out, len);
            *out.add(len) = 0;
        }
        text.len() as i32
    }

    fn window<R>(window: *mut CXWnd, f: impl FnOnce(Option<&FakeWindow>) -> R) -> R {
        with(|windows| f(windows.windows.get(&(window as usize))))
    }

    fn record(entry: String) -> bool {
        with(|windows| windows.log.push(entry));
        true
    }

    /// How a log entry names a window: its screen ID, else its name.
    pub(crate) fn label(raw: *mut CXWnd) -> String {
        window(raw, |w| w.and_then(|w| w.xml[1].clone().or_else(|| w.xml[0].clone()))).unwrap_or_else(|| "?".into())
    }

    unsafe extern "C" fn find_window(name: *const c_char) -> *mut CXWnd {
        let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().to_lowercase();
        assert!(!name.contains('/'), "paths are walked in Rust");
        with(|windows| windows.by_name.get(&name).copied().unwrap_or(0)) as *mut CXWnd
    }

    unsafe extern "C" fn window_type(raw: *mut CXWnd) -> i32 {
        window(raw, |w| w.map_or(UI_UNKNOWN, |w| w.kind))
    }

    unsafe extern "C" fn xml_text(raw: *mut CXWnd, which: i32, out: *mut c_char, cap: usize) -> i32 {
        window(raw, |w| write(w.and_then(|w| w.xml.get(which as usize)?.as_deref()), out, cap))
    }

    unsafe extern "C" fn tooltip(raw: *mut CXWnd, out: *mut c_char, cap: usize) -> i32 {
        window(raw, |w| write(w.map(|w| w.tooltip.as_str()), out, cap))
    }

    unsafe extern "C" fn screen_rect(raw: *mut CXWnd, out: *mut mqemu_rect) -> bool {
        window(raw, |w| w.map(|w| unsafe { *out = w.screen_rect }).is_some())
    }

    unsafe extern "C" fn click(raw: *mut CXWnd, click: i32) -> bool {
        record(format!("click {} {click}", label(raw)))
    }

    unsafe extern "C" fn notify(raw: *mut CXWnd, sender: *mut CXWnd, message: u32, data: i32) -> bool {
        let sender = if sender.is_null() { "0".into() } else { label(sender) };
        record(format!("notify {} {sender} {message} {data}", label(raw)))
    }

    unsafe extern "C" fn show(raw: *mut CXWnd, show: bool) -> bool {
        unsafe { (*raw).set_dShow(show) };
        record(format!("show {} {show}", label(raw)))
    }

    unsafe extern "C" fn move_window(raw: *mut CXWnd, rect: *const mqemu_rect) -> bool {
        let rect = unsafe { *rect };
        let location = unsafe { (*raw).Location() };
        location.set_left(rect.left);
        location.set_top(rect.top);
        location.set_right(rect.right);
        location.set_bottom(rect.bottom);
        record(format!("move {} {} {} {} {}", label(raw), rect.left, rect.top, rect.right, rect.bottom))
    }

    unsafe extern "C" fn refade(raw: *mut CXWnd) -> bool {
        record(format!("refade {}", label(raw)))
    }

    unsafe extern "C" fn set_text(raw: *mut CXWnd, text: *const c_char) -> bool {
        let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
        record(format!("set_text {} {text}", label(raw)))
    }

    unsafe extern "C" fn item_text(list: *mut CListWnd, row: i32, column: i32, out: *mut c_char, cap: usize) -> i32 {
        with(|windows| write(windows.cells.get(&(list as usize, row, column)).map(String::as_str), out, cap))
    }

    unsafe extern "C" fn list_select(raw: *mut CXWnd, row: i32) -> bool {
        record(format!("list_select {} {row}", label(raw)))
    }

    unsafe extern "C" fn tab_select(tabs: *mut CTabWnd, page: i32) -> bool {
        unsafe { (*tabs).set_CurTabIndex(page) };
        record(format!("tab_select {} {page}", label(tabs.cast())))
    }

    unsafe extern "C" fn menu_select(menu: *mut CContextMenu, item: i32) -> bool {
        record(format!("menu_select {} {item}", label(menu.cast())))
    }

    unsafe extern "C" fn menus_close() -> bool {
        record("menus_close".into())
    }

    unsafe extern "C" fn notify_link(raw: *mut CXWnd, sender: *mut CXWnd, text: *const c_char) -> bool {
        let sender = if sender.is_null() { "0".into() } else { label(sender) };
        let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
        record(format!("notify_link {} {sender} {text}", label(raw)))
    }

    unsafe extern "C" fn held_item(raw: *mut CInvSlotWnd) -> *mut ItemClient {
        with(|windows| windows.items.get(&(raw as usize)).copied().unwrap_or(0)) as *mut ItemClient
    }

    unsafe extern "C" fn hot_button_item(raw: *mut CHotButton) -> *mut ItemClient {
        with(|windows| windows.items.get(&(raw as usize)).copied().unwrap_or(0)) as *mut ItemClient
    }

    unsafe extern "C" fn inv_slot_text(raw: *mut CInvSlotWnd, out: *mut c_char, cap: usize) -> i32 {
        with(|windows| write(windows.texts.get(&(raw as usize)).map(String::as_str), out, cap))
    }

    unsafe extern "C" fn texture_name(raw: *mut CTextureAnimation, out: *mut c_char, cap: usize) -> i32 {
        with(|windows| write(windows.texts.get(&(raw as usize)).map(String::as_str), out, cap))
    }

    unsafe extern "C" fn activate(raw: *mut CHotButton) -> bool {
        record(format!("activate {}", label(raw.cast())))
    }

    unsafe extern "C" fn add_xml_file(name: *const c_char) -> bool {
        let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
        record(format!("add_xml_file {name}"))
    }

    unsafe extern "C" fn remove_xml_file(name: *const c_char) -> bool {
        let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
        record(format!("remove_xml_file {name}"))
    }

    unsafe extern "C" fn screen_piece_loaded(name: *const c_char) -> bool {
        let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().to_lowercase();
        with(|windows| windows.screen_pieces.contains(&name))
    }

    unsafe extern "C" fn create(screen: *const c_char, notify: mqemu_window_notification_fn, user: *mut c_void) -> *mut CXWnd {
        let screen = unsafe { CStr::from_ptr(screen) }.to_string_lossy().to_lowercase();
        with(|windows| {
            if !windows.screen_pieces.contains(&screen) {
                return std::ptr::null_mut();
            }
            let Some(window) = windows.plugin_windows.get(&screen).copied() else { return std::ptr::null_mut() };
            assert!(!windows.made.iter().any(|(w, ..)| *w == window), "{screen} made twice");
            windows.made.push((window, notify, user as usize));
            windows.log.push(format!("create {screen}"));
            window as *mut CXWnd
        })
    }

    unsafe extern "C" fn destroy(raw: *mut CXWnd) -> bool {
        let entry = label(raw);
        with(|windows| {
            let made = windows.made.iter().position(|(w, ..)| *w == raw as usize).expect("destroying a window that wasn't made");
            windows.made.remove(made);
        });
        record(format!("destroy {entry}"))
    }

    /// Has the fake game's window `window` (made by `create`) send its
    /// plugin a notification, as the real window does.
    pub(crate) fn send(window: *mut CXWnd, sender: *mut CXWnd, message: u32, data: *mut c_void) {
        let (notify, user) = with(|windows| {
            let (_, notify, user) = windows.made.iter().find(|(w, ..)| *w == window as usize).copied().expect("a window that was made");
            (notify, user)
        });
        unsafe { notify(user as *mut c_void, window, sender, message, data) };
    }

    pub(crate) static API: Api = Api {
        mqemu_find_window: find_window,
        mqemu_window_type: window_type,
        mqemu_window_xml_text: xml_text,
        mqemu_window_tooltip: tooltip,
        mqemu_window_screen_rect: screen_rect,
        mqemu_window_click: click,
        mqemu_window_notify: notify,
        mqemu_window_show: show,
        mqemu_window_move: move_window,
        mqemu_window_refade: refade,
        mqemu_window_set_text: set_text,
        mqemu_list_item_text: item_text,
        mqemu_list_select: list_select,
        mqemu_tab_select: tab_select,
        mqemu_menu_select: menu_select,
        mqemu_menus_close: menus_close,
        mqemu_window_notify_link: notify_link,
        mqemu_inv_slot_item: held_item,
        mqemu_inv_slot_text: inv_slot_text,
        mqemu_texture_animation_name: texture_name,
        mqemu_hot_button_item: hot_button_item,
        mqemu_hot_button_activate: activate,
        mqemu_add_xml_file: add_xml_file,
        mqemu_remove_xml_file: remove_xml_file,
        mqemu_screen_piece_loaded: screen_piece_loaded,
        mqemu_plugin_window_create: create,
        mqemu_plugin_window_destroy: destroy,
    };
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::layouts::{CXStr, SListWndCell};

    use super::*;
    use crate::items::tests::Fake;
    use crate::text::tests::cstrrep;

    fn window<T>(fake: &Fake<T>) -> Window<'_> {
        unsafe { Window::from_raw(fake.ptr().cast(), &fake::API) }.unwrap()
    }

    /// Makes `children` the controls of `parent`, in order.
    fn adopt<T>(parent: &Fake<T>, children: &[*mut CXWnd]) {
        let parent = parent.ptr().cast::<CXWnd>();
        unsafe { (*parent).set_m_pFirstNode(children.first().copied().unwrap_or(std::ptr::null_mut())) };
        for (i, child) in children.iter().enumerate() {
            let child = unsafe { &**child };
            child.set_ParentWindow(parent);
            child.set_m_pNext(children.get(i + 1).copied().unwrap_or(std::ptr::null_mut()));
        }
    }

    /// Text for a CXStr, kept alive by the returned storage.
    fn set_text(text: &CXStr, value: &str) -> Vec<u64> {
        let storage = cstrrep(0, value.len() as u32, value.as_bytes());
        text.set_m_data(storage.as_ptr().cast_mut().cast());
        storage
    }

    struct Tree {
        top: Fake<CXWnd>,
        a: Fake<CXWnd>,
        a1: Fake<CXWnd>,
        a2: Fake<CXWnd>,
        b: Fake<CXWnd>,
        b1: Fake<CXWnd>,
    }

    /// top: a (a1, a2), b (b1). Each control's screen ID is "<letter>_ID";
    /// a2 is also named "B1".
    fn tree() -> Tree {
        fake::reset();
        let tree = Tree { top: Fake::new(), a: Fake::new(), a1: Fake::new(), a2: Fake::new(), b: Fake::new(), b1: Fake::new() };
        fake::add(tree.top.ptr(), UI_SCREEN, Some("Top"), None);
        for (w, name) in [(&tree.a, "a"), (&tree.a1, "a1"), (&tree.a2, "a2"), (&tree.b, "b"), (&tree.b1, "b1")] {
            fake::add(w.ptr(), UI_BUTTON, None, Some(&format!("{name}_ID")));
        }
        fake::with(|w| w.windows.get_mut(&(tree.a2.ptr() as usize)).unwrap().xml[0] = Some("B1".into()));
        adopt(&tree.top, &[tree.a.ptr(), tree.b.ptr()]);
        adopt(&tree.a, &[tree.a1.ptr(), tree.a2.ptr()]);
        adopt(&tree.b, &[tree.b1.ptr()]);
        tree
    }

    fn ids<'a>(windows: impl Iterator<Item = Window<'a>>) -> Vec<String> {
        windows.map(|w| w.screen_id().or(w.name()).unwrap_or_default()).collect()
    }

    #[test]
    fn the_tree_is_walked_depth_first() {
        let tree = tree();
        let top = window(&tree.top);
        assert_eq!(ids(top.children()), ["a_ID", "b_ID"]);
        assert_eq!(ids(top.descendants()), ["a_ID", "a1_ID", "a2_ID", "b_ID", "b1_ID"]);
        assert_eq!(ids(window(&tree.a).descendants()), ["a1_ID", "a2_ID"], "only inside a, not on into b");
        assert_eq!(ids(window(&tree.b1).descendants()), Vec::<String>::new());
        assert_eq!(window(&tree.a1).parent(), Some(window(&tree.a)));
        assert_eq!(window(&tree.top).parent(), None);
        assert_eq!((window(&tree.a).next_sibling(), window(&tree.b).next_sibling()), (Some(window(&tree.b)), None));
    }

    #[test]
    fn a_loop_in_the_tree_ends_the_walk() {
        let tree = tree();
        // b1's first control is the top window: the walk would never end.
        tree.b1.get().set_m_pFirstNode(tree.top.ptr());
        assert_eq!(window(&tree.top).descendants().count(), MAX_DESCENDANTS);
        assert_eq!(window(&tree.top).child("nothing"), None);
    }

    #[test]
    fn children_are_found_by_name_or_screen_id_ignoring_case() {
        let tree = tree();
        let top = window(&tree.top);
        assert_eq!(top.child("A1_id"), Some(window(&tree.a1)));
        assert_eq!(top.child("b1"), Some(window(&tree.a2)), "a2's name comes first, depth first");
        assert_eq!(top.child("b1_ID"), Some(window(&tree.b1)));
        assert_eq!(top.child("Top"), None, "not the window itself");
    }

    #[test]
    fn children_are_only_looked_for_inside_the_window() {
        // MacroQuest's GetChildItem carries on from a window into the
        // windows after it: Child[b1_ID] of a would find b's control.
        let tree = tree();
        assert_eq!(window(&tree.a).child("b1_ID"), None);
        assert_eq!(window(&tree.a).child("b_ID"), None, "nor a's next sibling");
        assert_eq!(window(&tree.b).child("b1_ID"), Some(window(&tree.b1)));
    }

    #[test]
    fn kinds_round_trip() {
        for raw in -1..60 {
            let kind = WindowKind::from_raw(raw);
            assert_eq!(kind.raw(), raw);
            assert_eq!(matches!(kind, WindowKind::Other(_)), kind.name() == "Unknown", "{raw}");
        }
        assert_eq!(WindowKind::from_raw(UI_EDIT_BOX).name(), "Editbox");
        assert_eq!(WindowKind::from_raw(UI_UNKNOWN).name(), "CXWnd");
        assert_eq!(WindowKind::from_raw(UI_STML_BOX), WindowKind::StmlBox);
        assert_eq!(WindowKind::from_raw(1), WindowKind::Other(1), "an XML class no window is");
    }

    #[test]
    fn state_comes_from_the_window() {
        fake::reset();
        let fake = Fake::<CXWnd>::new();
        let raw = fake.get();
        fake::add(fake.ptr(), UI_LABEL, Some("Label"), Some("L1"));
        fake::with(|w| {
            let entry = w.windows.get_mut(&(fake.ptr() as usize)).unwrap();
            entry.tooltip = "a tip ".repeat(100);
            entry.screen_rect = mqemu_rect { left: 110, top: 220, right: 130, bottom: 260 };
        });
        raw.set_dShow(true);
        raw.set_Enabled(true);
        raw.Location().set_left(10);
        raw.Location().set_top(20);
        raw.Location().set_right(30);
        raw.Location().set_bottom(60);
        raw.set_BGColor(0xff102030);
        raw.set_VScrollPos(5);
        raw.set_VScrollMax(9);
        let _text = set_text(raw.WindowText(), "Hello");

        let w = window(&fake);
        assert_eq!((w.kind(), w.name().as_deref(), w.screen_id().as_deref(), w.type_name()), (WindowKind::Label, Some("Label"), Some("L1"), None));
        assert!(w.is_open() && w.is_enabled() && !w.is_minimized() && !w.is_mouse_over());
        assert_eq!(w.location(), WindowRect::new(10, 20, 20, 40));
        assert_eq!((w.location().width(), w.location().height()), (20, 40));
        assert_eq!(w.screen_rect(), WindowRect { left: 110, top: 220, right: 130, bottom: 260 });
        assert_eq!(w.text(), "Hello");
        assert_eq!(w.tooltip().len(), 600, "longer than the first buffer");
        assert_eq!(w.bg_color(), 0xff102030);
        assert_eq!(w.vertical_scroll(), Scroll { position: 5, max: 9 });
        assert_eq!((w.is_checked(), w.gauge_value()), (None, None), "a label is neither");
        assert!(w.as_list().is_none() && w.as_tabs().is_none());
    }

    #[test]
    fn text_follows_the_kind() {
        fake::reset();
        let (stml, page, button) = (Fake::<CStmlWnd>::new(), Fake::<CPageWnd>::new(), Fake::<CButtonWnd>::new());
        fake::add(stml.ptr().cast(), UI_STML_BOX, None, None);
        fake::add(page.ptr().cast(), UI_PAGE, None, None);
        fake::add(button.ptr().cast(), UI_BUTTON, None, None);
        let _texts = [
            set_text(stml.get().STMLText(), "<b>Hail</b>"),
            set_text(stml.get().WindowText(), "not this"),
            set_text(page.get().TabText(), "Main"),
            set_text(page.get().WindowText(), "not this"),
            set_text(button.get().WindowText(), "Done"),
        ];
        assert_eq!([window(&stml).text(), window(&page).text(), window(&button).text()], ["<b>Hail</b>", "Main", "Done"]);
    }

    #[test]
    fn checked_and_gauge_values_only_for_their_kinds() {
        fake::reset();
        let (button, gauge) = (Fake::<CButtonWnd>::new(), Fake::<CGaugeWnd>::new());
        fake::add(button.ptr().cast(), UI_BUTTON, None, None);
        fake::add(gauge.ptr().cast(), UI_GAUGE, None, None);
        button.get().set_Checked(true);
        gauge.get().set_LastFrameVal(750.0);
        assert_eq!((window(&button).is_checked(), window(&button).gauge_value()), (Some(true), None));
        assert_eq!((window(&gauge).is_checked(), window(&gauge).gauge_value()), (None, Some(750.0)));
        for kind in [UI_INV_SLOT, UI_SPELL_GEM] {
            fake::add(button.ptr().cast(), kind, None, None);
            assert_eq!(window(&button).is_checked(), Some(true), "{kind} windows are buttons");
        }
    }

    /// A two-column list: rows "Bread"/"1", "Water"/"2", and a row whose
    /// first cell holds a window (with a label "Milk" in it).
    struct List {
        list: Fake<CListWnd>,
        /// The lines, the cells and the label's text.
        _storage: Vec<Vec<u64>>,
        holder: Fake<CXWnd>,
        label: Fake<CXWnd>,
    }

    /// Zeroed storage for `count` records of `T` in a row.
    fn records<T>(count: usize) -> Vec<u64> {
        vec![0; (size_of::<T>() * count).div_ceil(8)]
    }

    fn list() -> List {
        fake::reset();
        let list = Fake::<CListWnd>::new();
        fake::add(list.ptr().cast(), UI_LISTBOX, None, Some("Goods"));
        // The game's lines lie one after another; so must the fake's.
        let mut lines = records::<SListWndLine>(3);
        let mut cells: Vec<Vec<u64>> = (0..3).map(|_| records::<SListWndCell>(2)).collect();
        let first = lines.as_mut_ptr().cast::<SListWndLine>();
        let line = |row: usize| unsafe { &*first.add(row) };
        for (row, storage) in cells.iter_mut().enumerate() {
            line(row).Cells().set_m_array(storage.as_mut_ptr().cast());
            line(row).Cells().set_m_length(2);
        }
        list.get().ItemsArray().set_m_array(lines.as_mut_ptr().cast());
        list.get().ItemsArray().set_m_length(3);
        list.get().Columns().set_m_length(2);
        list.get().set_CurSel(-1);
        let (holder, label) = (Fake::<CXWnd>::new(), Fake::<CXWnd>::new());
        holder.get().set_m_pFirstNode(label.ptr());
        unsafe { &*cells[2].as_mut_ptr().cast::<SListWndCell>() }.set_pWnd(holder.ptr());
        fake::add(label.ptr(), UI_LABEL, None, Some("MilkLabel"));
        let text = set_text(label.get().WindowText(), "Milk");
        fake::with(|w| {
            for (row, texts) in [["Bread", "1"], ["Water", "2"], ["", "3"]].iter().enumerate() {
                for (column, text) in texts.iter().enumerate() {
                    w.cells.insert((list.ptr() as usize, row as i32, column as i32), (*text).into());
                }
            }
        });
        // Moved, not copied: the list points into these.
        let mut storage = vec![lines, text];
        storage.extend(cells);
        List { list, _storage: storage, holder, label }
    }

    #[test]
    fn lists_read_rows_cells_and_the_selection() {
        let fake = list();
        let list = window(&fake.list).as_list().unwrap();
        assert_eq!((list.len(), list.columns(), list.selected()), (3, 2, None));
        assert_eq!((list.text(1, 0).as_deref(), list.text(1, 1).as_deref(), list.text(3, 0)), (Some("Water"), Some("2"), None));
        assert_eq!(list.cell_window(2, 0), Some(window(&fake.label)), "the cell's window's first control");
        assert_eq!((list.cell_window(0, 0), list.cell_window(2, 5), list.cell_window(9, 0)), (None, None, None));
        assert_ne!(list.cell_window(2, 0), Some(window(&fake.holder)));
        fake.list.get().set_CurSel(1);
        assert_eq!(list.selected(), Some(1));
        fake.list.get().set_CurSel(3);
        assert_eq!(list.selected(), None, "past the end");
    }

    #[test]
    fn a_cell_a_short_row_lacks_isnt_there() {
        let fake = list();
        let list = window(&fake.list).as_list().unwrap();
        // Row 1 keeps one of the list's two cells; the game would read the
        // other as empty, as the fake's cell texts still do.
        list.lines()[1].Cells().set_m_length(1);
        assert_eq!((list.text(1, 0).as_deref(), list.text(1, 1)), (Some("Water"), None), "as ${{Window.List[2,2]}} is NULL");
        assert_eq!(list.find(1, |t| t == "2"), None);
        assert_eq!(list.text(0, 1).as_deref(), Some("1"), "other rows keep theirs");
    }

    #[test]
    fn lists_find_rows_by_text_or_by_their_window() {
        let fake = list();
        let list = window(&fake.list).as_list().unwrap();
        assert_eq!(list.find(0, |t| t.eq_ignore_ascii_case("water")), Some(1));
        assert_eq!(list.find(0, |t| t == "Milk"), Some(2), "through the cell's window");
        assert_eq!(list.find(1, |t| t == "3"), Some(2));
        assert_eq!(list.find(0, |t| t == "Juice"), None);
    }

    #[test]
    fn selecting_rows_goes_to_the_game() {
        let fake = list();
        let list = window(&fake.list).as_list().unwrap();
        assert!(list.select(2));
        assert_eq!(fake::log(), ["list_select Goods 2"]);
    }

    #[test]
    fn a_combo_box_is_its_drop_down_list() {
        let fake = list();
        let combo = Fake::<CComboWnd>::new();
        fake::add(combo.ptr().cast(), UI_COMBOBOX, None, Some("Choice"));
        assert!(window(&combo).as_list().is_none(), "no drop-down yet");
        combo.get().set_pListWnd(fake.list.ptr());
        let list = window(&combo).as_list().unwrap();
        assert_eq!((list.len(), list.window(), list.text(0, 0).as_deref()), (3, window(&combo), Some("Bread")));
        list.select(0);
        assert_eq!(fake::log(), ["list_select Choice 0"], "the combo box is selected in, as SendListSelect2 takes it");
    }

    #[test]
    fn tabs_read_and_select_pages() {
        fake::reset();
        let tabs = Fake::<CTabWnd>::new();
        let pages = [Fake::<CPageWnd>::new(), Fake::<CPageWnd>::new()];
        fake::add(tabs.ptr().cast(), UI_TAB_BOX, None, Some("Tabs"));
        let mut ptrs = [pages[0].ptr(), pages[1].ptr()];
        let _texts: Vec<_> = pages
            .iter()
            .zip(["Main", "Options"])
            .map(|(page, text)| {
                fake::add(page.ptr().cast(), UI_PAGE, None, None);
                set_text(page.get().TabText(), text)
            })
            .collect();
        tabs.get().PageArray().set_m_array(ptrs.as_mut_ptr());
        tabs.get().PageArray().set_m_length(2);
        tabs.get().set_CurTabIndex(0);

        let tabs = window(&tabs).as_tabs().unwrap();
        assert_eq!((tabs.len(), tabs.current(), tabs.find("OPTIONS"), tabs.find("Nope")), (2, Some(0), Some(1), None));
        assert_eq!(tabs.pages().map(|p| p.text()).collect::<Vec<_>>(), ["Main", "Options"]);
        assert_eq!(tabs.page(2), None);
        assert!(tabs.select(1));
        assert_eq!((fake::log(), tabs.current()), (vec!["tab_select Tabs 1".to_owned()], Some(1)));
        tabs.raw().set_CurTabIndex(-1);
        assert_eq!(tabs.current(), None);
    }

    #[test]
    fn clicks_and_notifications_have_notify_names() {
        let names = Click::ALL.map(Click::name);
        assert_eq!(names, ["leftmouse", "leftmouseup", "leftmouseheld", "leftmouseheldup", "rightmouse", "rightmouseup", "rightmouseheld", "rightmouseheldup"]);
        for click in Click::ALL {
            assert_eq!(Click::from_name(&click.name().to_uppercase()), Some(click));
        }
        assert_eq!(Click::from_name("leftmousedown"), None, "the name ${{Window.LeftMouseDown}} sends, which MacroQuest's click doesn't know");
        for n in Notification::ALL {
            assert_eq!(Notification::from_name(n.name()), Some(n));
        }
        assert_eq!(Notification::from_name("link"), None, "its data is text, which notify doesn't send");
        let raw: Vec<u32> = Notification::ALL.iter().map(|n| n.raw()).collect();
        assert_eq!(raw, [1, 2, 3, 6, 9, 10, 14, 20, 21, 22, 23, 29], "the messages MacroQuest's /notify names");
    }

    #[test]
    fn actions_go_to_the_game() {
        let tree = tree();
        let (top, b1) = (window(&tree.top), window(&tree.b1));
        assert!(b1.click(Click::Left) && b1.click(Click::RightHeldUp));
        assert!(top.notify(Some(b1), Notification::NewValue, 7) && top.notify(None, Notification::Close, 0));
        assert!(top.open() && top.is_open());
        assert!(top.close() && !top.is_open());
        assert!(top.set_location(WindowRect::new(5, 6, 100, 50)));
        assert_eq!(top.location(), WindowRect { left: 5, top: 6, right: 105, bottom: 56 });
        top.set_bg_color(0x80ff0000);
        top.set_alpha(200);
        top.set_fade_to_alpha(100);
        assert_eq!((top.bg_color(), top.alpha(), top.fade_to_alpha()), (0x80ff0000, 200, 100));
        assert!(b1.set_text("hello"));
        assert_eq!(
            fake::log(),
            [
                "click b1_ID 1",
                "click b1_ID 7",
                "notify Top b1_ID 14 7",
                "notify Top 0 10 0",
                "show Top true",
                "show Top false",
                "move Top 5 6 105 56",
                "refade Top",
                "refade Top",
                "refade Top",
                "set_text b1_ID hello",
            ]
        );
    }

    fn fake_write(text: &str, out: *mut c_char, cap: usize) -> i32 {
        let len = text.len().min(cap - 1);
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr().cast(), out, len);
            *out.add(len) = 0;
        }
        text.len() as i32
    }

    #[test]
    fn long_text_grows_to_fit_and_gives_up_on_a_broken_function() {
        let long = "x".repeat(1000);
        let text = |want: &str| long_text(|out, cap| fake_write(want, out, cap));
        assert_eq!(text(&long).as_deref(), Some(long.as_str()));
        assert_eq!(text("short").as_deref(), Some("short"));
        assert_eq!(long_text(|_, _| -1), None);
        let calls = std::cell::Cell::new(0);
        let always_too_long = |_: *mut c_char, cap: usize| {
            calls.set(calls.get() + 1);
            cap as i32
        };
        assert_eq!(long_text(always_too_long), None, "always one more than fits");
        assert_eq!(calls.get(), 3);
    }

    #[test]
    fn binned_arrays_are_read_bin_by_bin_and_safely() {
        // Five windows in bins of two: [a, b], [c, d], [e, unused].
        let windows: Vec<*mut CXWnd> = (1..=5).map(|i| (i * 8) as *mut CXWnd).collect();
        let mut storage: Vec<Vec<*mut CXWnd>> = windows.chunks(2).map(|bin| [bin, &[std::ptr::null_mut()][..]].concat()).collect();
        let mut bins: Vec<*mut *mut CXWnd> = storage.iter_mut().map(|bin| bin.as_mut_ptr()).collect();
        let array = Fake::<ArrayClass2_CXWnd>::new();
        let raw = array.get();
        raw.set_m_array(bins.as_mut_ptr());
        raw.set_m_binCount(3);
        raw.set_m_binShift(1);
        raw.set_m_slotMask(1);
        raw.set_m_length(5);
        assert_eq!(binned_windows(raw), windows);

        raw.set_m_binCount(2);
        assert_eq!(binned_windows(raw), windows[..4], "not past the last bin");
        raw.set_m_binCount(3);
        raw.set_m_slotMask(7);
        assert_eq!(binned_windows(raw), windows[..2], "not past a bin's end");
        raw.set_m_slotMask(1);
        for shift in [-1, 16] {
            raw.set_m_binShift(shift);
            assert!(binned_windows(raw).is_empty(), "shift {shift}");
        }
        raw.set_m_binShift(1);
        raw.set_m_array(std::ptr::null_mut());
        assert!(binned_windows(raw).is_empty());
    }

    #[test]
    fn the_open_context_menu_is_the_current_one() {
        let manager = Fake::<CContextMenuManager>::new();
        let menus = [Fake::<CContextMenu>::new(), Fake::<CContextMenu>::new()];
        manager.get().set_pCurrMenus_at(0, menus[0].ptr());
        manager.get().set_pCurrMenus_at(1, menus[1].ptr());
        assert!(open_context_menu(manager.get()).is_null(), "none visible");
        manager.get().set_NumVisibleMenus(2);
        manager.get().set_CurrMenu(1);
        assert_eq!(open_context_menu(manager.get()), menus[1].ptr());
        manager.get().set_CurrMenu(MAX_CONTEXT_MENU_DEPTH as i32);
        assert!(open_context_menu(manager.get()).is_null(), "past the array");
        manager.get().set_CurrMenu(-1);
        assert!(open_context_menu(manager.get()).is_null());
    }
}
