//! Drawing with Dear ImGui, MacroQuest's overlay UI.
//!
//! [`Plugin::on_update_imgui`](crate::Plugin::on_update_imgui) gets a
//! [`Ui`] once per frame; everything is drawn through it, immediate-mode:
//! describe the window every frame, and widgets report what the user did.
//!
//! ```ignore
//! fn on_update_imgui(&mut self, mq: &Mq<Self>, ui: &mut Ui) {
//!     ui.window("My plugin").open(&mut self.show).size([300.0, 200.0], Cond::FirstUseEver).show(|ui| {
//!         ui.text(format!("{} pulses", mq.stats().pulses));
//!         if ui.button("Say hello") {
//!             mq.command("/say hello");
//!         }
//!         ui.checkbox("Enabled", &mut self.enabled);
//!     });
//! }
//! ```
//!
//! # Scopes
//!
//! ImGui has many begin/end pairs (windows, tables, tree nodes, ID and style
//! stacks). Here each is a method taking a closure: the end call always
//! happens, in the right order, even if the closure panics, and the closure
//! only runs when ImGui says to draw the contents (a collapsed window's
//! doesn't). A mismatched pair would make MacroQuest pause ImGui for every
//! plugin, and some misuse crashes the game outright (a table row outside a
//! table), so the API makes them impossible:
//!
//! - Scopes borrow the [`Ui`] mutably, so inside one only the closure's
//!   argument can draw: a table's [`Table`] can't be reached from a window
//!   opened inside one of its cells, where it isn't current.
//! - Calls that only make sense inside something (table rows, tab items)
//!   are methods of that scope's argument ([`Table`], [`TabBar`]).
//!
//! ```no_run
//! # fn draw(ui: &mut mq_emu::Ui) {
//! ui.table("spawns", 2).show(|table| {
//!     table.next_row();
//!     table.next_column();
//!     table.child("details").show(|ui| ui.text("inside the cell"));
//! });
//! # }
//! ```
//!
//! ```compile_fail,E0500
//! # fn draw(ui: &mut mq_emu::Ui) {
//! ui.table("spawns", 2).show(|table| {
//!     table.child("details").show(|ui| {
//!         table.next_row(); // the table isn't current in here
//!     });
//! });
//! # }
//! ```
//!
//! If ImGui still reports an error, it's caught (it never reaches Rust as an
//! exception) and shown once in chat.
//!
//! Labels follow ImGui's ID rules: `"Save##settings"` shows "Save" with an ID
//! of the whole label, and `"###id"` keeps an ID while the text changes. Use
//! [`Ui::with_id`] to tell apart widgets with the same label.
//!
//! # Beyond widgets
//!
//! - Drawing: [`Ui::draw_list`] (on the window), [`Ui::foreground_draw_list`]
//!   and [`Ui::background_draw_list`] give a [`DrawList`]: shapes, text,
//!   images and [paths](DrawList::path), with an [`Ui::invisible_button`]
//!   as the hit area of something custom.
//! - Images: [`Ui::load_texture`] loads a file; [`Ui::find_texture_animation`]
//!   finds the game's own art, such as item and spell icons ([`ITEM_ICONS`]).
//! - Drag and drop: [`Ui::drag_drop_source`] and [`Ui::drag_drop_target`] on
//!   the last item.
//! - Fonts: [`Ui::with_font`] with MacroQuest's, EverQuest's or a loaded
//!   [`Font`]; [`Ui::with_font_size`] for size alone.
//! - Docking: [`Ui::dock_space`] and [`Window::dock`], and layouts built in
//!   code with [`Ui::dock_builder`].
//! - Custom widgets: [`Ui::add_item`] lays out an item of your own and
//!   [`Ui::button_behavior`] makes it clickable, as ImGui's widgets are
//!   made; [`Ui::splitter`], [`Ui::render_frame`] and the like, and
//!   [`Ui::button_with`] for ImGui's private button flags.
//! - Long lists: [`Ui::list_clipper`] only draws the items in view, in a
//!   window, a [table](Table::list_clipper) or a
//!   [multi-select list](MultiSelect::list_clipper).
//! - Multi-select: [`Ui::multi_select`] lets the user select many items at
//!   once (ctrl+click, shift+click, box select) into a [`Selection`].
//! - ImPlot: [`Ui::plot`] and [`Ui::subplots`] (see [`Plot`]), with items
//!   from closures ([`Plot::line_fn`]), axis formatters and custom scales
//!   ([`Plot::setup_axis_format_fn`], [`Plot::setup_axis_scale_fn`]), and
//!   which buttons do what ([`PlotInputMap`]).
//! - The game's look: spell gems ([`Ui::spell_gem`]), item link text
//!   ([`Ui::item_link_text`]) and text in EverQuest's fonts
//!   ([`DrawList::add_eq_text`]).

use std::ffi::{c_char, c_void};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, RangeInclusive};

use mq_emu_sys::imgui as sys;
use mq_emu_sys::imgui_mq;
use mq_emu_sys::implot;

/// An RGBA color, each component 0.0 to 1.0.
pub type Color = [f32; 4];

/// Draws ImGui windows and widgets. Only exists during
/// [`Plugin::on_update_imgui`](crate::Plugin::on_update_imgui), inside
/// MacroQuest's ImGui frame.
pub struct Ui {
    api: &'static sys::Api,
    plot: &'static implot::Api,
    mq: &'static imgui_mq::Api,
    _not_send: PhantomData<*const ()>,
}

/// The shim's function tables a [`Ui`] draws through: ImGui, ImPlot, and
/// MacroQuest's own (textures, EQ fonts). See `mq_emu_sys::imgui::Api` for
/// why they're tables.
#[derive(Clone, Copy)]
pub(crate) struct Apis {
    pub imgui: &'static sys::Api,
    pub implot: &'static implot::Api,
    pub mq: &'static imgui_mq::Api,
}

/// Calls an ImGui end function when dropped, so scopes end even on a panic.
struct EndOnDrop(unsafe extern "C" fn());

impl Drop for EndOnDrop {
    fn drop(&mut self) {
        unsafe { (self.0)() }
    }
}

/// Runs a closure when dropped, for ends that take arguments.
struct OnDrop<F: FnMut()>(F);

impl<F: FnMut()> Drop for OnDrop<F> {
    fn drop(&mut self) {
        (self.0)()
    }
}

fn s(text: &str) -> (*const c_char, usize) {
    (text.as_ptr().cast(), text.len())
}

/// An optional string for the shim: absent as `NO_STRING`.
fn opt(text: Option<&str>) -> (*const c_char, usize) {
    text.map_or((std::ptr::null(), sys::NO_STRING), s)
}

/// When a `set_next_*`/window setting applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum Cond {
    /// Every frame.
    #[default]
    Always = sys::ImGuiCond_Always,
    /// Once per run (the first time the window is drawn).
    Once = sys::ImGuiCond_Once,
    /// Only if the window has no saved state yet (in MacroQuest's ImGui ini).
    FirstUseEver = sys::ImGuiCond_FirstUseEver,
    /// Whenever the window appears after being hidden.
    Appearing = sys::ImGuiCond_Appearing,
}

/// A mouse button (`ImGuiMouseButton_`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum MouseButton {
    Left = sys::ImGuiMouseButton_Left,
    Right = sys::ImGuiMouseButton_Right,
    Middle = sys::ImGuiMouseButton_Middle,
    /// The first extra button (back, on most mice), which ImGui numbers 3.
    Extra1 = 3,
    /// The second extra button (forward), 4.
    Extra2 = 4,
}

impl MouseButton {
    /// The button ImGui numbers `index`.
    pub fn from_index(index: i32) -> Option<MouseButton> {
        use MouseButton::*;
        [Left, Right, Middle, Extra1, Extra2].into_iter().find(|b| *b as i32 == index)
    }
}

/// A dock space or dock node ([`Ui::dock_id`], [`Ui::dock_space`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DockId(pub u32);

macro_rules! flags {
    ($(#[$meta:meta])* $name:ident { $($(#[$flag_meta:meta])* $flag:ident = $value:ident,)* }) => {
        flags! { $(#[$meta])* $name { $($(#[$flag_meta])* $flag = $value,)* } in sys }
    };
    ($(#[$meta:meta])* $name:ident { $($(#[$flag_meta:meta])* $flag:ident = $value:ident,)* } in $module:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name(i32);

        impl $name {
            pub const NONE: $name = $name(0);
            $( $(#[$flag_meta])* pub const $flag: $name = $name($module::$value); )*

            pub const fn bits(self) -> i32 {
                self.0
            }

            pub const fn contains(self, other: $name) -> bool {
                self.0 & other.0 == other.0
            }
        }

        impl ::std::ops::BitOr for $name {
            type Output = $name;
            fn bitor(self, other: $name) -> $name {
                $name(self.0 | other.0)
            }
        }

        impl ::std::ops::BitOrAssign for $name {
            fn bitor_assign(&mut self, other: $name) {
                self.0 |= other.0;
            }
        }
    };
}

mod clip;
mod custom;
mod dnd;
mod dock;
mod draw;
mod eq;
mod font;
mod plot;
mod select;
mod texture;

pub use clip::ListClipper;
pub use custom::{ButtonState, Item, ItemId, Split};
pub use dnd::{DragDropFlags, DragSource, DropTarget, MAX_PAYLOAD_KIND, Payload};
pub use dock::{Dir, DockBuilder, DockNode};
pub use draw::{Channels, Corners, DrawList, Path, Vec2, pack_color};
pub use eq::{EqTextFlags, SpellGemFlags};
pub use font::{Font, LoadedFont};
pub use plot::{
    Axis, AxisFlags, BarGroupsFlags, BarsFlags, Bins, Colormap, ColormapScaleFlags, DigitalFlags, DragState, DragToolFlags, DummyFlags,
    ErrorBarsFlags, HeatmapFlags, HistogramFlags, InfLinesFlags, LegendFlags, LineFlags, Location, Marker, MouseTextFlags, PieChartFlags,
    Plot, PlotBuilder, PlotFlags, PlotImageFlags, PlotInputMap, PlotRect, PlotStyleColor, PlotStyleVar, PlotValue, Scale, ScatterFlags,
    Series, ShadedFlags, StairsFlags, StemsFlags, SubplotFlags, Subplots, SubplotsBuilder,
};
pub use select::{MultiSelect, MultiSelectFlags, MultiSelectIo, Selection, SelectionRequest};
pub use texture::{ITEM_ICONS, ImageOptions, SPELL_ICONS, Texture, TextureAnimation, TextureId};
pub(crate) use font::remove_loaded_fonts;
pub(crate) use texture::{free_retired_textures, ui_unloaded};

flags! {
    /// Options for [`Ui::window`] (`ImGuiWindowFlags_`).
    WindowFlags {
        NO_TITLE_BAR = ImGuiWindowFlags_NoTitleBar,
        NO_RESIZE = ImGuiWindowFlags_NoResize,
        NO_MOVE = ImGuiWindowFlags_NoMove,
        NO_SCROLLBAR = ImGuiWindowFlags_NoScrollbar,
        NO_SCROLL_WITH_MOUSE = ImGuiWindowFlags_NoScrollWithMouse,
        NO_COLLAPSE = ImGuiWindowFlags_NoCollapse,
        ALWAYS_AUTO_RESIZE = ImGuiWindowFlags_AlwaysAutoResize,
        NO_BACKGROUND = ImGuiWindowFlags_NoBackground,
        /// Don't save position and size in MacroQuest's ImGui ini.
        NO_SAVED_SETTINGS = ImGuiWindowFlags_NoSavedSettings,
        NO_MOUSE_INPUTS = ImGuiWindowFlags_NoMouseInputs,
        /// Gives the window a menu bar; draw it with [`Ui::menu_bar`].
        MENU_BAR = ImGuiWindowFlags_MenuBar,
        HORIZONTAL_SCROLLBAR = ImGuiWindowFlags_HorizontalScrollbar,
        NO_FOCUS_ON_APPEARING = ImGuiWindowFlags_NoFocusOnAppearing,
        NO_BRING_TO_FRONT_ON_FOCUS = ImGuiWindowFlags_NoBringToFrontOnFocus,
        ALWAYS_VERTICAL_SCROLLBAR = ImGuiWindowFlags_AlwaysVerticalScrollbar,
        ALWAYS_HORIZONTAL_SCROLLBAR = ImGuiWindowFlags_AlwaysHorizontalScrollbar,
        NO_NAV_INPUTS = ImGuiWindowFlags_NoNavInputs,
        NO_NAV_FOCUS = ImGuiWindowFlags_NoNavFocus,
        UNSAVED_DOCUMENT = ImGuiWindowFlags_UnsavedDocument,
        NO_DOCKING = ImGuiWindowFlags_NoDocking,
        NO_NAV = ImGuiWindowFlags_NoNav,
        NO_DECORATION = ImGuiWindowFlags_NoDecoration,
        NO_INPUTS = ImGuiWindowFlags_NoInputs,
    }
}

flags! {
    /// Options for [`Ui::invisible_button_with`] (`ImGuiButtonFlags_`).
    ButtonFlags {
        /// React to the left button (the default when none is given).
        MOUSE_BUTTON_LEFT = ImGuiButtonFlags_MouseButtonLeft,
        MOUSE_BUTTON_RIGHT = ImGuiButtonFlags_MouseButtonRight,
        MOUSE_BUTTON_MIDDLE = ImGuiButtonFlags_MouseButtonMiddle,
        /// Reachable with keyboard navigation.
        ENABLE_NAV = ImGuiButtonFlags_EnableNav,
        // ImGui's private flags (imgui_internal.h), for custom widgets.
        /// Pressed when the mouse goes down on it.
        PRESSED_ON_CLICK = ImGuiButtonFlags_PressedOnClick,
        /// Pressed when the mouse goes down and back up on it (the default).
        PRESSED_ON_CLICK_RELEASE = ImGuiButtonFlags_PressedOnClickRelease,
        /// Pressed when the mouse goes down on it and back up anywhere.
        PRESSED_ON_CLICK_RELEASE_ANYWHERE = ImGuiButtonFlags_PressedOnClickReleaseAnywhere,
        /// Pressed when the mouse is released on it, wherever it went down.
        PRESSED_ON_RELEASE = ImGuiButtonFlags_PressedOnRelease,
        PRESSED_ON_DOUBLE_CLICK = ImGuiButtonFlags_PressedOnDoubleClick,
        /// Pressed when something dragged is held over it (as tree nodes open).
        PRESSED_ON_DRAG_DROP_HOLD = ImGuiButtonFlags_PressedOnDragDropHold,
        /// Works even with a child window over it.
        FLATTEN_CHILDREN = ImGuiButtonFlags_FlattenChildren,
        /// Lets an item drawn after it, over it, take the mouse.
        ALLOW_OVERLAP = ImGuiButtonFlags_AllowOverlap,
        /// Lines the button up with text on the line ([`Ui::button_with`] only).
        ALIGN_TEXT_BASE_LINE = ImGuiButtonFlags_AlignTextBaseLine,
        /// Ignores the mouse while ctrl, shift or alt is held.
        NO_KEY_MODS_ALLOWED = ImGuiButtonFlags_NoKeyModsAllowed,
        /// Doesn't become the active item while held (with PRESSED_ON_CLICK).
        NO_HOLDING_ACTIVE_ID = ImGuiButtonFlags_NoHoldingActiveId,
        /// Doesn't take keyboard navigation focus when clicked.
        NO_NAV_FOCUS = ImGuiButtonFlags_NoNavFocus,
        /// Not reported hovered while it has keyboard focus.
        NO_HOVERED_ON_FOCUS = ImGuiButtonFlags_NoHoveredOnFocus,
        /// Doesn't claim the mouse button when clicked.
        NO_SET_KEY_OWNER = ImGuiButtonFlags_NoSetKeyOwner,
        /// Reacts to the mouse button even if something else claimed it.
        NO_TEST_KEY_OWNER = ImGuiButtonFlags_NoTestKeyOwner,
        /// Clicking it doesn't focus its window (experimental in ImGui).
        NO_FOCUS = ImGuiButtonFlags_NoFocus,
    }
}

flags! {
    /// Options for [`Ui::dock_space`] and the [`DockBuilder`]'s nodes
    /// (`ImGuiDockNodeFlags_`). The ones after [`DockNodeFlags::NO_UNDOCKING`]
    /// are ImGui's internal flags, meant for nodes the dock builder makes.
    DockNodeFlags {
        /// Keep the dock space (and what's docked in it) without showing it.
        KEEP_ALIVE_ONLY = ImGuiDockNodeFlags_KeepAliveOnly,
        NO_DOCKING_OVER_CENTRAL_NODE = ImGuiDockNodeFlags_NoDockingOverCentralNode,
        /// Clicks go through the empty middle, and nothing is drawn there.
        PASSTHRU_CENTRAL_NODE = ImGuiDockNodeFlags_PassthruCentralNode,
        NO_DOCKING_SPLIT = ImGuiDockNodeFlags_NoDockingSplit,
        NO_RESIZE = ImGuiDockNodeFlags_NoResize,
        AUTO_HIDE_TAB_BAR = ImGuiDockNodeFlags_AutoHideTabBar,
        NO_UNDOCKING = ImGuiDockNodeFlags_NoUndocking,
        /// No tab bar, and no way to bring it back.
        NO_TAB_BAR = ImGuiDockNodeFlags_NoTabBar,
        /// The tab bar hidden, with a triangle in the corner to show it.
        HIDDEN_TAB_BAR = ImGuiDockNodeFlags_HiddenTabBar,
        NO_WINDOW_MENU_BUTTON = ImGuiDockNodeFlags_NoWindowMenuButton,
        NO_CLOSE_BUTTON = ImGuiDockNodeFlags_NoCloseButton,
        NO_RESIZE_X = ImGuiDockNodeFlags_NoResizeX,
        NO_RESIZE_Y = ImGuiDockNodeFlags_NoResizeY,
        /// Shortcuts of the docking window work while any window docked in it has focus.
        DOCKED_WINDOWS_IN_FOCUS_ROUTE = ImGuiDockNodeFlags_DockedWindowsInFocusRoute,
        /// The node can't split other windows or nodes.
        NO_DOCKING_SPLIT_OTHER = ImGuiDockNodeFlags_NoDockingSplitOther,
        /// Nothing can be docked over this node.
        NO_DOCKING_OVER_ME = ImGuiDockNodeFlags_NoDockingOverMe,
        /// The node can't be docked over another window or non-empty node.
        NO_DOCKING_OVER_OTHER = ImGuiDockNodeFlags_NoDockingOverOther,
        /// The node can't be docked over an empty node.
        NO_DOCKING_OVER_EMPTY = ImGuiDockNodeFlags_NoDockingOverEmpty,
        /// No docking or undocking in or out of this node at all.
        NO_DOCKING = ImGuiDockNodeFlags_NoDocking,
    }
}

flags! {
    /// Modifier keys (`ImGuiMod_`), as [`PlotInputMap`] takes them.
    KeyMods {
        CTRL = ImGuiMod_Ctrl,
        SHIFT = ImGuiMod_Shift,
        ALT = ImGuiMod_Alt,
        /// The Windows key.
        SUPER = ImGuiMod_Super,
    }
}

flags! {
    /// Options for [`Ui::child`] (`ImGuiChildFlags_`).
    ChildFlags {
        BORDERS = ImGuiChildFlags_Borders,
        ALWAYS_USE_WINDOW_PADDING = ImGuiChildFlags_AlwaysUseWindowPadding,
        RESIZE_X = ImGuiChildFlags_ResizeX,
        RESIZE_Y = ImGuiChildFlags_ResizeY,
        AUTO_RESIZE_X = ImGuiChildFlags_AutoResizeX,
        AUTO_RESIZE_Y = ImGuiChildFlags_AutoResizeY,
        ALWAYS_AUTO_RESIZE = ImGuiChildFlags_AlwaysAutoResize,
        FRAME_STYLE = ImGuiChildFlags_FrameStyle,
        NAV_FLATTENED = ImGuiChildFlags_NavFlattened,
    }
}

flags! {
    /// Options for [`Ui::table`] (`ImGuiTableFlags_`).
    TableFlags {
        RESIZABLE = ImGuiTableFlags_Resizable,
        REORDERABLE = ImGuiTableFlags_Reorderable,
        HIDEABLE = ImGuiTableFlags_Hideable,
        NO_SAVED_SETTINGS = ImGuiTableFlags_NoSavedSettings,
        CONTEXT_MENU_IN_BODY = ImGuiTableFlags_ContextMenuInBody,
        ROW_BG = ImGuiTableFlags_RowBg,
        BORDERS_INNER_H = ImGuiTableFlags_BordersInnerH,
        BORDERS_OUTER_H = ImGuiTableFlags_BordersOuterH,
        BORDERS_INNER_V = ImGuiTableFlags_BordersInnerV,
        BORDERS_OUTER_V = ImGuiTableFlags_BordersOuterV,
        BORDERS_H = ImGuiTableFlags_BordersH,
        BORDERS_V = ImGuiTableFlags_BordersV,
        BORDERS_INNER = ImGuiTableFlags_BordersInner,
        BORDERS_OUTER = ImGuiTableFlags_BordersOuter,
        BORDERS = ImGuiTableFlags_Borders,
        NO_BORDERS_IN_BODY = ImGuiTableFlags_NoBordersInBody,
        NO_BORDERS_IN_BODY_UNTIL_RESIZE = ImGuiTableFlags_NoBordersInBodyUntilResize,
        SIZING_FIXED_FIT = ImGuiTableFlags_SizingFixedFit,
        SIZING_FIXED_SAME = ImGuiTableFlags_SizingFixedSame,
        SIZING_STRETCH_PROP = ImGuiTableFlags_SizingStretchProp,
        SIZING_STRETCH_SAME = ImGuiTableFlags_SizingStretchSame,
        NO_HOST_EXTEND_X = ImGuiTableFlags_NoHostExtendX,
        NO_HOST_EXTEND_Y = ImGuiTableFlags_NoHostExtendY,
        NO_KEEP_COLUMNS_VISIBLE = ImGuiTableFlags_NoKeepColumnsVisible,
        PRECISE_WIDTHS = ImGuiTableFlags_PreciseWidths,
        NO_CLIP = ImGuiTableFlags_NoClip,
        PAD_OUTER_X = ImGuiTableFlags_PadOuterX,
        NO_PAD_OUTER_X = ImGuiTableFlags_NoPadOuterX,
        NO_PAD_INNER_X = ImGuiTableFlags_NoPadInnerX,
        SCROLL_X = ImGuiTableFlags_ScrollX,
        SCROLL_Y = ImGuiTableFlags_ScrollY,
        HIGHLIGHT_HOVERED_COLUMN = ImGuiTableFlags_HighlightHoveredColumn,
    }
}

flags! {
    /// Options for [`Table::setup_column_with`] (`ImGuiTableColumnFlags_`).
    TableColumnFlags {
        DISABLED = ImGuiTableColumnFlags_Disabled,
        DEFAULT_HIDE = ImGuiTableColumnFlags_DefaultHide,
        WIDTH_STRETCH = ImGuiTableColumnFlags_WidthStretch,
        WIDTH_FIXED = ImGuiTableColumnFlags_WidthFixed,
        NO_RESIZE = ImGuiTableColumnFlags_NoResize,
        NO_REORDER = ImGuiTableColumnFlags_NoReorder,
        NO_HIDE = ImGuiTableColumnFlags_NoHide,
        NO_CLIP = ImGuiTableColumnFlags_NoClip,
        NO_HEADER_LABEL = ImGuiTableColumnFlags_NoHeaderLabel,
        NO_HEADER_WIDTH = ImGuiTableColumnFlags_NoHeaderWidth,
        INDENT_ENABLE = ImGuiTableColumnFlags_IndentEnable,
        INDENT_DISABLE = ImGuiTableColumnFlags_IndentDisable,
        ANGLED_HEADER = ImGuiTableColumnFlags_AngledHeader,
    }
}

flags! {
    /// Options for [`Ui::tree_node_with`] and [`Ui::collapsing_header_with`] (`ImGuiTreeNodeFlags_`).
    TreeNodeFlags {
        SELECTED = ImGuiTreeNodeFlags_Selected,
        FRAMED = ImGuiTreeNodeFlags_Framed,
        ALLOW_OVERLAP = ImGuiTreeNodeFlags_AllowOverlap,
        DEFAULT_OPEN = ImGuiTreeNodeFlags_DefaultOpen,
        OPEN_ON_DOUBLE_CLICK = ImGuiTreeNodeFlags_OpenOnDoubleClick,
        OPEN_ON_ARROW = ImGuiTreeNodeFlags_OpenOnArrow,
        LEAF = ImGuiTreeNodeFlags_Leaf,
        BULLET = ImGuiTreeNodeFlags_Bullet,
        FRAME_PADDING = ImGuiTreeNodeFlags_FramePadding,
        SPAN_AVAIL_WIDTH = ImGuiTreeNodeFlags_SpanAvailWidth,
        SPAN_FULL_WIDTH = ImGuiTreeNodeFlags_SpanFullWidth,
        SPAN_LABEL_WIDTH = ImGuiTreeNodeFlags_SpanLabelWidth,
        SPAN_ALL_COLUMNS = ImGuiTreeNodeFlags_SpanAllColumns,
        LABEL_SPAN_ALL_COLUMNS = ImGuiTreeNodeFlags_LabelSpanAllColumns,
        NAV_LEFT_JUMPS_TO_PARENT = ImGuiTreeNodeFlags_NavLeftJumpsToParent,
        DRAW_LINES_NONE = ImGuiTreeNodeFlags_DrawLinesNone,
        DRAW_LINES_FULL = ImGuiTreeNodeFlags_DrawLinesFull,
        DRAW_LINES_TO_NODES = ImGuiTreeNodeFlags_DrawLinesToNodes,
    }
}

flags! {
    /// Options for [`Ui::selectable_with`] (`ImGuiSelectableFlags_`).
    SelectableFlags {
        NO_AUTO_CLOSE_POPUPS = ImGuiSelectableFlags_NoAutoClosePopups,
        SPAN_ALL_COLUMNS = ImGuiSelectableFlags_SpanAllColumns,
        ALLOW_DOUBLE_CLICK = ImGuiSelectableFlags_AllowDoubleClick,
        DISABLED = ImGuiSelectableFlags_Disabled,
        ALLOW_OVERLAP = ImGuiSelectableFlags_AllowOverlap,
        HIGHLIGHT = ImGuiSelectableFlags_Highlight,
    }
}

flags! {
    /// Options for [`Ui::tab_bar_with`] (`ImGuiTabBarFlags_`).
    TabBarFlags {
        REORDERABLE = ImGuiTabBarFlags_Reorderable,
        AUTO_SELECT_NEW_TABS = ImGuiTabBarFlags_AutoSelectNewTabs,
        TAB_LIST_POPUP_BUTTON = ImGuiTabBarFlags_TabListPopupButton,
        NO_CLOSE_WITH_MIDDLE_MOUSE_BUTTON = ImGuiTabBarFlags_NoCloseWithMiddleMouseButton,
        NO_TAB_LIST_SCROLLING_BUTTONS = ImGuiTabBarFlags_NoTabListScrollingButtons,
        NO_TOOLTIP = ImGuiTabBarFlags_NoTooltip,
        DRAW_SELECTED_OVERLINE = ImGuiTabBarFlags_DrawSelectedOverline,
        FITTING_POLICY_MIXED = ImGuiTabBarFlags_FittingPolicyMixed,
        FITTING_POLICY_SHRINK = ImGuiTabBarFlags_FittingPolicyShrink,
        FITTING_POLICY_SCROLL = ImGuiTabBarFlags_FittingPolicyScroll,
    }
}

flags! {
    /// Options for [`TabBar::tab_with`] (`ImGuiTabItemFlags_`).
    TabItemFlags {
        UNSAVED_DOCUMENT = ImGuiTabItemFlags_UnsavedDocument,
        SET_SELECTED = ImGuiTabItemFlags_SetSelected,
        NO_CLOSE_WITH_MIDDLE_MOUSE_BUTTON = ImGuiTabItemFlags_NoCloseWithMiddleMouseButton,
        NO_TOOLTIP = ImGuiTabItemFlags_NoTooltip,
        NO_REORDER = ImGuiTabItemFlags_NoReorder,
        LEADING = ImGuiTabItemFlags_Leading,
        TRAILING = ImGuiTabItemFlags_Trailing,
    }
}

flags! {
    /// Options for [`Ui::input_text_with`] (`ImGuiInputTextFlags_`).
    InputTextFlags {
        CHARS_DECIMAL = ImGuiInputTextFlags_CharsDecimal,
        CHARS_HEXADECIMAL = ImGuiInputTextFlags_CharsHexadecimal,
        CHARS_SCIENTIFIC = ImGuiInputTextFlags_CharsScientific,
        CHARS_UPPERCASE = ImGuiInputTextFlags_CharsUppercase,
        CHARS_NO_BLANK = ImGuiInputTextFlags_CharsNoBlank,
        ALLOW_TAB_INPUT = ImGuiInputTextFlags_AllowTabInput,
        /// Return true only when Enter is pressed, rather than on every edit.
        ENTER_RETURNS_TRUE = ImGuiInputTextFlags_EnterReturnsTrue,
        ESCAPE_CLEARS_ALL = ImGuiInputTextFlags_EscapeClearsAll,
        CTRL_ENTER_FOR_NEW_LINE = ImGuiInputTextFlags_CtrlEnterForNewLine,
        READ_ONLY = ImGuiInputTextFlags_ReadOnly,
        PASSWORD = ImGuiInputTextFlags_Password,
        ALWAYS_OVERWRITE = ImGuiInputTextFlags_AlwaysOverwrite,
        AUTO_SELECT_ALL = ImGuiInputTextFlags_AutoSelectAll,
        NO_HORIZONTAL_SCROLL = ImGuiInputTextFlags_NoHorizontalScroll,
        NO_UNDO_REDO = ImGuiInputTextFlags_NoUndoRedo,
        ELIDE_LEFT = ImGuiInputTextFlags_ElideLeft,
        WORD_WRAP = ImGuiInputTextFlags_WordWrap,
    }
}

/// A style color, for [`Ui::with_style_color`] (`ImGuiCol_`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum StyleColor {
    Text = sys::ImGuiCol_Text,
    TextDisabled = sys::ImGuiCol_TextDisabled,
    WindowBg = sys::ImGuiCol_WindowBg,
    ChildBg = sys::ImGuiCol_ChildBg,
    PopupBg = sys::ImGuiCol_PopupBg,
    Border = sys::ImGuiCol_Border,
    BorderShadow = sys::ImGuiCol_BorderShadow,
    FrameBg = sys::ImGuiCol_FrameBg,
    FrameBgHovered = sys::ImGuiCol_FrameBgHovered,
    FrameBgActive = sys::ImGuiCol_FrameBgActive,
    TitleBg = sys::ImGuiCol_TitleBg,
    TitleBgActive = sys::ImGuiCol_TitleBgActive,
    TitleBgCollapsed = sys::ImGuiCol_TitleBgCollapsed,
    MenuBarBg = sys::ImGuiCol_MenuBarBg,
    ScrollbarBg = sys::ImGuiCol_ScrollbarBg,
    ScrollbarGrab = sys::ImGuiCol_ScrollbarGrab,
    ScrollbarGrabHovered = sys::ImGuiCol_ScrollbarGrabHovered,
    ScrollbarGrabActive = sys::ImGuiCol_ScrollbarGrabActive,
    CheckMark = sys::ImGuiCol_CheckMark,
    SliderGrab = sys::ImGuiCol_SliderGrab,
    SliderGrabActive = sys::ImGuiCol_SliderGrabActive,
    Button = sys::ImGuiCol_Button,
    ButtonHovered = sys::ImGuiCol_ButtonHovered,
    ButtonActive = sys::ImGuiCol_ButtonActive,
    Header = sys::ImGuiCol_Header,
    HeaderHovered = sys::ImGuiCol_HeaderHovered,
    HeaderActive = sys::ImGuiCol_HeaderActive,
    Separator = sys::ImGuiCol_Separator,
    SeparatorHovered = sys::ImGuiCol_SeparatorHovered,
    SeparatorActive = sys::ImGuiCol_SeparatorActive,
    ResizeGrip = sys::ImGuiCol_ResizeGrip,
    ResizeGripHovered = sys::ImGuiCol_ResizeGripHovered,
    ResizeGripActive = sys::ImGuiCol_ResizeGripActive,
    InputTextCursor = sys::ImGuiCol_InputTextCursor,
    TabHovered = sys::ImGuiCol_TabHovered,
    Tab = sys::ImGuiCol_Tab,
    TabSelected = sys::ImGuiCol_TabSelected,
    TabSelectedOverline = sys::ImGuiCol_TabSelectedOverline,
    TabDimmed = sys::ImGuiCol_TabDimmed,
    TabDimmedSelected = sys::ImGuiCol_TabDimmedSelected,
    TabDimmedSelectedOverline = sys::ImGuiCol_TabDimmedSelectedOverline,
    DockingPreview = sys::ImGuiCol_DockingPreview,
    DockingEmptyBg = sys::ImGuiCol_DockingEmptyBg,
    PlotLines = sys::ImGuiCol_PlotLines,
    PlotLinesHovered = sys::ImGuiCol_PlotLinesHovered,
    PlotHistogram = sys::ImGuiCol_PlotHistogram,
    PlotHistogramHovered = sys::ImGuiCol_PlotHistogramHovered,
    TableHeaderBg = sys::ImGuiCol_TableHeaderBg,
    TableBorderStrong = sys::ImGuiCol_TableBorderStrong,
    TableBorderLight = sys::ImGuiCol_TableBorderLight,
    TableRowBg = sys::ImGuiCol_TableRowBg,
    TableRowBgAlt = sys::ImGuiCol_TableRowBgAlt,
    TextLink = sys::ImGuiCol_TextLink,
    TextSelectedBg = sys::ImGuiCol_TextSelectedBg,
    TreeLines = sys::ImGuiCol_TreeLines,
    DragDropTarget = sys::ImGuiCol_DragDropTarget,
    DragDropTargetBg = sys::ImGuiCol_DragDropTargetBg,
    UnsavedMarker = sys::ImGuiCol_UnsavedMarker,
    NavCursor = sys::ImGuiCol_NavCursor,
    NavWindowingHighlight = sys::ImGuiCol_NavWindowingHighlight,
    NavWindowingDimBg = sys::ImGuiCol_NavWindowingDimBg,
    ModalWindowDimBg = sys::ImGuiCol_ModalWindowDimBg,
}

/// A style variable with its new value, for [`Ui::with_style`] (`ImGuiStyleVar_`).
/// Sizes and padding are in pixels, `[x, y]`; alignments are 0.0 to 1.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StyleVar {
    Alpha(f32),
    /// Alpha multiplier for disabled items, on top of `Alpha`.
    DisabledAlpha(f32),
    WindowPadding([f32; 2]),
    WindowRounding(f32),
    WindowBorderSize(f32),
    WindowMinSize([f32; 2]),
    WindowTitleAlign([f32; 2]),
    ChildRounding(f32),
    ChildBorderSize(f32),
    PopupRounding(f32),
    PopupBorderSize(f32),
    FramePadding([f32; 2]),
    FrameRounding(f32),
    FrameBorderSize(f32),
    ItemSpacing([f32; 2]),
    ItemInnerSpacing([f32; 2]),
    IndentSpacing(f32),
    CellPadding([f32; 2]),
    ScrollbarSize(f32),
    ScrollbarRounding(f32),
    ScrollbarPadding(f32),
    GrabMinSize(f32),
    GrabRounding(f32),
    ImageBorderSize(f32),
    TabRounding(f32),
    TabBorderSize(f32),
    TabBarBorderSize(f32),
    TabBarOverlineSize(f32),
    TreeLinesSize(f32),
    TreeLinesRounding(f32),
    ButtonTextAlign([f32; 2]),
    SelectableTextAlign([f32; 2]),
    SeparatorTextBorderSize(f32),
    SeparatorTextAlign([f32; 2]),
    SeparatorTextPadding([f32; 2]),
}

/// A style variable's index and value; ImGui checks the value's kind.
enum StyleValue {
    Float(f32),
    Vec2([f32; 2]),
}

impl StyleVar {
    fn raw(self) -> (i32, StyleValue) {
        use StyleValue::{Float, Vec2};
        match self {
            StyleVar::Alpha(v) => (sys::ImGuiStyleVar_Alpha, Float(v)),
            StyleVar::DisabledAlpha(v) => (sys::ImGuiStyleVar_DisabledAlpha, Float(v)),
            StyleVar::WindowPadding(v) => (sys::ImGuiStyleVar_WindowPadding, Vec2(v)),
            StyleVar::WindowRounding(v) => (sys::ImGuiStyleVar_WindowRounding, Float(v)),
            StyleVar::WindowBorderSize(v) => (sys::ImGuiStyleVar_WindowBorderSize, Float(v)),
            StyleVar::WindowMinSize(v) => (sys::ImGuiStyleVar_WindowMinSize, Vec2(v)),
            StyleVar::WindowTitleAlign(v) => (sys::ImGuiStyleVar_WindowTitleAlign, Vec2(v)),
            StyleVar::ChildRounding(v) => (sys::ImGuiStyleVar_ChildRounding, Float(v)),
            StyleVar::ChildBorderSize(v) => (sys::ImGuiStyleVar_ChildBorderSize, Float(v)),
            StyleVar::PopupRounding(v) => (sys::ImGuiStyleVar_PopupRounding, Float(v)),
            StyleVar::PopupBorderSize(v) => (sys::ImGuiStyleVar_PopupBorderSize, Float(v)),
            StyleVar::FramePadding(v) => (sys::ImGuiStyleVar_FramePadding, Vec2(v)),
            StyleVar::FrameRounding(v) => (sys::ImGuiStyleVar_FrameRounding, Float(v)),
            StyleVar::FrameBorderSize(v) => (sys::ImGuiStyleVar_FrameBorderSize, Float(v)),
            StyleVar::ItemSpacing(v) => (sys::ImGuiStyleVar_ItemSpacing, Vec2(v)),
            StyleVar::ItemInnerSpacing(v) => (sys::ImGuiStyleVar_ItemInnerSpacing, Vec2(v)),
            StyleVar::IndentSpacing(v) => (sys::ImGuiStyleVar_IndentSpacing, Float(v)),
            StyleVar::CellPadding(v) => (sys::ImGuiStyleVar_CellPadding, Vec2(v)),
            StyleVar::ScrollbarSize(v) => (sys::ImGuiStyleVar_ScrollbarSize, Float(v)),
            StyleVar::ScrollbarRounding(v) => (sys::ImGuiStyleVar_ScrollbarRounding, Float(v)),
            StyleVar::ScrollbarPadding(v) => (sys::ImGuiStyleVar_ScrollbarPadding, Float(v)),
            StyleVar::GrabMinSize(v) => (sys::ImGuiStyleVar_GrabMinSize, Float(v)),
            StyleVar::GrabRounding(v) => (sys::ImGuiStyleVar_GrabRounding, Float(v)),
            StyleVar::ImageBorderSize(v) => (sys::ImGuiStyleVar_ImageBorderSize, Float(v)),
            StyleVar::TabRounding(v) => (sys::ImGuiStyleVar_TabRounding, Float(v)),
            StyleVar::TabBorderSize(v) => (sys::ImGuiStyleVar_TabBorderSize, Float(v)),
            StyleVar::TabBarBorderSize(v) => (sys::ImGuiStyleVar_TabBarBorderSize, Float(v)),
            StyleVar::TabBarOverlineSize(v) => (sys::ImGuiStyleVar_TabBarOverlineSize, Float(v)),
            StyleVar::TreeLinesSize(v) => (sys::ImGuiStyleVar_TreeLinesSize, Float(v)),
            StyleVar::TreeLinesRounding(v) => (sys::ImGuiStyleVar_TreeLinesRounding, Float(v)),
            StyleVar::ButtonTextAlign(v) => (sys::ImGuiStyleVar_ButtonTextAlign, Vec2(v)),
            StyleVar::SelectableTextAlign(v) => (sys::ImGuiStyleVar_SelectableTextAlign, Vec2(v)),
            StyleVar::SeparatorTextBorderSize(v) => (sys::ImGuiStyleVar_SeparatorTextBorderSize, Float(v)),
            StyleVar::SeparatorTextAlign(v) => (sys::ImGuiStyleVar_SeparatorTextAlign, Vec2(v)),
            StyleVar::SeparatorTextPadding(v) => (sys::ImGuiStyleVar_SeparatorTextPadding, Vec2(v)),
        }
    }
}

/// Something that can identify widgets on ImGui's ID stack ([`Ui::with_id`]).
pub trait Id {
    #[doc(hidden)]
    fn push(&self, api: &sys::Api);
}

impl Id for str {
    fn push(&self, api: &sys::Api) {
        let (p, n) = s(self);
        unsafe { (api.mqemu_imgui_push_id_str)(p, n) }
    }
}

impl Id for String {
    fn push(&self, api: &sys::Api) {
        self.as_str().push(api)
    }
}

impl<T: Id + ?Sized> Id for &T {
    fn push(&self, api: &sys::Api) {
        (**self).push(api)
    }
}

macro_rules! int_id {
    ($($t:ty),*) => {$(
        impl Id for $t {
            /// Hashed as ImGui's int ID; wider values wrap to 32 bits.
            fn push(&self, api: &sys::Api) {
                unsafe { (api.mqemu_imgui_push_id_int)(*self as i32) }
            }
        }
    )*};
}
int_id!(i32, u32, i64, u64, usize, isize, u16, i16, u8, i8);

/// A number a slider, drag or input box can edit.
pub trait Number: Copy + sealed::Sealed {
    #[doc(hidden)]
    const DEFAULT_FORMAT: &'static str;
    #[doc(hidden)]
    fn format_ok(format: &str) -> bool;
    #[doc(hidden)]
    fn slider(api: &sys::Api, label: &str, value: &mut Self, min: Self, max: Self, format: &str) -> bool;
    #[doc(hidden)]
    fn drag(api: &sys::Api, label: &str, value: &mut Self, speed: f32, min: Self, max: Self, format: &str) -> bool;
    #[doc(hidden)]
    fn input(api: &sys::Api, label: &str, value: &mut Self) -> bool;
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for f32 {}
    impl Sealed for i32 {}
}

impl Number for f32 {
    const DEFAULT_FORMAT: &'static str = "%.3f";

    fn format_ok(format: &str) -> bool {
        printf_conversions(format, b"fFeEgGaA").is_some_and(|n| n <= 1)
    }

    fn slider(api: &sys::Api, label: &str, value: &mut f32, min: f32, max: f32, format: &str) -> bool {
        let ((l, ln), (f, fl)) = (s(label), s(format));
        unsafe { (api.mqemu_imgui_slider_float)(l, ln, value, min, max, f, fl, 0) }
    }

    fn drag(api: &sys::Api, label: &str, value: &mut f32, speed: f32, min: f32, max: f32, format: &str) -> bool {
        let ((l, ln), (f, fl)) = (s(label), s(format));
        unsafe { (api.mqemu_imgui_drag_float)(l, ln, value, speed, min, max, f, fl, 0) }
    }

    fn input(api: &sys::Api, label: &str, value: &mut f32) -> bool {
        let ((l, ln), (f, fl)) = (s(label), s(Self::DEFAULT_FORMAT));
        unsafe { (api.mqemu_imgui_input_float)(l, ln, value, 0.0, 0.0, f, fl, 0) }
    }
}

impl Number for i32 {
    const DEFAULT_FORMAT: &'static str = "%d";

    fn format_ok(format: &str) -> bool {
        printf_conversions(format, b"diuxXo").is_some_and(|n| n <= 1)
    }

    fn slider(api: &sys::Api, label: &str, value: &mut i32, min: i32, max: i32, format: &str) -> bool {
        let ((l, ln), (f, fl)) = (s(label), s(format));
        unsafe { (api.mqemu_imgui_slider_int)(l, ln, value, min, max, f, fl, 0) }
    }

    fn drag(api: &sys::Api, label: &str, value: &mut i32, speed: f32, min: i32, max: i32, format: &str) -> bool {
        let ((l, ln), (f, fl)) = (s(label), s(format));
        unsafe { (api.mqemu_imgui_drag_int)(l, ln, value, speed, min, max, f, fl, 0) }
    }

    fn input(api: &sys::Api, label: &str, value: &mut i32) -> bool {
        let (l, ln) = s(label);
        unsafe { (api.mqemu_imgui_input_int)(l, ln, value, 1, 100, 0) }
    }
}

/// Counts the printf conversions in `format`, or None if one isn't of the
/// allowed kinds (or the format is malformed). `%%` is a literal percent.
fn printf_conversions(format: &str, allowed: &[u8]) -> Option<usize> {
    let bytes = format.as_bytes();
    let (mut i, mut count) = (0, 0);
    while i < bytes.len() {
        if bytes[i] != b'%' {
            i += 1;
            continue;
        }
        i += 1;
        if bytes.get(i) == Some(&b'%') {
            i += 1;
            continue;
        }
        while bytes.get(i).is_some_and(|b| b"-+ #0'".contains(b)) {
            i += 1;
        }
        while bytes.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if bytes.get(i) == Some(&b'.') {
            i += 1;
            while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                i += 1;
            }
        }
        if !bytes.get(i).is_some_and(|b| allowed.contains(b)) {
            return None;
        }
        i += 1;
        count += 1;
    }
    Some(count)
}

fn check_format<T: Number>(format: &str) {
    assert!(
        T::format_ok(format),
        "ImGui display format {format:?} must contain at most one {} conversion (and no others)",
        std::any::type_name::<T>()
    );
}

/// Grows an input box's buffer (a `Vec<u8>` whose whole length is usable).
unsafe extern "C" fn grow(user: *mut c_void, size: usize, capacity: *mut usize) -> *mut c_char {
    let buffer = unsafe { &mut *user.cast::<Vec<u8>>() };
    if size > buffer.len() {
        buffer.resize(size, 0);
    }
    unsafe { *capacity = buffer.len() };
    buffer.as_mut_ptr().cast()
}

/// Lets ImGui edit `text` through `edit(buffer, capacity, user)`.
fn edit_text(text: &mut String, edit: impl FnOnce(*mut c_char, usize, *mut c_void) -> bool) -> bool {
    let mut buffer = std::mem::take(text).into_bytes();
    buffer.push(0);
    let vec: *mut Vec<u8> = &mut buffer;
    // SAFETY: the shim only uses the buffer, and `grow` the Vec, during this call.
    let changed = unsafe { edit((*vec).as_mut_ptr().cast(), (*vec).len(), vec.cast()) };
    let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
    buffer.truncate(end);
    *text = String::from_utf8(buffer).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    changed
}

impl Ui {
    /// Only the runtime makes one, for a frame.
    pub(crate) fn new(apis: &'static Apis) -> Ui {
        Ui { api: apis.imgui, plot: apis.implot, mq: apis.mq, _not_send: PhantomData }
    }

    // ----- windows -----

    /// A top-level window; configure it, then [`Window::show`] its contents.
    /// Its title is also its ID (see the module docs for `##`).
    pub fn window<'ui, 'a>(&'ui mut self, title: &'a str) -> Window<'ui, 'a> {
        Window { ui: self, title, open: None, flags: WindowFlags::NONE, pos: None, size: None, constraints: None, collapsed: None, dock: None }
    }

    /// A scrolling region inside the current window. `size` 0 on an axis
    /// fills the space left; negative leaves that much room.
    pub fn child<'ui, 'a>(&'ui mut self, id: &'a str) -> Child<'ui, 'a> {
        Child { ui: self, id, size: [0.0, 0.0], flags: ChildFlags::NONE, window_flags: WindowFlags::NONE }
    }

    /// True on the first frame a window is shown (after being hidden).
    pub fn is_window_appearing(&self) -> bool {
        unsafe { (self.api.mqemu_imgui_is_window_appearing)() }
    }

    /// Room left in the current window or cell, from the cursor, `[w, h]`.
    pub fn content_region_avail(&self) -> [f32; 2] {
        let (mut w, mut h) = (0.0, 0.0);
        unsafe { (self.api.mqemu_imgui_content_region_avail)(&mut w, &mut h) };
        [w, h]
    }

    // ----- text -----

    /// Text as is: no formatting, so `%` and `${...}` show literally.
    pub fn text(&mut self, text: impl AsRef<str>) {
        let (p, n) = s(text.as_ref());
        unsafe { (self.api.mqemu_imgui_text)(p, n) }
    }

    pub fn text_colored(&mut self, color: Color, text: impl AsRef<str>) {
        self.with_style_color(StyleColor::Text, color, |ui| ui.text(text));
    }

    /// Text in the style's disabled color.
    pub fn text_disabled(&mut self, text: impl AsRef<str>) {
        // ImGui's own TextDisabled is printf-formatted; this isn't.
        self.disabled(true, |ui| ui.text(text));
    }

    /// Text wrapped at the end of the window (or column).
    pub fn text_wrapped(&mut self, text: impl AsRef<str>) {
        unsafe { (self.api.mqemu_imgui_push_text_wrap_pos)(0.0) };
        let _pop = EndOnDrop(self.api.mqemu_imgui_pop_text_wrap_pos);
        self.text(text);
    }

    /// `text` on the left and `label` where a widget's label would be.
    pub fn label_text(&mut self, label: &str, text: impl AsRef<str>) {
        let ((l, ln), (t, tn)) = (s(label), s(text.as_ref()));
        unsafe { (self.api.mqemu_imgui_label_text)(l, ln, t, tn) }
    }

    /// A bullet point, then `text` on the same line.
    pub fn bullet_text(&mut self, text: impl AsRef<str>) {
        unsafe { (self.api.mqemu_imgui_bullet)() };
        self.text(text);
    }

    /// Text that looks and acts like a hyperlink; true when clicked.
    pub fn text_link(&mut self, label: &str) -> bool {
        let (l, ln) = s(label);
        unsafe { (self.api.mqemu_imgui_text_link)(l, ln) }
    }

    /// How big `text` would be, `[w, h]`.
    pub fn calc_text_size(&self, text: &str) -> [f32; 2] {
        let (p, n) = s(text);
        let (mut w, mut h) = (0.0, 0.0);
        unsafe { (self.api.mqemu_imgui_calc_text_size)(p, n, &mut w, &mut h) };
        [w, h]
    }

    // ----- layout -----

    pub fn separator(&mut self) {
        unsafe { (self.api.mqemu_imgui_separator)() }
    }

    /// A separator with a label in it.
    pub fn separator_text(&mut self, label: &str) {
        let (l, ln) = s(label);
        unsafe { (self.api.mqemu_imgui_separator_text)(l, ln) }
    }

    /// Puts the next item on the same line as the last one.
    pub fn same_line(&mut self) {
        unsafe { (self.api.mqemu_imgui_same_line)(0.0, -1.0) }
    }

    /// [`Ui::same_line`] at `offset_from_start_x` (0 for after the last
    /// item), `spacing` pixels after it (negative for the style's spacing).
    pub fn same_line_with(&mut self, offset_from_start_x: f32, spacing: f32) {
        unsafe { (self.api.mqemu_imgui_same_line)(offset_from_start_x, spacing) }
    }

    pub fn new_line(&mut self) {
        unsafe { (self.api.mqemu_imgui_new_line)() }
    }

    /// A little vertical space.
    pub fn spacing(&mut self) {
        unsafe { (self.api.mqemu_imgui_spacing)() }
    }

    /// Empty space of `size`, `[w, h]`.
    pub fn dummy(&mut self, size: [f32; 2]) {
        unsafe { (self.api.mqemu_imgui_dummy)(size[0], size[1]) }
    }

    /// Lines up text with framed widgets (buttons, inputs) on the same line.
    pub fn align_text_to_frame_padding(&mut self) {
        unsafe { (self.api.mqemu_imgui_align_text_to_frame_padding)() }
    }

    /// The next widget's width: pixels, or negative to leave that much room
    /// on the right (`-f32::MIN_POSITIVE` fills the width).
    pub fn set_next_item_width(&mut self, width: f32) {
        unsafe { (self.api.mqemu_imgui_set_next_item_width)(width) }
    }

    /// Draws `f` indented by the style's indent spacing.
    pub fn indented<R>(&mut self, f: impl FnOnce(&mut Ui) -> R) -> R {
        self.indented_by(0.0, f)
    }

    /// Draws `f` indented by `width` pixels (0 for the style's spacing).
    pub fn indented_by<R>(&mut self, width: f32, f: impl FnOnce(&mut Ui) -> R) -> R {
        unsafe { (self.api.mqemu_imgui_indent)(width) };
        let unindent = self.api.mqemu_imgui_unindent;
        let _unindent = OnDrop(move || unsafe { unindent(width) });
        f(self)
    }

    /// Draws `f` as a group: one item for layout (`same_line` after it) and hover checks.
    pub fn group<R>(&mut self, f: impl FnOnce(&mut Ui) -> R) -> R {
        unsafe { (self.api.mqemu_imgui_begin_group)() };
        let _end = EndOnDrop(self.api.mqemu_imgui_end_group);
        f(self)
    }

    // ----- ID, style and state stacks -----

    /// Draws `f` with `id` on the ID stack, so widgets with the same labels
    /// in different calls (rows of a list, say) are told apart.
    pub fn with_id<R>(&mut self, id: impl Id, f: impl FnOnce(&mut Ui) -> R) -> R {
        id.push(self.api);
        let _pop = EndOnDrop(self.api.mqemu_imgui_pop_id);
        f(self)
    }

    /// Draws `f` with a style color changed.
    pub fn with_style_color<R>(&mut self, which: StyleColor, color: Color, f: impl FnOnce(&mut Ui) -> R) -> R {
        unsafe { (self.api.mqemu_imgui_push_style_color)(which as i32, color[0], color[1], color[2], color[3]) };
        let pop = self.api.mqemu_imgui_pop_style_color;
        let _pop = OnDrop(move || unsafe { pop(1) });
        f(self)
    }

    /// Draws `f` with a style variable changed.
    pub fn with_style<R>(&mut self, var: StyleVar, f: impl FnOnce(&mut Ui) -> R) -> R {
        match var.raw() {
            (index, StyleValue::Float(v)) => unsafe { (self.api.mqemu_imgui_push_style_var_float)(index, v) },
            (index, StyleValue::Vec2([x, y])) => unsafe { (self.api.mqemu_imgui_push_style_var_vec2)(index, x, y) },
        }
        let pop = self.api.mqemu_imgui_pop_style_var;
        let _pop = OnDrop(move || unsafe { pop(1) });
        f(self)
    }

    /// Draws `f` greyed out and not interactive, if `disabled`.
    pub fn disabled<R>(&mut self, disabled: bool, f: impl FnOnce(&mut Ui) -> R) -> R {
        unsafe { (self.api.mqemu_imgui_begin_disabled)(disabled) };
        let _end = EndOnDrop(self.api.mqemu_imgui_end_disabled);
        f(self)
    }

    // ----- widgets -----

    /// True when clicked.
    pub fn button(&mut self, label: &str) -> bool {
        self.button_sized(label, [0.0, 0.0])
    }

    /// A button of `size`, `[w, h]` (0 on an axis fits the label).
    pub fn button_sized(&mut self, label: &str, size: [f32; 2]) -> bool {
        let (l, ln) = s(label);
        unsafe { (self.api.mqemu_imgui_button)(l, ln, size[0], size[1]) }
    }

    /// A button without frame padding, to sit within text.
    pub fn small_button(&mut self, label: &str) -> bool {
        let (l, ln) = s(label);
        unsafe { (self.api.mqemu_imgui_small_button)(l, ln) }
    }

    /// True when clicked, which flips `value`.
    pub fn checkbox(&mut self, label: &str, value: &mut bool) -> bool {
        let (l, ln) = s(label);
        unsafe { (self.api.mqemu_imgui_checkbox)(l, ln, value) }
    }

    /// A radio button shown selected if `active`; true when clicked.
    pub fn radio_button(&mut self, label: &str, active: bool) -> bool {
        let (l, ln) = s(label);
        unsafe { (self.api.mqemu_imgui_radio_button)(l, ln, active) }
    }

    /// A radio button for one `option` of `value`: selected when they're
    /// equal, and sets `value` to `option` when clicked.
    pub fn radio_value<T: PartialEq + Copy>(&mut self, label: &str, value: &mut T, option: T) -> bool {
        let clicked = self.radio_button(label, *value == option);
        if clicked {
            *value = option;
        }
        clicked
    }

    /// A line of text that highlights when hovered and can be selected; true when clicked.
    pub fn selectable(&mut self, label: &str, selected: bool) -> bool {
        self.selectable_with(label, selected, SelectableFlags::NONE, [0.0, 0.0])
    }

    pub fn selectable_with(&mut self, label: &str, selected: bool, flags: SelectableFlags, size: [f32; 2]) -> bool {
        let (l, ln) = s(label);
        unsafe { (self.api.mqemu_imgui_selectable)(l, ln, selected, flags.bits(), size[0], size[1]) }
    }

    /// A slider between the range's ends (ctrl+click to type a value); true when changed.
    pub fn slider<T: Number>(&mut self, label: &str, value: &mut T, range: RangeInclusive<T>) -> bool {
        T::slider(self.api, label, value, *range.start(), *range.end(), T::DEFAULT_FORMAT)
    }

    /// [`Ui::slider`] showing the value with a printf `format` such as
    /// `"%.1f ms"`: at most one conversion, of the value's type.
    ///
    /// # Panics
    /// If `format` has another conversion, which would make printf read garbage.
    pub fn slider_format<T: Number>(&mut self, label: &str, value: &mut T, range: RangeInclusive<T>, format: &str) -> bool {
        check_format::<T>(format);
        T::slider(self.api, label, value, *range.start(), *range.end(), format)
    }

    /// A value changed by dragging; true when changed. Unbounded.
    pub fn drag<T: Number + Default>(&mut self, label: &str, value: &mut T, speed: f32) -> bool {
        // ImGui treats min == max as no bounds.
        T::drag(self.api, label, value, speed, T::default(), T::default(), T::DEFAULT_FORMAT)
    }

    /// [`Ui::drag`] within a range, with an optional printf `format` (as for [`Ui::slider_format`]).
    ///
    /// # Panics
    /// If `format` has another conversion.
    pub fn drag_range<T: Number>(&mut self, label: &str, value: &mut T, speed: f32, range: RangeInclusive<T>, format: Option<&str>) -> bool {
        let format = format.unwrap_or(T::DEFAULT_FORMAT);
        check_format::<T>(format);
        T::drag(self.api, label, value, speed, *range.start(), *range.end(), format)
    }

    /// A box to type a number into, with +/- buttons for integers; true when changed.
    pub fn input_number<T: Number>(&mut self, label: &str, value: &mut T) -> bool {
        T::input(self.api, label, value)
    }

    /// A one-line text box editing `text`; true when it changed.
    pub fn input_text(&mut self, label: &str, text: &mut String) -> bool {
        self.input_text_with(label, text, InputTextFlags::NONE)
    }

    pub fn input_text_with(&mut self, label: &str, text: &mut String, flags: InputTextFlags) -> bool {
        let (l, ln) = s(label);
        edit_text(text, |buffer, capacity, user| unsafe {
            (self.api.mqemu_imgui_input_text)(l, ln, buffer, capacity, flags.bits(), grow, user)
        })
    }

    /// A multi-line text box of `size`, `[w, h]` (0 for a default).
    pub fn input_text_multiline(&mut self, label: &str, text: &mut String, size: [f32; 2]) -> bool {
        let (l, ln) = s(label);
        edit_text(text, |buffer, capacity, user| unsafe {
            (self.api.mqemu_imgui_input_text_multiline)(l, ln, buffer, capacity, size[0], size[1], 0, grow, user)
        })
    }

    /// A color swatch and editor; true when changed.
    pub fn color_edit(&mut self, label: &str, color: &mut Color) -> bool {
        let (l, ln) = s(label);
        unsafe { (self.api.mqemu_imgui_color_edit4)(l, ln, color.as_mut_ptr(), 0) }
    }

    /// A bar `fraction` (0.0 to 1.0) full, as wide as the window, showing the percentage.
    pub fn progress_bar(&mut self, fraction: f32) {
        self.progress_bar_with(fraction, [-f32::MIN_POSITIVE, 0.0], None);
    }

    /// [`Ui::progress_bar`] of `size` (`[w, h]`, 0 for defaults), showing
    /// `overlay` at the end of the fill: None shows the percentage, `Some("")` nothing.
    pub fn progress_bar_with(&mut self, fraction: f32, size: [f32; 2], overlay: Option<&str>) {
        let (o, on) = opt(overlay);
        unsafe { (self.api.mqemu_imgui_progress_bar)(fraction, size[0], size[1], o, on) }
    }

    /// A line graph of `values`, scaled to fit them.
    pub fn plot_lines(&mut self, label: &str, values: &[f32]) {
        self.simple_plot(label, values, PlotOptions::default(), false);
    }

    /// [`Ui::plot_lines`] with a scale, size and overlay text.
    pub fn plot_lines_with(&mut self, label: &str, values: &[f32], options: PlotOptions<'_>) {
        self.simple_plot(label, values, options, false);
    }

    /// A bar graph of `values`, scaled to fit them.
    pub fn plot_histogram(&mut self, label: &str, values: &[f32]) {
        self.simple_plot(label, values, PlotOptions::default(), true);
    }

    pub fn plot_histogram_with(&mut self, label: &str, values: &[f32], options: PlotOptions<'_>) {
        self.simple_plot(label, values, options, true);
    }

    fn simple_plot(&mut self, label: &str, values: &[f32], options: PlotOptions<'_>, histogram: bool) {
        let ((l, ln), (o, on)) = (s(label), opt(options.overlay));
        let count = i32::try_from(values.len()).unwrap_or(i32::MAX);
        let (min, max) = options.scale.map_or((f32::MAX, f32::MAX), |r| (*r.start(), *r.end()));
        let [w, h] = options.size;
        unsafe { (self.api.mqemu_imgui_plot_lines)(l, ln, values.as_ptr(), count, 0, o, on, min, max, w, h, histogram) }
    }

    // ----- combo boxes -----

    /// A drop-down showing `preview`; while open, `f` draws its contents
    /// (usually [`Ui::selectable`]s). Returns `f`'s result if it was open.
    pub fn combo<R>(&mut self, label: &str, preview: &str, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let ((l, ln), (p, pn)) = (s(label), s(preview));
        if !unsafe { (self.api.mqemu_imgui_begin_combo)(l, ln, p, pn, 0) } {
            return None;
        }
        let _end = EndOnDrop(self.api.mqemu_imgui_end_combo);
        Some(f(self))
    }

    /// A drop-down choosing one of `items` by index; true when the choice changed.
    pub fn combo_list<S: AsRef<str>>(&mut self, label: &str, current: &mut usize, items: &[S]) -> bool {
        let preview = items.get(*current).map_or("", AsRef::as_ref);
        self.combo(label, preview, |ui| {
            let mut changed = false;
            for (index, item) in items.iter().enumerate() {
                let selected = index == *current;
                if ui.with_id(index, |ui| ui.selectable(item.as_ref(), selected)) && !selected {
                    *current = index;
                    changed = true;
                }
                if selected {
                    unsafe { (self.api.mqemu_imgui_set_item_default_focus)() };
                }
            }
            changed
        })
        .unwrap_or(false)
    }

    // ----- trees -----

    /// An expandable node; `f` draws its children, indented, while it's open.
    pub fn tree_node<R>(&mut self, label: &str, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        self.tree_node_with(label, TreeNodeFlags::NONE, f)
    }

    pub fn tree_node_with<R>(&mut self, label: &str, flags: TreeNodeFlags, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let (l, ln) = s(label);
        if !unsafe { (self.api.mqemu_imgui_tree_node)(l, ln, flags.bits()) } {
            return None;
        }
        let _pop = EndOnDrop(self.api.mqemu_imgui_tree_pop);
        Some(f(self))
    }

    /// A full-width header; true while open. Draw the section's contents
    /// after it when it is (they aren't indented).
    pub fn collapsing_header(&mut self, label: &str) -> bool {
        self.collapsing_header_with(label, TreeNodeFlags::NONE)
    }

    pub fn collapsing_header_with(&mut self, label: &str, flags: TreeNodeFlags) -> bool {
        let (l, ln) = s(label);
        unsafe { (self.api.mqemu_imgui_collapsing_header)(l, ln, flags.bits()) }
    }

    /// Opens or closes the next tree node or collapsing header.
    pub fn set_next_item_open(&mut self, open: bool, cond: Cond) {
        unsafe { (self.api.mqemu_imgui_set_next_item_open)(open, cond as i32) }
    }

    // ----- tabs -----

    /// A row of tabs; `f` adds them with [`TabBar::tab`].
    pub fn tab_bar<R>(&mut self, id: &str, f: impl FnOnce(&mut TabBar<'_>) -> R) -> Option<R> {
        self.tab_bar_with(id, TabBarFlags::NONE, f)
    }

    pub fn tab_bar_with<R>(&mut self, id: &str, flags: TabBarFlags, f: impl FnOnce(&mut TabBar<'_>) -> R) -> Option<R> {
        let (i, n) = s(id);
        if !unsafe { (self.api.mqemu_imgui_begin_tab_bar)(i, n, flags.bits()) } {
            return None;
        }
        let _end = EndOnDrop(self.api.mqemu_imgui_end_tab_bar);
        Some(f(&mut TabBar { ui: self }))
    }

    // ----- tables -----

    /// A table of `columns` columns (1 to 512); configure it, then
    /// [`TableBuilder::show`] its rows.
    ///
    /// # Panics
    /// If `columns` is out of range.
    pub fn table<'ui, 'a>(&'ui mut self, id: &'a str, columns: usize) -> TableBuilder<'ui, 'a> {
        assert!((1..=MAX_TABLE_COLUMNS).contains(&columns), "an ImGui table has 1 to {MAX_TABLE_COLUMNS} columns, not {columns}");
        TableBuilder { ui: self, id, columns, flags: TableFlags::NONE, outer_size: [0.0, 0.0], inner_width: 0.0 }
    }

    // ----- tooltips and popups -----

    /// Whether the mouse is over the last item.
    pub fn is_item_hovered(&self) -> bool {
        unsafe { (self.api.mqemu_imgui_is_item_hovered)(0) }
    }

    /// Whether the last item was just clicked (left button).
    pub fn is_item_clicked(&self) -> bool {
        unsafe { (self.api.mqemu_imgui_is_item_clicked)(0) }
    }

    /// Whether the last item is active (a button held down, a box being edited).
    pub fn is_item_active(&self) -> bool {
        unsafe { (self.api.mqemu_imgui_is_item_active)() }
    }

    /// Shows `text` as a tooltip for the last item, when it's hovered.
    pub fn item_tooltip_text(&mut self, text: impl AsRef<str>) {
        if unsafe { (self.api.mqemu_imgui_is_item_hovered)(sys::ImGuiHoveredFlags_ForTooltip) } {
            let (t, n) = s(text.as_ref());
            unsafe { (self.api.mqemu_imgui_set_tooltip)(t, n) }
        }
    }

    /// Draws a tooltip for the last item with `f`, when it's hovered.
    pub fn item_tooltip<R>(&mut self, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        if !unsafe { (self.api.mqemu_imgui_begin_item_tooltip)() } {
            return None;
        }
        let _end = EndOnDrop(self.api.mqemu_imgui_end_tooltip);
        Some(f(self))
    }

    /// Draws a tooltip by the mouse with `f`, this frame.
    pub fn tooltip<R>(&mut self, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        if !unsafe { (self.api.mqemu_imgui_begin_tooltip)() } {
            return None;
        }
        let _end = EndOnDrop(self.api.mqemu_imgui_end_tooltip);
        Some(f(self))
    }

    /// Opens the popup `id` (call once, e.g. on a click; draw it with [`Ui::popup`] every frame).
    pub fn open_popup(&mut self, id: &str) {
        let (i, n) = s(id);
        unsafe { (self.api.mqemu_imgui_open_popup)(i, n, 0) }
    }

    /// Draws the popup `id` with `f` while it's open. The same ID stack as
    /// [`Ui::open_popup`]'s call applies.
    pub fn popup<R>(&mut self, id: &str, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let (i, n) = s(id);
        if !unsafe { (self.api.mqemu_imgui_begin_popup)(i, n, 0) } {
            return None;
        }
        let _end = EndOnDrop(self.api.mqemu_imgui_end_popup);
        Some(f(self))
    }

    /// Closes the popup being drawn; nothing outside one.
    pub fn close_current_popup(&mut self) {
        unsafe { (self.api.mqemu_imgui_close_current_popup)() }
    }

    // ----- menus -----

    /// Draws the current window's menu bar with `f` (the window needs
    /// [`WindowFlags::MENU_BAR`]; without it, nothing happens).
    pub fn menu_bar<R>(&mut self, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        if !unsafe { (self.api.mqemu_imgui_begin_menu_bar)() } {
            return None;
        }
        let _end = EndOnDrop(self.api.mqemu_imgui_end_menu_bar);
        Some(f(self))
    }

    /// A menu (in a menu bar, or a submenu in another menu); `f` draws its items while open.
    pub fn menu<R>(&mut self, label: &str, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let (l, ln) = s(label);
        if !unsafe { (self.api.mqemu_imgui_begin_menu)(l, ln, true) } {
            return None;
        }
        let _end = EndOnDrop(self.api.mqemu_imgui_end_menu);
        Some(f(self))
    }

    /// A menu entry; true when chosen.
    pub fn menu_item(&mut self, label: &str) -> bool {
        self.menu_item_with(label, None, false, true)
    }

    /// A menu entry showing `shortcut` text (it doesn't bind a key), a check
    /// mark if `selected`, greyed out unless `enabled`.
    pub fn menu_item_with(&mut self, label: &str, shortcut: Option<&str>, selected: bool, enabled: bool) -> bool {
        let ((l, ln), (k, kn)) = (s(label), opt(shortcut));
        unsafe { (self.api.mqemu_imgui_menu_item)(l, ln, k, kn, selected, enabled) }
    }

    // ----- items, style and drawing -----

    /// The last item's rectangle in screen coordinates, `(min, max)`.
    pub fn item_rect(&self) -> ([f32; 2], [f32; 2]) {
        let (mut x0, mut y0, mut x1, mut y1) = (0.0, 0.0, 0.0, 0.0);
        unsafe { (self.api.mqemu_imgui_item_rect)(&mut x0, &mut y0, &mut x1, &mut y1) };
        ([x0, y0], [x1, y1])
    }

    /// A color of the current style (MacroQuest's theme, plus any
    /// [`Ui::with_style_color`] in effect).
    pub fn style_color(&self, which: StyleColor) -> Color {
        let mut color = [0.0; 4];
        unsafe { (self.api.mqemu_imgui_style_color)(which as i32, color.as_mut_ptr()) };
        color
    }

    /// Draws shapes and text on the current window, in screen coordinates,
    /// without affecting layout: e.g. a label over the last item, at
    /// [`Ui::item_rect`]. Borrows the `Ui`, so the window can't change while
    /// it's held.
    pub fn draw_list(&self) -> DrawList<'_> {
        DrawList::new(self, unsafe { (self.api.mqemu_imgui_window_draw_list)() })
    }

    /// Draws over every window (anywhere on the screen).
    pub fn foreground_draw_list(&self) -> DrawList<'_> {
        DrawList::new(self, unsafe { (self.api.mqemu_imgui_foreground_draw_list)() })
    }

    /// Draws under every window, over the game: a HUD, say.
    pub fn background_draw_list(&self) -> DrawList<'_> {
        DrawList::new(self, unsafe { (self.api.mqemu_imgui_background_draw_list)() })
    }

    // ----- cursor, mouse and window geometry -----

    /// The current font's line height.
    pub fn text_line_height(&self) -> f32 {
        unsafe { (self.api.mqemu_imgui_text_line_height)(false) }
    }

    /// From one line of text to the next: the line height plus the spacing
    /// between items.
    pub fn text_line_height_with_spacing(&self) -> f32 {
        unsafe { (self.api.mqemu_imgui_text_line_height)(true) }
    }

    /// A framed widget's height (a button, an input).
    pub fn frame_height(&self) -> f32 {
        unsafe { (self.api.mqemu_imgui_frame_height)(false) }
    }

    /// From one framed widget to the next below it.
    pub fn frame_height_with_spacing(&self) -> f32 {
        unsafe { (self.api.mqemu_imgui_frame_height)(true) }
    }

    /// Where the next item goes, in screen coordinates.
    pub fn cursor_screen_pos(&self) -> Vec2 {
        let (mut x, mut y) = (0.0, 0.0);
        unsafe { (self.api.mqemu_imgui_cursor_screen_pos)(&mut x, &mut y) };
        [x, y]
    }

    /// Moves where the next item goes, in screen coordinates.
    pub fn set_cursor_screen_pos(&mut self, pos: Vec2) {
        unsafe { (self.api.mqemu_imgui_set_cursor_screen_pos)(pos[0], pos[1]) }
    }

    /// The current window's top left, on the screen.
    pub fn window_pos(&self) -> Vec2 {
        let (mut x, mut y) = (0.0, 0.0);
        unsafe { (self.api.mqemu_imgui_window_pos)(&mut x, &mut y) };
        [x, y]
    }

    /// The current window's size, `[w, h]`.
    pub fn window_size(&self) -> Vec2 {
        let (mut w, mut h) = (0.0, 0.0);
        unsafe { (self.api.mqemu_imgui_window_size)(&mut w, &mut h) };
        [w, h]
    }

    /// The part of the game's window ImGui can use: top left and size.
    pub fn screen_rect(&self) -> (Vec2, Vec2) {
        let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
        unsafe { (self.api.mqemu_imgui_main_viewport_rect)(&mut x, &mut y, &mut w, &mut h) };
        ([x, y], [w, h])
    }

    pub fn mouse_pos(&self) -> Vec2 {
        let (mut x, mut y) = (0.0, 0.0);
        unsafe { (self.api.mqemu_imgui_mouse_pos)(&mut x, &mut y) };
        [x, y]
    }

    pub fn is_mouse_down(&self, button: MouseButton) -> bool {
        unsafe { (self.api.mqemu_imgui_is_mouse_down)(button as i32) }
    }

    /// Whether `button` went down this frame.
    pub fn is_mouse_clicked(&self, button: MouseButton) -> bool {
        unsafe { (self.api.mqemu_imgui_is_mouse_clicked)(button as i32) }
    }

    /// Whether the mouse is in the rectangle (and the current clip rectangle).
    pub fn is_mouse_hovering_rect(&self, min: Vec2, max: Vec2) -> bool {
        unsafe { (self.api.mqemu_imgui_is_mouse_hovering_rect)(min[0], min[1], max[0], max[1]) }
    }

    /// An item of `size` that shows nothing but can be clicked, hovered and
    /// dragged: the hit area of something drawn with [`Ui::draw_list`].
    /// True when clicked. A negative size fills the space left but that much.
    ///
    /// # Panics
    /// If either size is 0.
    pub fn invisible_button(&mut self, id: &str, size: Vec2) -> bool {
        self.invisible_button_with(id, size, ButtonFlags::NONE)
    }

    /// # Panics
    /// If either size is 0.
    pub fn invisible_button_with(&mut self, id: &str, size: Vec2, flags: ButtonFlags) -> bool {
        assert!(size[0] != 0.0 && size[1] != 0.0, "an invisible button can't be 0 wide or tall: {size:?}");
        let (i, n) = s(id);
        unsafe { (self.api.mqemu_imgui_invisible_button)(i, n, size[0], size[1], flags.bits()) }
    }

    // ----- docking -----

    /// Whether docking is on (a MacroQuest overlay setting). Without it,
    /// dock spaces do nothing.
    pub fn is_docking_enabled(&self) -> bool {
        unsafe { (self.api.mqemu_imgui_docking_enabled)() }
    }

    /// The dock ID `name` gets here (it depends on the ID stack, like a
    /// widget's ID).
    pub fn dock_id(&self, name: &str) -> DockId {
        let (p, n) = s(name);
        DockId(unsafe { (self.api.mqemu_imgui_get_id)(p, n) })
    }

    /// A dock space in the current window: an area other windows can be
    /// docked into ([`Window::dock`] puts one there). A `size` of 0 fills the
    /// space left. None if docking is off.
    pub fn dock_space(&mut self, id: DockId, size: Vec2, flags: DockNodeFlags) -> Option<DockId> {
        let id = unsafe { (self.api.mqemu_imgui_dock_space)(id.0, size[0], size[1], flags.bits()) };
        (id != 0).then_some(DockId(id))
    }

    /// A dock space covering the whole screen, behind every window (with
    /// [`DockNodeFlags::PASSTHRU_CENTRAL_NODE`], clicks go through where
    /// nothing is docked). `id` None picks one. None if docking is off.
    pub fn dock_space_over_screen(&mut self, id: Option<DockId>, flags: DockNodeFlags) -> Option<DockId> {
        let id = unsafe { (self.api.mqemu_imgui_dock_space_over_viewport)(id.map_or(0, |id| id.0), flags.bits()) };
        (id != 0).then_some(DockId(id))
    }

    /// The dock node the current window is docked in, if any.
    pub fn window_dock_id(&self) -> Option<DockId> {
        let id = unsafe { (self.api.mqemu_imgui_window_dock_id)() };
        (id != 0).then_some(DockId(id))
    }

    pub fn is_window_docked(&self) -> bool {
        unsafe { (self.api.mqemu_imgui_is_window_docked)() }
    }

    // ----- frame info -----

    /// ImGui's frame counter.
    pub fn frame_count(&self) -> i32 {
        unsafe { (self.api.mqemu_imgui_frame_count)() }
    }

    /// Seconds since ImGui started.
    pub fn time(&self) -> f64 {
        unsafe { (self.api.mqemu_imgui_time)() }
    }

    /// ImGui's estimate of frames per second.
    pub fn framerate(&self) -> f32 {
        unsafe { (self.api.mqemu_imgui_framerate)() }
    }

    /// Seconds since the last frame.
    pub fn delta_time(&self) -> f32 {
        unsafe { (self.api.mqemu_imgui_delta_time)() }
    }
}

/// Options for [`Ui::plot_lines_with`] and [`Ui::plot_histogram_with`], ImGui's
/// simple graphs ([`Ui::plot`] draws ImPlot's).
#[derive(Debug, Clone, Default)]
pub struct PlotOptions<'a> {
    /// The values at the bottom and top; None fits the data.
    pub scale: Option<RangeInclusive<f32>>,
    /// `[w, h]`; 0 for defaults.
    pub size: [f32; 2],
    /// Text drawn over the graph.
    pub overlay: Option<&'a str>,
}

/// A window being configured; see [`Ui::window`].
#[must_use = "a window is drawn by calling show"]
pub struct Window<'ui, 'a> {
    ui: &'ui mut Ui,
    title: &'a str,
    open: Option<&'a mut bool>,
    flags: WindowFlags,
    pos: Option<([f32; 2], Cond, [f32; 2])>,
    size: Option<([f32; 2], Cond)>,
    constraints: Option<([f32; 2], [f32; 2])>,
    collapsed: Option<(bool, Cond)>,
    dock: Option<(DockId, Cond)>,
}

impl<'a> Window<'_, 'a> {
    /// Gives the window a close button, which sets `open` to false. While
    /// it's false, the window isn't drawn.
    pub fn open(mut self, open: &'a mut bool) -> Self {
        self.open = Some(open);
        self
    }

    pub fn flags(mut self, flags: WindowFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Screen position of the top-left corner, `[x, y]`.
    pub fn position(mut self, pos: [f32; 2], cond: Cond) -> Self {
        self.pos = Some((pos, cond, [0.0, 0.0]));
        self
    }

    /// Screen position of the point `pivot` (`[0.5, 0.5]` for the center) of the window.
    pub fn position_pivot(mut self, pos: [f32; 2], cond: Cond, pivot: [f32; 2]) -> Self {
        self.pos = Some((pos, cond, pivot));
        self
    }

    /// `[w, h]`; 0 on an axis fits the contents.
    pub fn size(mut self, size: [f32; 2], cond: Cond) -> Self {
        self.size = Some((size, cond));
        self
    }

    /// Limits resizing; use `f32::MAX` for no upper limit.
    pub fn size_constraints(mut self, min: [f32; 2], max: [f32; 2]) -> Self {
        self.constraints = Some((min, max));
        self
    }

    pub fn collapsed(mut self, collapsed: bool, cond: Cond) -> Self {
        self.collapsed = Some((collapsed, cond));
        self
    }

    /// Docks the window into the dock space or node `id` ([`Ui::dock_space`]).
    pub fn dock(mut self, id: DockId, cond: Cond) -> Self {
        self.dock = Some((id, cond));
        self
    }

    /// Draws the window, with `f` drawing its contents unless it's collapsed
    /// or scrolled off. Returns `f`'s result if it ran.
    pub fn show<R>(self, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        if self.open.as_deref() == Some(&false) {
            return None;
        }
        unsafe {
            if let Some(([x, y], cond, [px, py])) = self.pos {
                (self.ui.api.mqemu_imgui_set_next_window_pos)(x, y, cond as i32, px, py);
            }
            if let Some(([w, h], cond)) = self.size {
                (self.ui.api.mqemu_imgui_set_next_window_size)(w, h, cond as i32);
            }
            if let Some(([min_w, min_h], [max_w, max_h])) = self.constraints {
                (self.ui.api.mqemu_imgui_set_next_window_size_constraints)(min_w, min_h, max_w, max_h);
            }
            if let Some((collapsed, cond)) = self.collapsed {
                (self.ui.api.mqemu_imgui_set_next_window_collapsed)(collapsed, cond as i32);
            }
            if let Some((id, cond)) = self.dock {
                (self.ui.api.mqemu_imgui_set_next_window_dock_id)(id.0, cond as i32);
            }
        }
        let (t, n) = s(self.title);
        let open = self.open.map_or(std::ptr::null_mut(), |open| open as *mut bool);
        // End is called whatever Begin returns.
        let visible = unsafe { (self.ui.api.mqemu_imgui_begin)(t, n, open, self.flags.bits()) };
        let _end = EndOnDrop(self.ui.api.mqemu_imgui_end);
        visible.then(|| f(self.ui))
    }
}

/// A child region being configured; see [`Ui::child`].
#[must_use = "a child region is drawn by calling show"]
pub struct Child<'ui, 'a> {
    ui: &'ui mut Ui,
    id: &'a str,
    size: [f32; 2],
    flags: ChildFlags,
    window_flags: WindowFlags,
}

impl Child<'_, '_> {
    /// `[w, h]`: 0 fills the space left on that axis, negative leaves that much.
    pub fn size(mut self, size: [f32; 2]) -> Self {
        self.size = size;
        self
    }

    pub fn flags(mut self, flags: ChildFlags) -> Self {
        self.flags = flags;
        self
    }

    pub fn window_flags(mut self, flags: WindowFlags) -> Self {
        self.window_flags = flags;
        self
    }

    /// Draws the region, with `f` drawing its contents if any of it is visible.
    pub fn show<R>(self, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let (i, n) = s(self.id);
        let visible = unsafe { (self.ui.api.mqemu_imgui_begin_child)(i, n, self.size[0], self.size[1], self.flags.bits(), self.window_flags.bits()) };
        // EndChild is called whatever BeginChild returns.
        let _end = EndOnDrop(self.ui.api.mqemu_imgui_end_child);
        visible.then(|| f(self.ui))
    }
}

/// The tabs of a [`Ui::tab_bar`].
pub struct TabBar<'ui> {
    ui: &'ui mut Ui,
}

impl TabBar<'_> {
    /// A tab; `f` draws its contents while it's the selected one.
    pub fn tab<R>(&mut self, label: &str, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        self.tab_with(label, None, TabItemFlags::NONE, f)
    }

    /// A tab with a close button if `open` is given (which sets it to false;
    /// while it's false, the tab isn't shown).
    pub fn tab_with<R>(&mut self, label: &str, open: Option<&mut bool>, flags: TabItemFlags, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        if open.as_deref() == Some(&false) {
            return None;
        }
        let (l, ln) = s(label);
        let open = open.map_or(std::ptr::null_mut(), |open| open as *mut bool);
        if !unsafe { (self.ui.api.mqemu_imgui_begin_tab_item)(l, ln, open, flags.bits()) } {
            return None;
        }
        let _end = EndOnDrop(self.ui.api.mqemu_imgui_end_tab_item);
        Some(f(self.ui))
    }
}

/// Most columns an ImGui table may have.
pub const MAX_TABLE_COLUMNS: usize = 512;

/// A table being configured; see [`Ui::table`].
#[must_use = "a table is drawn by calling show"]
pub struct TableBuilder<'ui, 'a> {
    ui: &'ui mut Ui,
    id: &'a str,
    columns: usize,
    flags: TableFlags,
    outer_size: [f32; 2],
    inner_width: f32,
}

impl TableBuilder<'_, '_> {
    pub fn flags(mut self, flags: TableFlags) -> Self {
        self.flags = flags;
        self
    }

    /// `[w, h]` of the table: needed for a height with [`TableFlags::SCROLL_Y`].
    pub fn outer_size(mut self, size: [f32; 2]) -> Self {
        self.outer_size = size;
        self
    }

    /// The scrolled width with [`TableFlags::SCROLL_X`].
    pub fn inner_width(mut self, width: f32) -> Self {
        self.inner_width = width;
        self
    }

    /// Draws the table, with `f` setting up columns and drawing rows through
    /// [`Table`] (which also draws like a [`Ui`] into the current cell), if
    /// it's visible.
    pub fn show<R>(self, f: impl FnOnce(&mut Table<'_>) -> R) -> Option<R> {
        let (i, n) = s(self.id);
        let [w, h] = self.outer_size;
        // The column count was checked in Ui::table.
        if !unsafe { (self.ui.api.mqemu_imgui_begin_table)(i, n, self.columns as i32, self.flags.bits(), w, h, self.inner_width) } {
            return None;
        }
        let _end = EndOnDrop(self.ui.api.mqemu_imgui_end_table);
        Some(f(&mut Table { ui: self.ui, columns: self.columns, set_up: 0, rows_started: false }))
    }
}

/// The current table, inside [`TableBuilder::show`]. Draws into the current
/// cell like a [`Ui`] (through `Deref`).
pub struct Table<'ui> {
    ui: &'ui mut Ui,
    columns: usize,
    /// Columns set up so far.
    set_up: usize,
    /// Whether a row or the headers have been drawn; setup must come first.
    rows_started: bool,
}

impl Table<'_> {
    /// Names the next column (for [`Table::headers_row`]).
    ///
    /// # Panics
    /// After the first row, or for more columns than the table has.
    pub fn setup_column(&mut self, label: &str) {
        self.setup_column_with(label, TableColumnFlags::NONE, 0.0);
    }

    /// [`Table::setup_column`] with flags and an initial width (fixed
    /// columns) or weight (stretched ones); 0 for automatic.
    ///
    /// # Panics
    /// As [`Table::setup_column`].
    pub fn setup_column_with(&mut self, label: &str, flags: TableColumnFlags, width_or_weight: f32) {
        assert!(!self.rows_started, "ImGui table columns must be set up before the first row");
        assert!(self.set_up < self.columns, "this ImGui table has only {} columns", self.columns);
        self.set_up += 1;
        let (l, ln) = s(label);
        unsafe { (self.ui.api.mqemu_imgui_table_setup_column)(l, ln, flags.bits(), width_or_weight) }
    }

    /// Keeps the first `columns` columns and `rows` rows in view when scrolling.
    ///
    /// # Panics
    /// After the first row.
    pub fn setup_scroll_freeze(&mut self, columns: usize, rows: usize) {
        assert!(!self.rows_started, "ImGui table scroll freezing must be set up before the first row");
        let clamp = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        unsafe { (self.ui.api.mqemu_imgui_table_setup_scroll_freeze)(clamp(columns.min(self.columns)), clamp(rows)) }
    }

    /// A row of column headers, from [`Table::setup_column`]'s labels.
    pub fn headers_row(&mut self) {
        self.rows_started = true;
        unsafe { (self.ui.api.mqemu_imgui_table_headers_row)() }
    }

    /// Starts a new row.
    pub fn next_row(&mut self) {
        self.next_row_with(0.0);
    }

    /// Starts a new row at least `min_height` pixels tall.
    pub fn next_row_with(&mut self, min_height: f32) {
        self.rows_started = true;
        unsafe { (self.ui.api.mqemu_imgui_table_next_row)(0, min_height) }
    }

    /// Moves to the next cell (the first of a new row after the last one).
    /// False if that column is hidden or clipped; drawing there is harmless.
    pub fn next_column(&mut self) -> bool {
        self.rows_started = true;
        unsafe { (self.ui.api.mqemu_imgui_table_next_column)() }
    }

    /// Moves to column `index` of the current row. False if it's hidden,
    /// clipped, or out of range (then nothing moves).
    pub fn set_column(&mut self, index: usize) -> bool {
        if index >= self.columns {
            return false;
        }
        self.rows_started = true;
        unsafe { (self.ui.api.mqemu_imgui_table_set_column_index)(index as i32) }
    }

    /// Fills a whole row: one cell per value.
    pub fn row<S: AsRef<str>>(&mut self, cells: &[S]) {
        self.next_row();
        for cell in cells {
            self.next_column();
            self.text(cell);
        }
    }
}

impl Deref for Table<'_> {
    type Target = Ui;
    fn deref(&self) -> &Ui {
        self.ui
    }
}

impl DerefMut for Table<'_> {
    fn deref_mut(&mut self) -> &mut Ui {
        self.ui
    }
}

#[cfg(test)]
pub(crate) mod fake;
#[cfg(test)]
mod tests;
