//! Plots with ImPlot, which MacroQuest builds into its ImGui.
//!
//! ```no_run
//! # fn draw(ui: &mut mq_emu::Ui, hp: &[f32], mana: &[f32]) {
//! use mq_emu::imgui::{Axis, Cond};
//! ui.plot("Health").size([-1.0, 200.0]).show(|plot| {
//!     plot.setup_axes(Some("seconds"), Some("%"));
//!     plot.setup_axis_limits(Axis::Y1, 0.0..=100.0, Cond::Always);
//!     plot.line("HP", hp);
//!     plot.line("Mana", mana);
//! });
//! # }
//! ```
//!
//! Like ImGui's scopes, a plot is a closure: it always ends, and setup calls
//! ([`Plot::setup_axis`] and the like) panic once something has been
//! plotted or queried, where ImPlot no longer takes them.

use std::ffi::{c_char, c_void};
use std::ops::RangeInclusive;

use mq_emu_sys::imgui as imgui_sys;
use mq_emu_sys::implot as sys;

use super::dnd::{self, DragDropFlags, DragSource, DropTarget};
use super::draw::{DrawList, Vec2};
use super::{Color, Cond, EndOnDrop, OnDrop, TextureId, Ui, opt, printf_conversions, s};

macro_rules! plot_flags {
    ($(#[$meta:meta])* $name:ident { $($(#[$flag_meta:meta])* $flag:ident = $value:ident,)* }) => {
        flags! {
            $(#[$meta])*
            $name {
                $($(#[$flag_meta])* $flag = $value,)*
            }
            in sys
        }
    };
}

plot_flags! {
    /// Options for [`Ui::plot`] (`ImPlotFlags_`).
    PlotFlags {
        NO_TITLE = ImPlotFlags_NoTitle,
        NO_LEGEND = ImPlotFlags_NoLegend,
        /// No mouse position shown in the plot.
        NO_MOUSE_TEXT = ImPlotFlags_NoMouseText,
        NO_INPUTS = ImPlotFlags_NoInputs,
        NO_MENUS = ImPlotFlags_NoMenus,
        NO_BOX_SELECT = ImPlotFlags_NoBoxSelect,
        NO_FRAME = ImPlotFlags_NoFrame,
        /// Equal units per pixel on both axes.
        EQUAL = ImPlotFlags_Equal,
        CROSSHAIRS = ImPlotFlags_Crosshairs,
        /// Only the plot area: no title, legend, menus, box selection or mouse text.
        CANVAS_ONLY = ImPlotFlags_CanvasOnly,
    }
}

plot_flags! {
    /// Options for [`Plot::setup_axis`] (`ImPlotAxisFlags_`).
    AxisFlags {
        NO_LABEL = ImPlotAxisFlags_NoLabel,
        NO_GRID_LINES = ImPlotAxisFlags_NoGridLines,
        NO_TICK_MARKS = ImPlotAxisFlags_NoTickMarks,
        NO_TICK_LABELS = ImPlotAxisFlags_NoTickLabels,
        /// Don't fit the data the first frame.
        NO_INITIAL_FIT = ImPlotAxisFlags_NoInitialFit,
        NO_MENUS = ImPlotAxisFlags_NoMenus,
        NO_SIDE_SWITCH = ImPlotAxisFlags_NoSideSwitch,
        NO_HIGHLIGHT = ImPlotAxisFlags_NoHighlight,
        /// Ticks and labels on the other side (right or top).
        OPPOSITE = ImPlotAxisFlags_Opposite,
        /// Grid lines over the data.
        FOREGROUND = ImPlotAxisFlags_Foreground,
        INVERT = ImPlotAxisFlags_Invert,
        /// Fit the data every frame.
        AUTO_FIT = ImPlotAxisFlags_AutoFit,
        /// Fit only what's in view on the other axis.
        RANGE_FIT = ImPlotAxisFlags_RangeFit,
        PAN_STRETCH = ImPlotAxisFlags_PanStretch,
        LOCK_MIN = ImPlotAxisFlags_LockMin,
        LOCK_MAX = ImPlotAxisFlags_LockMax,
        LOCK = ImPlotAxisFlags_Lock,
        NO_DECORATIONS = ImPlotAxisFlags_NoDecorations,
        /// How ImPlot sets up extra axes: no grid lines, on the other side.
        AUX_DEFAULT = ImPlotAxisFlags_AuxDefault,
    }
}

plot_flags! {
    /// Options for [`Ui::subplots`] (`ImPlotSubplotFlags_`).
    SubplotFlags {
        NO_TITLE = ImPlotSubplotFlags_NoTitle,
        /// No shared legend (with [`SubplotFlags::SHARE_ITEMS`]).
        NO_LEGEND = ImPlotSubplotFlags_NoLegend,
        NO_MENUS = ImPlotSubplotFlags_NoMenus,
        NO_RESIZE = ImPlotSubplotFlags_NoResize,
        NO_ALIGN = ImPlotSubplotFlags_NoAlign,
        /// One legend for every plot's items.
        SHARE_ITEMS = ImPlotSubplotFlags_ShareItems,
        LINK_ROWS = ImPlotSubplotFlags_LinkRows,
        LINK_COLS = ImPlotSubplotFlags_LinkCols,
        LINK_ALL_X = ImPlotSubplotFlags_LinkAllX,
        LINK_ALL_Y = ImPlotSubplotFlags_LinkAllY,
        /// Fill the grid column by column.
        COL_MAJOR = ImPlotSubplotFlags_ColMajor,
    }
}

plot_flags! {
    /// Options for [`Plot::setup_legend`] (`ImPlotLegendFlags_`).
    LegendFlags {
        /// Entries don't hide and show items when clicked.
        NO_BUTTONS = ImPlotLegendFlags_NoButtons,
        NO_HIGHLIGHT_ITEM = ImPlotLegendFlags_NoHighlightItem,
        NO_HIGHLIGHT_AXIS = ImPlotLegendFlags_NoHighlightAxis,
        NO_MENUS = ImPlotLegendFlags_NoMenus,
        OUTSIDE = ImPlotLegendFlags_Outside,
        HORIZONTAL = ImPlotLegendFlags_Horizontal,
        SORT = ImPlotLegendFlags_Sort,
        REVERSE = ImPlotLegendFlags_Reverse,
    }
}

plot_flags! {
    /// Options for [`Plot::setup_mouse_text`] (`ImPlotMouseTextFlags_`).
    MouseTextFlags {
        NO_AUX_AXES = ImPlotMouseTextFlags_NoAuxAxes,
        NO_FORMAT = ImPlotMouseTextFlags_NoFormat,
        SHOW_ALWAYS = ImPlotMouseTextFlags_ShowAlways,
    }
}

plot_flags! {
    /// Options for drag tools ([`Plot::drag_point`] and the like) (`ImPlotDragToolFlags_`).
    DragToolFlags {
        NO_CURSORS = ImPlotDragToolFlags_NoCursors,
        NO_FIT = ImPlotDragToolFlags_NoFit,
        NO_INPUTS = ImPlotDragToolFlags_NoInputs,
        /// Draw a frame late, after the value has been adjusted.
        DELAYED = ImPlotDragToolFlags_Delayed,
    }
}

plot_flags! {
    /// Options for [`Ui::colormap_scale`] (`ImPlotColormapScaleFlags_`).
    ColormapScaleFlags {
        NO_LABEL = ImPlotColormapScaleFlags_NoLabel,
        OPPOSITE = ImPlotColormapScaleFlags_Opposite,
        INVERT = ImPlotColormapScaleFlags_Invert,
    }
}

/// Declares an item flags type: the item kind's own flags plus the ones
/// every item takes.
macro_rules! item_flags {
    ($(#[$meta:meta])* $name:ident { $($(#[$flag_meta:meta])* $flag:ident = $value:ident,)* }) => {
        plot_flags! {
            $(#[$meta])*
            $name {
                /// No legend entry.
                NO_LEGEND = ImPlotItemFlags_NoLegend,
                /// Not considered when fitting axes to the data.
                NO_FIT = ImPlotItemFlags_NoFit,
                $($(#[$flag_meta])* $flag = $value,)*
            }
        }
    };
}

item_flags! {
    /// Options for [`Plot::line_with`] (`ImPlotLineFlags_`).
    LineFlags {
        /// A segment between each pair of points, rather than one line.
        SEGMENTS = ImPlotLineFlags_Segments,
        /// Joins the last point to the first.
        LOOP = ImPlotLineFlags_Loop,
        /// Skips NaN values instead of breaking the line there.
        SKIP_NAN = ImPlotLineFlags_SkipNaN,
        NO_CLIP = ImPlotLineFlags_NoClip,
        /// Fills down to y = 0.
        SHADED = ImPlotLineFlags_Shaded,
    }
}

item_flags! {
    /// Options for [`Plot::scatter_with`] (`ImPlotScatterFlags_`).
    ScatterFlags {
        NO_CLIP = ImPlotScatterFlags_NoClip,
    }
}

item_flags! {
    /// Options for [`Plot::stairs_with`] (`ImPlotStairsFlags_`).
    StairsFlags {
        /// Each value holds to the left of its x rather than the right.
        PRE_STEP = ImPlotStairsFlags_PreStep,
        SHADED = ImPlotStairsFlags_Shaded,
    }
}

item_flags! {
    /// Options for [`Plot::shaded_with`] and [`Plot::shaded_between`] (`ImPlotShadedFlags_`).
    ShadedFlags {}
}

item_flags! {
    /// Options for [`Plot::bars_with`] (`ImPlotBarsFlags_`).
    BarsFlags {
        HORIZONTAL = ImPlotBarsFlags_Horizontal,
    }
}

item_flags! {
    /// Options for [`Plot::bar_groups`] (`ImPlotBarGroupsFlags_`).
    BarGroupsFlags {
        HORIZONTAL = ImPlotBarGroupsFlags_Horizontal,
        STACKED = ImPlotBarGroupsFlags_Stacked,
    }
}

item_flags! {
    /// Options for [`Plot::error_bars`] (`ImPlotErrorBarsFlags_`).
    ErrorBarsFlags {
        HORIZONTAL = ImPlotErrorBarsFlags_Horizontal,
    }
}

item_flags! {
    /// Options for [`Plot::stems_with`] (`ImPlotStemsFlags_`).
    StemsFlags {
        HORIZONTAL = ImPlotStemsFlags_Horizontal,
    }
}

item_flags! {
    /// Options for [`Plot::inf_lines`] (`ImPlotInfLinesFlags_`).
    InfLinesFlags {
        HORIZONTAL = ImPlotInfLinesFlags_Horizontal,
    }
}

item_flags! {
    /// Options for [`Plot::pie_chart`] (`ImPlotPieChartFlags_`).
    PieChartFlags {
        /// Always a full circle.
        NORMALIZE = ImPlotPieChartFlags_Normalize,
        /// Hidden slices leave no gap.
        IGNORE_HIDDEN = ImPlotPieChartFlags_IgnoreHidden,
        /// Pulls out the slice whose legend entry is hovered.
        EXPLODING = ImPlotPieChartFlags_Exploding,
    }
}

item_flags! {
    /// Options for [`Plot::heatmap`] (`ImPlotHeatmapFlags_`).
    HeatmapFlags {
        /// Values are column by column.
        COL_MAJOR = ImPlotHeatmapFlags_ColMajor,
    }
}

item_flags! {
    /// Options for [`Plot::histogram`] and [`Plot::histogram_2d`] (`ImPlotHistogramFlags_`).
    HistogramFlags {
        HORIZONTAL = ImPlotHistogramFlags_Horizontal,
        CUMULATIVE = ImPlotHistogramFlags_Cumulative,
        /// Normalized: a probability density.
        DENSITY = ImPlotHistogramFlags_Density,
        NO_OUTLIERS = ImPlotHistogramFlags_NoOutliers,
        COL_MAJOR = ImPlotHistogramFlags_ColMajor,
    }
}

item_flags! {
    /// Options for [`Plot::digital`] (`ImPlotDigitalFlags_`).
    DigitalFlags {}
}

item_flags! {
    /// Options for [`Plot::image`] (`ImPlotImageFlags_`).
    PlotImageFlags {}
}

item_flags! {
    /// Options for [`Plot::dummy`] (`ImPlotDummyFlags_`).
    DummyFlags {}
}

/// One of a plot's axes. X1 and Y1 are on by default; set up the others
/// with [`Plot::setup_axis`] to use them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Axis {
    X1 = sys::ImAxis_X1,
    X2 = sys::ImAxis_X2,
    X3 = sys::ImAxis_X3,
    Y1 = sys::ImAxis_Y1,
    Y2 = sys::ImAxis_Y2,
    Y3 = sys::ImAxis_Y3,
}

/// How an axis maps values (`ImPlotScale_`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Scale {
    Linear = sys::ImPlotScale_Linear,
    /// Unix time in seconds, shown as dates and times.
    Time = sys::ImPlotScale_Time,
    Log10 = sys::ImPlotScale_Log10,
    /// Logarithmic both ways from 0.
    SymLog = sys::ImPlotScale_SymLog,
}

/// Where in the plot a legend or text goes (`ImPlotLocation_`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Location {
    Center = sys::ImPlotLocation_Center,
    North = sys::ImPlotLocation_North,
    South = sys::ImPlotLocation_South,
    West = sys::ImPlotLocation_West,
    East = sys::ImPlotLocation_East,
    NorthWest = sys::ImPlotLocation_NorthWest,
    NorthEast = sys::ImPlotLocation_NorthEast,
    SouthWest = sys::ImPlotLocation_SouthWest,
    SouthEast = sys::ImPlotLocation_SouthEast,
}

/// A marker drawn at each point (`ImPlotMarker_`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Marker {
    /// None, unless the style says otherwise ([`PlotStyleVar::Marker`]).
    None = sys::ImPlotMarker_None,
    Circle = sys::ImPlotMarker_Circle,
    Square = sys::ImPlotMarker_Square,
    Diamond = sys::ImPlotMarker_Diamond,
    Up = sys::ImPlotMarker_Up,
    Down = sys::ImPlotMarker_Down,
    Left = sys::ImPlotMarker_Left,
    Right = sys::ImPlotMarker_Right,
    Cross = sys::ImPlotMarker_Cross,
    Plus = sys::ImPlotMarker_Plus,
    Asterisk = sys::ImPlotMarker_Asterisk,
}

/// How a histogram picks its bins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bins {
    /// Exactly this many (at least 1).
    Count(u32),
    /// √n bins.
    Sqrt,
    /// 1 + log₂(n) bins (ImPlot's default).
    Sturges,
    /// 2∛n bins.
    Rice,
    /// Bins 3.49 σ / ∛n wide.
    Scott,
}

impl Bins {
    fn raw(self) -> i32 {
        match self {
            Bins::Count(n) => i32::try_from(n.max(1)).unwrap_or(i32::MAX),
            Bins::Sqrt => sys::ImPlotBin_Sqrt,
            Bins::Sturges => sys::ImPlotBin_Sturges,
            Bins::Rice => sys::ImPlotBin_Rice,
            Bins::Scott => sys::ImPlotBin_Scott,
        }
    }
}

/// A plot style color, for [`Ui::with_plot_style_color`] (`ImPlotCol_`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum PlotStyleColor {
    Line = sys::ImPlotCol_Line,
    Fill = sys::ImPlotCol_Fill,
    MarkerOutline = sys::ImPlotCol_MarkerOutline,
    MarkerFill = sys::ImPlotCol_MarkerFill,
    ErrorBar = sys::ImPlotCol_ErrorBar,
    FrameBg = sys::ImPlotCol_FrameBg,
    PlotBg = sys::ImPlotCol_PlotBg,
    PlotBorder = sys::ImPlotCol_PlotBorder,
    LegendBg = sys::ImPlotCol_LegendBg,
    LegendBorder = sys::ImPlotCol_LegendBorder,
    LegendText = sys::ImPlotCol_LegendText,
    TitleText = sys::ImPlotCol_TitleText,
    InlayText = sys::ImPlotCol_InlayText,
    AxisText = sys::ImPlotCol_AxisText,
    AxisGrid = sys::ImPlotCol_AxisGrid,
    AxisTick = sys::ImPlotCol_AxisTick,
    AxisBg = sys::ImPlotCol_AxisBg,
    AxisBgHovered = sys::ImPlotCol_AxisBgHovered,
    AxisBgActive = sys::ImPlotCol_AxisBgActive,
    Selection = sys::ImPlotCol_Selection,
    Crosshairs = sys::ImPlotCol_Crosshairs,
}

/// A plot style variable with its new value, for [`Ui::with_plot_style`]
/// (`ImPlotStyleVar_`). Sizes are pixels, `[x, y]` for both axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlotStyleVar {
    LineWeight(f32),
    Marker(Marker),
    MarkerSize(f32),
    MarkerWeight(f32),
    FillAlpha(f32),
    ErrorBarSize(f32),
    ErrorBarWeight(f32),
    DigitalBitHeight(f32),
    DigitalBitGap(f32),
    PlotBorderSize(f32),
    MinorAlpha(f32),
    MajorTickLen(Vec2),
    MinorTickLen(Vec2),
    MajorTickSize(Vec2),
    MinorTickSize(Vec2),
    MajorGridSize(Vec2),
    MinorGridSize(Vec2),
    PlotPadding(Vec2),
    LabelPadding(Vec2),
    LegendPadding(Vec2),
    LegendInnerPadding(Vec2),
    LegendSpacing(Vec2),
    MousePosPadding(Vec2),
    AnnotationPadding(Vec2),
    /// Extra room around fitted data, as a fraction of its extent.
    FitPadding(Vec2),
    /// A plot's size when given `[0, 0]`.
    PlotDefaultSize(Vec2),
    PlotMinSize(Vec2),
}

enum PlotStyleValue {
    Float(f32),
    Int(i32),
    Vec2(Vec2),
}

impl PlotStyleVar {
    fn raw(self) -> (i32, PlotStyleValue) {
        use PlotStyleValue::{Float, Int, Vec2};
        use PlotStyleVar as V;
        match self {
            V::LineWeight(v) => (sys::ImPlotStyleVar_LineWeight, Float(v)),
            V::Marker(v) => (sys::ImPlotStyleVar_Marker, Int(v as i32)),
            V::MarkerSize(v) => (sys::ImPlotStyleVar_MarkerSize, Float(v)),
            V::MarkerWeight(v) => (sys::ImPlotStyleVar_MarkerWeight, Float(v)),
            V::FillAlpha(v) => (sys::ImPlotStyleVar_FillAlpha, Float(v)),
            V::ErrorBarSize(v) => (sys::ImPlotStyleVar_ErrorBarSize, Float(v)),
            V::ErrorBarWeight(v) => (sys::ImPlotStyleVar_ErrorBarWeight, Float(v)),
            V::DigitalBitHeight(v) => (sys::ImPlotStyleVar_DigitalBitHeight, Float(v)),
            V::DigitalBitGap(v) => (sys::ImPlotStyleVar_DigitalBitGap, Float(v)),
            V::PlotBorderSize(v) => (sys::ImPlotStyleVar_PlotBorderSize, Float(v)),
            V::MinorAlpha(v) => (sys::ImPlotStyleVar_MinorAlpha, Float(v)),
            V::MajorTickLen(v) => (sys::ImPlotStyleVar_MajorTickLen, Vec2(v)),
            V::MinorTickLen(v) => (sys::ImPlotStyleVar_MinorTickLen, Vec2(v)),
            V::MajorTickSize(v) => (sys::ImPlotStyleVar_MajorTickSize, Vec2(v)),
            V::MinorTickSize(v) => (sys::ImPlotStyleVar_MinorTickSize, Vec2(v)),
            V::MajorGridSize(v) => (sys::ImPlotStyleVar_MajorGridSize, Vec2(v)),
            V::MinorGridSize(v) => (sys::ImPlotStyleVar_MinorGridSize, Vec2(v)),
            V::PlotPadding(v) => (sys::ImPlotStyleVar_PlotPadding, Vec2(v)),
            V::LabelPadding(v) => (sys::ImPlotStyleVar_LabelPadding, Vec2(v)),
            V::LegendPadding(v) => (sys::ImPlotStyleVar_LegendPadding, Vec2(v)),
            V::LegendInnerPadding(v) => (sys::ImPlotStyleVar_LegendInnerPadding, Vec2(v)),
            V::LegendSpacing(v) => (sys::ImPlotStyleVar_LegendSpacing, Vec2(v)),
            V::MousePosPadding(v) => (sys::ImPlotStyleVar_MousePosPadding, Vec2(v)),
            V::AnnotationPadding(v) => (sys::ImPlotStyleVar_AnnotationPadding, Vec2(v)),
            V::FitPadding(v) => (sys::ImPlotStyleVar_FitPadding, Vec2(v)),
            V::PlotDefaultSize(v) => (sys::ImPlotStyleVar_PlotDefaultSize, Vec2(v)),
            V::PlotMinSize(v) => (sys::ImPlotStyleVar_PlotMinSize, Vec2(v)),
        }
    }
}

/// A colormap: the colors plot items cycle through, or heatmaps blend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Colormap(i32);

impl Colormap {
    pub const DEEP: Colormap = Colormap(sys::ImPlotColormap_Deep);
    pub const DARK: Colormap = Colormap(sys::ImPlotColormap_Dark);
    pub const PASTEL: Colormap = Colormap(sys::ImPlotColormap_Pastel);
    pub const PAIRED: Colormap = Colormap(sys::ImPlotColormap_Paired);
    pub const VIRIDIS: Colormap = Colormap(sys::ImPlotColormap_Viridis);
    pub const PLASMA: Colormap = Colormap(sys::ImPlotColormap_Plasma);
    pub const HOT: Colormap = Colormap(sys::ImPlotColormap_Hot);
    pub const COOL: Colormap = Colormap(sys::ImPlotColormap_Cool);
    pub const PINK: Colormap = Colormap(sys::ImPlotColormap_Pink);
    pub const JET: Colormap = Colormap(sys::ImPlotColormap_Jet);
    pub const TWILIGHT: Colormap = Colormap(sys::ImPlotColormap_Twilight);
    pub const RD_BU: Colormap = Colormap(sys::ImPlotColormap_RdBu);
    pub const BR_BG: Colormap = Colormap(sys::ImPlotColormap_BrBG);
    pub const PI_YG: Colormap = Colormap(sys::ImPlotColormap_PiYG);
    pub const SPECTRAL: Colormap = Colormap(sys::ImPlotColormap_Spectral);
    pub const GREYS: Colormap = Colormap(sys::ImPlotColormap_Greys);

    pub fn index(self) -> i32 {
        self.0
    }
}

/// A number type ImPlot plots.
pub trait PlotValue: Copy + sealed::Sealed {
    #[doc(hidden)]
    const KIND: i32;
}

mod sealed {
    pub trait Sealed {}
}

macro_rules! plot_value {
    ($($t:ty = $kind:ident),*) => {$(
        impl sealed::Sealed for $t {}
        impl PlotValue for $t {
            const KIND: i32 = sys::$kind;
        }
    )*};
}
plot_value!(
    f32 = MQEMU_PLOT_F32,
    f64 = MQEMU_PLOT_F64,
    i8 = MQEMU_PLOT_I8,
    u8 = MQEMU_PLOT_U8,
    i16 = MQEMU_PLOT_I16,
    u16 = MQEMU_PLOT_U16,
    i32 = MQEMU_PLOT_I32,
    u32 = MQEMU_PLOT_U32,
    i64 = MQEMU_PLOT_I64,
    u64 = MQEMU_PLOT_U64
);

/// Points to plot: y values against their index (`&ys`), or x and y values
/// (`(&xs, &ys)`). [`Series::x_scale`] and [`Series::x_start`] place values
/// given without x.
#[derive(Debug, Clone, Copy)]
pub struct Series<'a, T> {
    xs: Option<&'a [T]>,
    ys: &'a [T],
    x_scale: f64,
    x_start: f64,
}

impl<'a, T> Series<'a, T> {
    /// y values, at x = 0, 1, 2...
    pub fn values(ys: &'a [T]) -> Self {
        Series { xs: None, ys, x_scale: 1.0, x_start: 0.0 }
    }

    /// Points `(xs[i], ys[i])`.
    ///
    /// # Panics
    /// If the slices' lengths differ.
    pub fn xy(xs: &'a [T], ys: &'a [T]) -> Self {
        assert_eq!(xs.len(), ys.len(), "a plotted series needs as many x values as y values");
        Series { xs: Some(xs), ys, x_scale: 1.0, x_start: 0.0 }
    }

    /// For values without x: the x distance between them.
    pub fn x_scale(mut self, scale: f64) -> Self {
        self.x_scale = scale;
        self
    }

    /// For values without x: the first one's x.
    pub fn x_start(mut self, start: f64) -> Self {
        self.x_start = start;
        self
    }

    fn len(&self) -> i32 {
        count(self.ys)
    }

    fn xs_ptr(&self) -> *const c_void {
        self.xs.map_or(std::ptr::null(), |xs| xs.as_ptr().cast())
    }
}

impl<'a, T> From<&'a [T]> for Series<'a, T> {
    fn from(ys: &'a [T]) -> Self {
        Series::values(ys)
    }
}

impl<'a, T> From<&'a Vec<T>> for Series<'a, T> {
    fn from(ys: &'a Vec<T>) -> Self {
        Series::values(ys)
    }
}

impl<'a, T, const N: usize> From<&'a [T; N]> for Series<'a, T> {
    fn from(ys: &'a [T; N]) -> Self {
        Series::values(ys)
    }
}

impl<'a, T, X: AsRef<[T]> + ?Sized, Y: AsRef<[T]> + ?Sized> From<(&'a X, &'a Y)> for Series<'a, T> {
    fn from((xs, ys): (&'a X, &'a Y)) -> Self {
        Series::xy(xs.as_ref(), ys.as_ref())
    }
}

/// An area of a plot in plot units.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotRect {
    pub x: RangeInclusive<f64>,
    pub y: RangeInclusive<f64>,
}

impl PlotRect {
    fn from_raw(r: [f64; 4]) -> PlotRect {
        PlotRect { x: r[0]..=r[1], y: r[2]..=r[3] }
    }
}

/// What a drag tool did this frame ([`Plot::drag_point`] and the like).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DragState {
    /// The value changed (it's being dragged).
    pub changed: bool,
    pub clicked: bool,
    pub hovered: bool,
    pub held: bool,
}

/// ImPlot counts take an `int`.
fn count<T>(values: &[T]) -> i32 {
    i32::try_from(values.len()).unwrap_or(i32::MAX)
}

/// Panics unless `format` is a printf format for at most one double (ImPlot
/// formats every tick and label with one).
fn check_format(format: &str) {
    assert!(
        printf_conversions(format, b"fFeEgGaA").is_some_and(|n| n <= 1),
        "an ImPlot format must contain at most one floating-point conversion (and no others): {format:?}"
    );
}

/// Labels as the pointer and length arrays the shim takes.
fn label_arrays<S: AsRef<str>>(labels: &[S]) -> (Vec<*const c_char>, Vec<usize>) {
    labels.iter().map(|l| s(l.as_ref())).unzip()
}

fn auto_color(color: &Option<Color>) -> *const f32 {
    color.as_ref().map_or(std::ptr::null(), |c| c.as_ptr())
}

/// Most colors a colormap may have, and fewest.
const MIN_COLORMAP_COLORS: usize = 2;

impl Ui {
    /// The raw ImPlot functions, for tests that misuse them on purpose.
    #[cfg(test)]
    pub(crate) fn plot_api(&self) -> &'static sys::Api {
        self.plot
    }

    /// A plot; configure it, then [`PlotBuilder::show`] its contents.
    pub fn plot<'ui, 'a>(&'ui mut self, title: &'a str) -> PlotBuilder<'ui, 'a> {
        PlotBuilder { ui: self, title, size: [-1.0, 0.0], flags: PlotFlags::NONE, next: Vec::new() }
    }

    /// A `rows` x `cols` grid of plots sharing space (and optionally axes and
    /// a legend); [`SubplotsBuilder::show`] draws them.
    ///
    /// # Panics
    /// If `rows` or `cols` is 0.
    pub fn subplots<'ui, 'a>(&'ui mut self, title: &'a str, rows: usize, cols: usize) -> SubplotsBuilder<'ui, 'a> {
        assert!(rows > 0 && cols > 0, "subplots need at least 1 row and column, not {rows} x {cols}");
        SubplotsBuilder { ui: self, title, rows, cols, size: [-1.0, 400.0], flags: SubplotFlags::NONE, row_ratios: None, col_ratios: None }
    }

    /// Draws `f`, lining up the axes of the plots it draws (vertically
    /// stacked, or side by side if not `vertical`).
    pub fn aligned_plots<R>(&mut self, group: &str, vertical: bool, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let (g, n) = s(group);
        if !unsafe { (self.plot.mqemu_implot_begin_aligned_plots)(g, n, vertical) } {
            return None;
        }
        let _end = EndOnDrop(self.plot.mqemu_implot_end_aligned_plots);
        Some(f(self))
    }

    /// Draws `f` with a plot style color changed.
    pub fn with_plot_style_color<R>(&mut self, which: PlotStyleColor, color: Color, f: impl FnOnce(&mut Ui) -> R) -> R {
        unsafe { (self.plot.mqemu_implot_push_style_color)(which as i32, color.as_ptr()) };
        let pop = self.plot.mqemu_implot_pop_style_color;
        let _pop = OnDrop(move || unsafe { pop(1) });
        f(self)
    }

    /// Draws `f` with a plot style variable changed.
    pub fn with_plot_style<R>(&mut self, var: PlotStyleVar, f: impl FnOnce(&mut Ui) -> R) -> R {
        match var.raw() {
            (index, PlotStyleValue::Float(v)) => unsafe { (self.plot.mqemu_implot_push_style_var_float)(index, v) },
            (index, PlotStyleValue::Int(v)) => unsafe { (self.plot.mqemu_implot_push_style_var_int)(index, v) },
            (index, PlotStyleValue::Vec2([x, y])) => unsafe { (self.plot.mqemu_implot_push_style_var_vec2)(index, x, y) },
        }
        let pop = self.plot.mqemu_implot_pop_style_var;
        let _pop = OnDrop(move || unsafe { pop(1) });
        f(self)
    }

    /// Draws `f` with `colormap` as the plots' colormap.
    pub fn with_colormap<R>(&mut self, colormap: Colormap, f: impl FnOnce(&mut Ui) -> R) -> R {
        unsafe { (self.plot.mqemu_implot_push_colormap)(colormap.0) };
        let pop = self.plot.mqemu_implot_pop_colormap;
        let _pop = OnDrop(move || unsafe { pop(1) });
        f(self)
    }

    /// Adds a colormap of `colors` (at least 2), blended smoothly unless
    /// `qualitative`. ImPlot keeps it while the game runs: None if one named
    /// `name` already exists ([`Ui::colormap_named`] finds it).
    ///
    /// # Panics
    /// With fewer than 2 colors.
    pub fn add_colormap(&mut self, name: &str, colors: &[Color], qualitative: bool) -> Option<Colormap> {
        assert!(colors.len() >= MIN_COLORMAP_COLORS, "a colormap needs at least {MIN_COLORMAP_COLORS} colors");
        let (n, nl) = s(name);
        let index = unsafe { (self.plot.mqemu_implot_add_colormap)(n, nl, colors.as_ptr().cast(), count(colors), qualitative) };
        (index >= 0).then_some(Colormap(index))
    }

    /// The colormap called `name` (such as `"Viridis"`).
    pub fn colormap_named(&self, name: &str) -> Option<Colormap> {
        let (n, nl) = s(name);
        let index = unsafe { (self.plot.mqemu_implot_colormap_index)(n, nl) };
        (index >= 0).then_some(Colormap(index))
    }

    /// Every colormap: ImPlot's own, then added ones.
    pub fn colormaps(&self) -> impl Iterator<Item = Colormap> + use<> {
        (0..unsafe { (self.plot.mqemu_implot_colormap_count)() }).map(Colormap)
    }

    pub fn colormap_name(&self, colormap: Colormap) -> Option<String> {
        let name = unsafe { (self.plot.mqemu_implot_colormap_name)(colormap.0) };
        (!name.is_null()).then(|| unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned())
    }

    /// How many colors it has.
    pub fn colormap_size(&self, colormap: Colormap) -> usize {
        usize::try_from(unsafe { (self.plot.mqemu_implot_colormap_size)(colormap.0) }).unwrap_or(0)
    }

    /// Its color `index` (wrapping around).
    pub fn colormap_color(&self, colormap: Colormap, index: usize) -> Color {
        let mut out = [0.0; 4];
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        unsafe { (self.plot.mqemu_implot_colormap_color)(colormap.0, index, false, 0.0, out.as_mut_ptr()) };
        out
    }

    /// Its color at `t`, 0 to 1 along it.
    pub fn sample_colormap(&self, colormap: Colormap, t: f32) -> Color {
        let mut out = [0.0; 4];
        unsafe { (self.plot.mqemu_implot_colormap_color)(colormap.0, 0, true, t, out.as_mut_ptr()) };
        out
    }

    /// A bar showing `colormap` (None for the current one) from `range`'s
    /// start to its end, `size` pixels (0 for defaults), labeled with a
    /// printf `format` for one double such as `"%.0f"`.
    ///
    /// # Panics
    /// If `format` has any other conversion.
    pub fn colormap_scale(&mut self, label: &str, range: RangeInclusive<f64>, size: Vec2, format: &str, colormap: Option<Colormap>) {
        check_format(format);
        let ((l, ln), (f, fl)) = (s(label), s(format));
        let cmap = colormap.map_or(sys::IMPLOT_AUTO, |c| c.0);
        unsafe { (self.plot.mqemu_implot_colormap_scale)(l, ln, *range.start(), *range.end(), size[0], size[1], f, fl, 0, cmap) }
    }

    /// A slider over `colormap` (None for the current one) editing `t`, 0 to
    /// 1; true when changed. `color` gets the color at `t`.
    pub fn colormap_slider(&mut self, label: &str, t: &mut f32, color: Option<&mut Color>, colormap: Option<Colormap>) -> bool {
        let ((l, ln), (f, fl)) = (s(label), s(""));
        let out = color.map_or(std::ptr::null_mut(), |c| c.as_mut_ptr());
        let cmap = colormap.map_or(sys::IMPLOT_AUTO, |c| c.0);
        unsafe { (self.plot.mqemu_implot_colormap_slider)(l, ln, t, out, f, fl, cmap) }
    }

    /// A button showing `colormap` (None for the current one); true when clicked.
    pub fn colormap_button(&mut self, label: &str, size: Vec2, colormap: Option<Colormap>) -> bool {
        let (l, ln) = s(label);
        let cmap = colormap.map_or(sys::IMPLOT_AUTO, |c| c.0);
        unsafe { (self.plot.mqemu_implot_colormap_button)(l, ln, size[0], size[1], cmap) }
    }
}

/// A plot being configured; see [`Ui::plot`].
#[must_use = "a plot is drawn by calling show"]
pub struct PlotBuilder<'ui, 'a> {
    ui: &'ui mut Ui,
    title: &'a str,
    size: Vec2,
    flags: PlotFlags,
    /// Applied just before the plot begins.
    next: Vec<NextAxis>,
}

/// An axis setting made before a plot begins.
#[derive(Clone, Copy)]
enum NextAxis {
    Limits(Axis, f64, f64, Cond),
    Fit(Axis),
}

impl PlotBuilder<'_, '_> {
    /// `[w, h]` in pixels; -1 fills the width (the default), 0 is ImPlot's default size.
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = size;
        self
    }

    pub fn flags(mut self, flags: PlotFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Sets an axis's limits from outside the plot (as [`Plot::setup_axis_limits`]).
    pub fn axis_limits(mut self, axis: Axis, range: RangeInclusive<f64>, cond: Cond) -> Self {
        self.next.push(NextAxis::Limits(axis, *range.start(), *range.end(), cond));
        self
    }

    /// Fits an axis to the data this frame.
    pub fn fit_axis(mut self, axis: Axis) -> Self {
        self.next.push(NextAxis::Fit(axis));
        self
    }

    /// Draws the plot, with `f` setting it up and plotting through [`Plot`],
    /// if it's visible.
    pub fn show<R>(self, f: impl FnOnce(&mut Plot<'_>) -> R) -> Option<R> {
        begin_plot(self.ui, self.title, self.size, self.flags, &self.next, f)
    }
}

fn begin_plot<R>(ui: &mut Ui, title: &str, size: Vec2, flags: PlotFlags, next: &[NextAxis], f: impl FnOnce(&mut Plot<'_>) -> R) -> Option<R> {
    for &setting in next {
        match setting {
            NextAxis::Limits(axis, min, max, cond) => unsafe { (ui.plot.mqemu_implot_set_next_axis_limits)(axis as i32, min, max, cond as i32) },
            NextAxis::Fit(axis) => unsafe { (ui.plot.mqemu_implot_set_next_axis_to_fit)(axis as i32) },
        }
    }
    let (t, n) = s(title);
    if !unsafe { (ui.plot.mqemu_implot_begin_plot)(t, n, size[0], size[1], flags.bits()) } {
        return None;
    }
    let _end = EndOnDrop(ui.plot.mqemu_implot_end_plot);
    Some(f(&mut Plot { ui, locked: false }))
}

/// A grid of plots being configured; see [`Ui::subplots`].
#[must_use = "subplots are drawn by calling show"]
pub struct SubplotsBuilder<'ui, 'a> {
    ui: &'ui mut Ui,
    title: &'a str,
    rows: usize,
    cols: usize,
    size: Vec2,
    flags: SubplotFlags,
    row_ratios: Option<&'a mut [f32]>,
    col_ratios: Option<&'a mut [f32]>,
}

impl<'a> SubplotsBuilder<'_, 'a> {
    /// `[w, h]` of the whole grid; -1 fills the width. Plots' own sizes are ignored.
    pub fn size(mut self, size: Vec2) -> Self {
        self.size = size;
        self
    }

    pub fn flags(mut self, flags: SubplotFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Relative row heights (one per row); updated as the user resizes them.
    ///
    /// # Panics
    /// If there isn't one per row.
    pub fn row_ratios(mut self, ratios: &'a mut [f32]) -> Self {
        assert_eq!(ratios.len(), self.rows, "subplot row ratios need one value per row");
        self.row_ratios = Some(ratios);
        self
    }

    /// Relative column widths (one per column); updated as the user resizes them.
    ///
    /// # Panics
    /// If there isn't one per column.
    pub fn col_ratios(mut self, ratios: &'a mut [f32]) -> Self {
        assert_eq!(ratios.len(), self.cols, "subplot column ratios need one value per column");
        self.col_ratios = Some(ratios);
        self
    }

    /// Draws the grid, with `f` drawing its plots in order ([`Subplots::plot`]), if it's visible.
    pub fn show<R>(self, f: impl FnOnce(&mut Subplots<'_>) -> R) -> Option<R> {
        let (t, n) = s(self.title);
        let to_i32 = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        let rows = self.row_ratios.map_or(std::ptr::null_mut(), |r| r.as_mut_ptr());
        let cols = self.col_ratios.map_or(std::ptr::null_mut(), |c| c.as_mut_ptr());
        let flags = self.flags.bits();
        let [w, h] = self.size;
        if !unsafe { (self.ui.plot.mqemu_implot_begin_subplots)(t, n, to_i32(self.rows), to_i32(self.cols), w, h, flags, rows, cols) } {
            return None;
        }
        let _end = EndOnDrop(self.ui.plot.mqemu_implot_end_subplots);
        Some(f(&mut Subplots { ui: self.ui, remaining: self.rows.saturating_mul(self.cols) }))
    }
}

/// The plots of a [`Ui::subplots`] grid.
pub struct Subplots<'ui> {
    ui: &'ui mut Ui,
    /// Cells not drawn yet.
    remaining: usize,
}

impl Subplots<'_> {
    /// The next cell's plot (its size is the cell's). Titles may be empty.
    ///
    /// # Panics
    /// When every cell already has one.
    pub fn plot<R>(&mut self, title: &str, flags: PlotFlags, f: impl FnOnce(&mut Plot<'_>) -> R) -> Option<R> {
        assert!(self.remaining > 0, "every subplot cell already has a plot");
        self.remaining -= 1;
        begin_plot(self.ui, title, [0.0, 0.0], flags, &[], f)
    }

    /// Whether the mouse is over the grid.
    pub fn is_hovered(&self) -> bool {
        unsafe { (self.ui.plot.mqemu_implot_is_subplots_hovered)() }
    }
}

/// The current plot, inside [`PlotBuilder::show`].
pub struct Plot<'ui> {
    ui: &'ui mut Ui,
    /// Whether ImPlot has finished setup (something was plotted or queried).
    locked: bool,
}

impl Plot<'_> {
    /// The raw functions, for tests that misuse them on purpose.
    #[cfg(test)]
    pub(crate) fn api(&self) -> &'static sys::Api {
        self.ui.plot
    }

    fn setup(&self) -> &'static sys::Api {
        assert!(!self.locked, "ImPlot setup must come before anything is plotted or queried");
        self.ui.plot
    }

    fn lock(&mut self) -> &'static sys::Api {
        self.locked = true;
        self.ui.plot
    }

    // ----- setup -----

    /// Turns on and labels an axis (X1 and Y1 are on already).
    ///
    /// # Panics
    /// After something was plotted or queried.
    pub fn setup_axis(&mut self, axis: Axis, label: Option<&str>, flags: AxisFlags) {
        let (l, ln) = opt(label);
        unsafe { (self.setup().mqemu_implot_setup_axis)(axis as i32, l, ln, flags.bits()) }
    }

    /// Labels X1 and Y1.
    ///
    /// # Panics
    /// After something was plotted or queried.
    pub fn setup_axes(&mut self, x_label: Option<&str>, y_label: Option<&str>) {
        self.setup_axis(Axis::X1, x_label, AxisFlags::NONE);
        self.setup_axis(Axis::Y1, y_label, AxisFlags::NONE);
    }

    /// An axis's range: [`Cond::Always`] keeps it there, [`Cond::Once`] lets
    /// the user pan and zoom afterwards.
    ///
    /// # Panics
    /// After something was plotted or queried.
    pub fn setup_axis_limits(&mut self, axis: Axis, range: RangeInclusive<f64>, cond: Cond) {
        unsafe { (self.setup().mqemu_implot_setup_axis_limits)(axis as i32, *range.start(), *range.end(), cond as i32) }
    }

    /// How an axis labels its ticks: a printf format for one double, e.g. `"%.0f HP"`.
    ///
    /// # Panics
    /// After something was plotted or queried, or if `format` has another conversion.
    pub fn setup_axis_format(&mut self, axis: Axis, format: &str) {
        check_format(format);
        let (f, fl) = s(format);
        unsafe { (self.setup().mqemu_implot_setup_axis_format)(axis as i32, f, fl) }
    }

    /// Ticks at `values`, labeled with `labels` if given (one per value);
    /// ImPlot's own ticks too if `keep_default`.
    ///
    /// # Panics
    /// After something was plotted or queried, or if there isn't a label per value.
    pub fn setup_axis_ticks<S: AsRef<str>>(&mut self, axis: Axis, values: &[f64], labels: Option<&[S]>, keep_default: bool) {
        self.ticks(axis, Some(values), 0.0..=0.0, values.len(), labels, keep_default);
    }

    /// `count` evenly spaced ticks over `range`, labeled as for [`Plot::setup_axis_ticks`].
    ///
    /// # Panics
    /// As [`Plot::setup_axis_ticks`].
    pub fn setup_axis_ticks_range<S: AsRef<str>>(&mut self, axis: Axis, range: RangeInclusive<f64>, count: usize, labels: Option<&[S]>, keep_default: bool) {
        self.ticks(axis, None, range, count, labels, keep_default);
    }

    fn ticks<S: AsRef<str>>(&mut self, axis: Axis, values: Option<&[f64]>, range: RangeInclusive<f64>, n: usize, labels: Option<&[S]>, keep: bool) {
        let api = self.setup();
        let (names, lens) = labels.map(|l| {
            assert_eq!(l.len(), n, "axis ticks need one label per tick");
            label_arrays(l)
        }).unwrap_or_default();
        let (names_ptr, lens_ptr) = if labels.is_some() { (names.as_ptr(), lens.as_ptr()) } else { (std::ptr::null(), std::ptr::null()) };
        let values = values.map_or(std::ptr::null(), <[f64]>::as_ptr);
        let n = i32::try_from(n).unwrap_or(i32::MAX);
        unsafe { (api.mqemu_implot_setup_axis_ticks)(axis as i32, values, *range.start(), *range.end(), n, names_ptr, lens_ptr, keep) }
    }

    /// # Panics
    /// After something was plotted or queried.
    pub fn setup_axis_scale(&mut self, axis: Axis, scale: Scale) {
        unsafe { (self.setup().mqemu_implot_setup_axis_scale)(axis as i32, scale as i32) }
    }

    /// How far the user can pan an axis.
    ///
    /// # Panics
    /// After something was plotted or queried.
    pub fn setup_axis_limits_constraints(&mut self, axis: Axis, range: RangeInclusive<f64>) {
        unsafe { (self.setup().mqemu_implot_setup_axis_limits_constraints)(axis as i32, *range.start(), *range.end()) }
    }

    /// The narrowest and widest span the user can zoom an axis to.
    ///
    /// # Panics
    /// After something was plotted or queried.
    pub fn setup_axis_zoom_constraints(&mut self, axis: Axis, range: RangeInclusive<f64>) {
        unsafe { (self.setup().mqemu_implot_setup_axis_zoom_constraints)(axis as i32, *range.start(), *range.end()) }
    }

    /// # Panics
    /// After something was plotted or queried.
    pub fn setup_legend(&mut self, location: Location, flags: LegendFlags) {
        unsafe { (self.setup().mqemu_implot_setup_legend)(location as i32, flags.bits()) }
    }

    /// Where the mouse position is shown.
    ///
    /// # Panics
    /// After something was plotted or queried.
    pub fn setup_mouse_text(&mut self, location: Location, flags: MouseTextFlags) {
        unsafe { (self.setup().mqemu_implot_setup_mouse_text)(location as i32, flags.bits()) }
    }

    /// Ends setup now (plotting or querying would end it anyway).
    pub fn setup_finish(&mut self) {
        unsafe { (self.lock().mqemu_implot_setup_finish)() }
    }

    // ----- items -----

    /// A line through the points.
    pub fn line<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>) {
        self.line_with(label, data, LineFlags::NONE);
    }

    pub fn line_with<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>, flags: LineFlags) {
        let (data, (l, ln)) = (data.into(), s(label));
        let api = self.lock();
        unsafe { (api.mqemu_implot_line)(l, ln, T::KIND, data.xs_ptr(), data.ys.as_ptr().cast(), data.len(), data.x_scale, data.x_start, flags.bits()) }
    }

    /// A marker at each point.
    pub fn scatter<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>) {
        self.scatter_with(label, data, ScatterFlags::NONE);
    }

    pub fn scatter_with<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>, flags: ScatterFlags) {
        let (data, (l, ln)) = (data.into(), s(label));
        let api = self.lock();
        unsafe {
            (api.mqemu_implot_scatter)(l, ln, T::KIND, data.xs_ptr(), data.ys.as_ptr().cast(), data.len(), data.x_scale, data.x_start, flags.bits())
        }
    }

    /// A step line: each value holds until the next x.
    pub fn stairs<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>) {
        self.stairs_with(label, data, StairsFlags::NONE);
    }

    pub fn stairs_with<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>, flags: StairsFlags) {
        let (data, (l, ln)) = (data.into(), s(label));
        let api = self.lock();
        unsafe {
            (api.mqemu_implot_stairs)(l, ln, T::KIND, data.xs_ptr(), data.ys.as_ptr().cast(), data.len(), data.x_scale, data.x_start, flags.bits())
        }
    }

    /// The area between the points and y = `y_ref` (`f64::INFINITY` and
    /// `NEG_INFINITY` fill to the plot's edge).
    pub fn shaded<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>, y_ref: f64) {
        self.shaded_with(label, data, y_ref, ShadedFlags::NONE);
    }

    pub fn shaded_with<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>, y_ref: f64, flags: ShadedFlags) {
        let (data, (l, ln)) = (data.into(), s(label));
        let api = self.lock();
        let ys = data.ys.as_ptr().cast();
        unsafe {
            (api.mqemu_implot_shaded)(l, ln, T::KIND, data.xs_ptr(), ys, std::ptr::null(), data.len(), y_ref, data.x_scale, data.x_start, flags.bits())
        }
    }

    /// The area between two lines with the same x values.
    ///
    /// # Panics
    /// If the slices' lengths differ.
    pub fn shaded_between<T: PlotValue>(&mut self, label: &str, xs: &[T], ys1: &[T], ys2: &[T], flags: ShadedFlags) {
        assert!(xs.len() == ys1.len() && xs.len() == ys2.len(), "a shaded area needs as many values in each slice");
        let (l, ln) = s(label);
        let api = self.lock();
        let (x, y1, y2) = (xs.as_ptr().cast(), ys1.as_ptr().cast(), ys2.as_ptr().cast());
        unsafe { (api.mqemu_implot_shaded)(l, ln, T::KIND, x, y1, y2, count(xs), 0.0, 1.0, 0.0, flags.bits()) }
    }

    /// A bar per point, `bar_size` plot units wide (0.67 leaves gaps between
    /// bars one unit apart).
    pub fn bars<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>, bar_size: f64) {
        self.bars_with(label, data, bar_size, BarsFlags::NONE);
    }

    /// [`Plot::bars`] with flags; bars of values without x are at their
    /// [`Series::x_start`] plus their index.
    pub fn bars_with<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>, bar_size: f64, flags: BarsFlags) {
        let (data, (l, ln)) = (data.into(), s(label));
        let api = self.lock();
        unsafe { (api.mqemu_implot_bars)(l, ln, T::KIND, data.xs_ptr(), data.ys.as_ptr().cast(), data.len(), bar_size, data.x_start, flags.bits()) }
    }

    /// Groups of bars: `values` holds, for each of `items` (named in the
    /// legend), one value per group, item after item. `group_size` is the
    /// width of a group in plot units; groups are one unit apart, from `shift`.
    ///
    /// # Panics
    /// If `values` isn't `items.len()` times a whole number of groups.
    pub fn bar_groups<T: PlotValue, S: AsRef<str>>(&mut self, items: &[S], values: &[T], group_size: f64, shift: f64, flags: BarGroupsFlags) {
        assert!(!items.is_empty() && values.len() % items.len() == 0, "bar groups need the same number of values for every item");
        let groups = values.len() / items.len();
        let (names, lens) = label_arrays(items);
        let api = self.lock();
        let (n_items, n_groups) = (count(items), i32::try_from(groups).unwrap_or(i32::MAX));
        unsafe {
            (api.mqemu_implot_bar_groups)(names.as_ptr(), lens.as_ptr(), T::KIND, values.as_ptr().cast(), n_items, n_groups, group_size, shift, flags.bits())
        }
    }

    /// Error bars at each point: `neg` below and `pos` above (or `neg` both ways).
    ///
    /// # Panics
    /// If the slices' lengths differ.
    pub fn error_bars<T: PlotValue>(&mut self, label: &str, xs: &[T], ys: &[T], neg: &[T], pos: Option<&[T]>, flags: ErrorBarsFlags) {
        let same = xs.len() == ys.len() && xs.len() == neg.len() && pos.is_none_or(|p| p.len() == xs.len());
        assert!(same, "error bars need as many values in each slice");
        let (l, ln) = s(label);
        let api = self.lock();
        let pos = pos.map_or(std::ptr::null(), |p| p.as_ptr().cast());
        unsafe { (api.mqemu_implot_error_bars)(l, ln, T::KIND, xs.as_ptr().cast(), ys.as_ptr().cast(), neg.as_ptr().cast(), pos, count(xs), flags.bits()) }
    }

    /// A line from y = `reference` to each point, with a marker.
    pub fn stems<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>, reference: f64) {
        self.stems_with(label, data, reference, StemsFlags::NONE);
    }

    pub fn stems_with<'a, T: PlotValue + 'a>(&mut self, label: &str, data: impl Into<Series<'a, T>>, reference: f64, flags: StemsFlags) {
        let (data, (l, ln)) = (data.into(), s(label));
        let api = self.lock();
        let ys = data.ys.as_ptr().cast();
        unsafe { (api.mqemu_implot_stems)(l, ln, T::KIND, data.xs_ptr(), ys, data.len(), reference, data.x_scale, data.x_start, flags.bits()) }
    }

    /// Vertical lines across the plot at x = each value (horizontal ones at
    /// y with [`InfLinesFlags::HORIZONTAL`]).
    pub fn inf_lines<T: PlotValue>(&mut self, label: &str, values: &[T], flags: InfLinesFlags) {
        let (l, ln) = s(label);
        let api = self.lock();
        unsafe { (api.mqemu_implot_inf_lines)(l, ln, T::KIND, values.as_ptr().cast(), count(values), flags.bits()) }
    }

    /// A pie of `values`, one slice per label, centered at `center` with
    /// `radius` in plot units, starting at `angle0` degrees. `format` labels
    /// each slice with a printf format for one double (None for no labels).
    ///
    /// # Panics
    /// If there isn't a label per value, or `format` has another conversion.
    #[allow(clippy::too_many_arguments)]
    pub fn pie_chart<T: PlotValue, S: AsRef<str>>(
        &mut self,
        labels: &[S],
        values: &[T],
        center: [f64; 2],
        radius: f64,
        format: Option<&str>,
        angle0: f64,
        flags: PieChartFlags,
    ) {
        assert_eq!(labels.len(), values.len(), "a pie chart needs a label per value");
        format.inspect(|f| check_format(f));
        let (names, lens) = label_arrays(labels);
        let (f, fl) = opt(format);
        let api = self.lock();
        let [x, y] = center;
        unsafe {
            (api.mqemu_implot_pie_chart)(names.as_ptr(), lens.as_ptr(), T::KIND, values.as_ptr().cast(), count(values), x, y, radius, f, fl, angle0, flags.bits())
        }
    }

    /// `values` as `rows` rows of colored cells (row by row), colored by the
    /// colormap from `scale`'s start to its end (None fits the data), filling
    /// `bounds` (`[min, max]` corners in plot units). `format` labels each
    /// cell as for [`Plot::pie_chart`].
    ///
    /// # Panics
    /// If `values` doesn't fill whole rows, or `format` has another conversion.
    #[allow(clippy::too_many_arguments)]
    pub fn heatmap<T: PlotValue>(
        &mut self,
        label: &str,
        values: &[T],
        rows: usize,
        scale: Option<RangeInclusive<f64>>,
        format: Option<&str>,
        bounds: [[f64; 2]; 2],
        flags: HeatmapFlags,
    ) {
        assert!(rows > 0 && values.len() % rows == 0, "a heatmap's values must fill {rows} whole rows");
        format.inspect(|f| check_format(f));
        let cols = values.len() / rows;
        let ((l, ln), (f, fl)) = (s(label), opt(format));
        let (min, max) = scale.map_or((0.0, 0.0), |r| (*r.start(), *r.end()));
        let bounds = [bounds[0][0], bounds[0][1], bounds[1][0], bounds[1][1]];
        let api = self.lock();
        let to_i32 = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
        unsafe {
            (api.mqemu_implot_heatmap)(l, ln, T::KIND, values.as_ptr().cast(), to_i32(rows), to_i32(cols), min, max, f, fl, bounds.as_ptr(), flags.bits())
        }
    }

    /// Bars counting `values` in bins over `range` (None for the data's), each
    /// count times `bar_scale`. Returns the tallest bar's height.
    pub fn histogram<T: PlotValue>(&mut self, label: &str, values: &[T], bins: Bins, bar_scale: f64, range: Option<RangeInclusive<f64>>, flags: HistogramFlags) -> f64 {
        let (l, ln) = s(label);
        let (min, max) = range.map_or((0.0, 0.0), |r| (*r.start(), *r.end()));
        let api = self.lock();
        unsafe { (api.mqemu_implot_histogram)(l, ln, T::KIND, values.as_ptr().cast(), count(values), bins.raw(), bar_scale, min, max, flags.bits()) }
    }

    /// A 2D histogram of points, as a heatmap of counts over `range` (None
    /// for the data's). Returns the largest count.
    ///
    /// # Panics
    /// If the slices' lengths differ.
    #[allow(clippy::too_many_arguments)]
    pub fn histogram_2d<T: PlotValue>(&mut self, label: &str, xs: &[T], ys: &[T], x_bins: Bins, y_bins: Bins, range: Option<PlotRect>, flags: HistogramFlags) -> f64 {
        assert_eq!(xs.len(), ys.len(), "a 2D histogram needs as many x values as y values");
        let (l, ln) = s(label);
        let range = range.map_or([0.0; 4], |r| [*r.x.start(), *r.x.end(), *r.y.start(), *r.y.end()]);
        let api = self.lock();
        let (x, y) = (xs.as_ptr().cast(), ys.as_ptr().cast());
        unsafe { (api.mqemu_implot_histogram_2d)(l, ln, T::KIND, x, y, count(xs), x_bins.raw(), y_bins.raw(), range.as_ptr(), flags.bits()) }
    }

    /// A digital signal (high where `ys` isn't 0), drawn in a band at the
    /// bottom of the plot.
    ///
    /// # Panics
    /// If the slices' lengths differ.
    pub fn digital<T: PlotValue>(&mut self, label: &str, xs: &[T], ys: &[T], flags: DigitalFlags) {
        assert_eq!(xs.len(), ys.len(), "a digital signal needs as many x values as y values");
        let (l, ln) = s(label);
        let api = self.lock();
        unsafe { (api.mqemu_implot_digital)(l, ln, T::KIND, xs.as_ptr().cast(), ys.as_ptr().cast(), count(xs), flags.bits()) }
    }

    /// `texture` filling `bounds` (`[min, max]` corners in plot units).
    pub fn image(&mut self, label: &str, texture: TextureId<'_>, bounds: [[f64; 2]; 2], uv: [Vec2; 2], tint: Color, flags: PlotImageFlags) {
        let (l, ln) = s(label);
        let bounds = [bounds[0][0], bounds[0][1], bounds[1][0], bounds[1][1]];
        let api = self.lock();
        unsafe { (api.mqemu_implot_image)(l, ln, texture.raw(), bounds.as_ptr(), uv.as_ptr().cast(), tint.as_ptr(), flags.bits()) }
    }

    /// Text centered at `pos` (plot units), moved by `offset` pixels; vertical if `vertical`.
    pub fn text(&mut self, text: &str, pos: [f64; 2], offset: Vec2, vertical: bool) {
        let (t, n) = s(text);
        let flags = if vertical { sys::ImPlotTextFlags_Vertical } else { 0 };
        let api = self.lock();
        unsafe { (api.mqemu_implot_text)(t, n, pos[0], pos[1], offset[0], offset[1], flags) }
    }

    /// A legend entry with nothing plotted (for a legend popup, say).
    pub fn dummy(&mut self, label: &str, flags: DummyFlags) {
        let (l, ln) = s(label);
        unsafe { (self.lock().mqemu_implot_dummy)(l, ln, flags.bits()) }
    }

    // ----- tools -----

    /// A point the user can drag, `size` pixels; `pos` follows it.
    pub fn drag_point(&mut self, id: i32, pos: &mut [f64; 2], color: Color, size: f32, flags: DragToolFlags) -> DragState {
        let mut state = [false; 3];
        let [x, y] = pos;
        let changed = unsafe { (self.lock().mqemu_implot_drag_point)(id, x, y, color.as_ptr(), size, flags.bits(), state.as_mut_ptr()) };
        drag_state(changed, state)
    }

    /// A vertical line the user can drag; `x` follows it.
    pub fn drag_line_x(&mut self, id: i32, x: &mut f64, color: Color, thickness: f32, flags: DragToolFlags) -> DragState {
        self.drag_line(id, false, x, color, thickness, flags)
    }

    /// A horizontal line the user can drag; `y` follows it.
    pub fn drag_line_y(&mut self, id: i32, y: &mut f64, color: Color, thickness: f32, flags: DragToolFlags) -> DragState {
        self.drag_line(id, true, y, color, thickness, flags)
    }

    fn drag_line(&mut self, id: i32, horizontal: bool, value: &mut f64, color: Color, thickness: f32, flags: DragToolFlags) -> DragState {
        let mut state = [false; 3];
        let api = self.lock();
        let changed = unsafe { (api.mqemu_implot_drag_line)(id, horizontal, value, color.as_ptr(), thickness, flags.bits(), state.as_mut_ptr()) };
        drag_state(changed, state)
    }

    /// A rectangle the user can drag and resize; `corners` (`[x1, y1, x2,
    /// y2]`) follow it.
    pub fn drag_rect(&mut self, id: i32, corners: &mut [f64; 4], color: Color, flags: DragToolFlags) -> DragState {
        let mut state = [false; 3];
        let api = self.lock();
        let changed = unsafe { (api.mqemu_implot_drag_rect)(id, corners.as_mut_ptr(), color.as_ptr(), flags.bits(), state.as_mut_ptr()) };
        drag_state(changed, state)
    }

    /// A label at `pos` (plot units), moved by `offset` pixels, kept in the
    /// plot if `clamp`; showing `text`, or the y value if None.
    pub fn annotation(&mut self, pos: [f64; 2], color: Color, offset: Vec2, clamp: bool, text: Option<&str>) {
        let (t, n) = opt(text);
        let api = self.lock();
        unsafe { (api.mqemu_implot_annotation)(pos[0], pos[1], color.as_ptr(), offset[0], offset[1], clamp, t, n) }
    }

    /// A tag on the x axis at `x`, showing `text` or the value.
    pub fn tag_x(&mut self, x: f64, color: Color, text: Option<&str>) {
        let (t, n) = opt(text);
        unsafe { (self.lock().mqemu_implot_tag)(false, x, color.as_ptr(), t, n) }
    }

    /// A tag on the y axis at `y`, showing `text` or the value.
    pub fn tag_y(&mut self, y: f64, color: Color, text: Option<&str>) {
        let (t, n) = opt(text);
        unsafe { (self.lock().mqemu_implot_tag)(true, y, color.as_ptr(), t, n) }
    }

    // ----- styling the next item -----

    /// The next item's line color and weight (None for ImPlot's choice).
    pub fn next_line_style(&mut self, color: Option<Color>, weight: Option<f32>) {
        unsafe { (self.ui.plot.mqemu_implot_set_next_line_style)(auto_color(&color), weight.unwrap_or(-1.0)) }
    }

    /// The next item's fill color and alpha.
    pub fn next_fill_style(&mut self, color: Option<Color>, alpha: Option<f32>) {
        unsafe { (self.ui.plot.mqemu_implot_set_next_fill_style)(auto_color(&color), alpha.unwrap_or(-1.0)) }
    }

    /// The next item's markers.
    pub fn next_marker_style(&mut self, marker: Marker, size: Option<f32>, fill: Option<Color>, weight: Option<f32>, outline: Option<Color>) {
        let api = self.ui.plot;
        unsafe {
            (api.mqemu_implot_set_next_marker_style)(marker as i32, size.unwrap_or(-1.0), auto_color(&fill), weight.unwrap_or(-1.0), auto_color(&outline))
        }
    }

    /// The next error bars' color, whisker size and weight.
    pub fn next_error_bar_style(&mut self, color: Option<Color>, size: Option<f32>, weight: Option<f32>) {
        unsafe { (self.ui.plot.mqemu_implot_set_next_error_bar_style)(auto_color(&color), size.unwrap_or(-1.0), weight.unwrap_or(-1.0)) }
    }

    /// Hides (or shows) the next item, as clicking its legend entry would:
    /// [`Cond::Once`] lets the user change it afterwards.
    pub fn hide_next_item(&mut self, hidden: bool, cond: Cond) {
        unsafe { (self.ui.plot.mqemu_implot_hide_next_item)(hidden, cond as i32) }
    }

    /// The color ImPlot gave the last item.
    pub fn last_item_color(&self) -> Color {
        let mut out = [0.0; 4];
        unsafe { (self.ui.plot.mqemu_implot_last_item_color)(out.as_mut_ptr()) };
        out
    }

    // ----- queries -----

    /// Plots the next items against these axes (set up first).
    pub fn set_axes(&mut self, x: Axis, y: Axis) {
        unsafe { (self.lock().mqemu_implot_set_axes)(x as i32, y as i32) }
    }

    /// A screen position in plot units (on the current axes).
    pub fn pixels_to_plot(&mut self, pixels: Vec2) -> [f64; 2] {
        let (mut x, mut y) = (0.0, 0.0);
        unsafe { (self.lock().mqemu_implot_pixels_to_plot)(pixels[0], pixels[1], sys::IMPLOT_AUTO, sys::IMPLOT_AUTO, &mut x, &mut y) };
        [x, y]
    }

    /// A point in plot units on the screen.
    pub fn plot_to_pixels(&mut self, point: [f64; 2]) -> Vec2 {
        let (mut x, mut y) = (0.0, 0.0);
        unsafe { (self.lock().mqemu_implot_plot_to_pixels)(point[0], point[1], sys::IMPLOT_AUTO, sys::IMPLOT_AUTO, &mut x, &mut y) };
        [x, y]
    }

    /// The plot area's top left and size on the screen.
    pub fn plot_rect(&mut self) -> (Vec2, Vec2) {
        let (mut x, mut y, mut w, mut h) = (0.0, 0.0, 0.0, 0.0);
        unsafe { (self.lock().mqemu_implot_plot_rect)(&mut x, &mut y, &mut w, &mut h) };
        ([x, y], [w, h])
    }

    /// The mouse position in plot units.
    pub fn mouse_pos(&mut self) -> [f64; 2] {
        let (mut x, mut y) = (0.0, 0.0);
        unsafe { (self.lock().mqemu_implot_mouse_pos)(sys::IMPLOT_AUTO, sys::IMPLOT_AUTO, &mut x, &mut y) };
        [x, y]
    }

    /// What's in view, in plot units.
    pub fn limits(&mut self) -> PlotRect {
        let mut out = [0.0; 4];
        unsafe { (self.lock().mqemu_implot_limits)(sys::IMPLOT_AUTO, sys::IMPLOT_AUTO, out.as_mut_ptr()) };
        PlotRect::from_raw(out)
    }

    pub fn is_hovered(&mut self) -> bool {
        unsafe { (self.lock().mqemu_implot_is_plot_hovered)() }
    }

    pub fn is_axis_hovered(&mut self, axis: Axis) -> bool {
        unsafe { (self.lock().mqemu_implot_is_axis_hovered)(axis as i32) }
    }

    /// The box the user selected (dragging with the right button), if any.
    pub fn selection(&mut self) -> Option<PlotRect> {
        let mut out = [0.0; 4];
        let selected = unsafe { (self.lock().mqemu_implot_selection)(sys::IMPLOT_AUTO, sys::IMPLOT_AUTO, out.as_mut_ptr()) };
        selected.then(|| PlotRect::from_raw(out))
    }

    pub fn cancel_selection(&mut self) {
        unsafe { (self.lock().mqemu_implot_cancel_selection)() }
    }

    /// Whether the mouse is over `label`'s legend entry.
    pub fn is_legend_entry_hovered(&mut self, label: &str) -> bool {
        let (l, ln) = s(label);
        unsafe { (self.lock().mqemu_implot_is_legend_entry_hovered)(l, ln) }
    }

    /// A popup opened by right-clicking `label`'s legend entry; `f` draws it while open.
    pub fn legend_popup<R>(&mut self, label: &str, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        let (l, ln) = s(label);
        let api = self.lock();
        if !unsafe { (api.mqemu_implot_begin_legend_popup)(l, ln, imgui_sys::ImGuiMouseButton_Right) } {
            return None;
        }
        let _end = EndOnDrop(api.mqemu_implot_end_legend_popup);
        Some(f(self.ui))
    }

    /// Draws on the plot, clipped to the plot area: positions are pixels
    /// ([`Plot::plot_to_pixels`] converts).
    pub fn draw<R>(&mut self, f: impl FnOnce(&DrawList<'_>) -> R) -> R {
        let api = self.lock();
        unsafe { (api.mqemu_implot_push_clip_rect)(0.0) };
        let _pop = EndOnDrop(api.mqemu_implot_pop_clip_rect);
        let list = unsafe { (api.mqemu_implot_draw_list)() };
        f(&DrawList::new(self.ui, list))
    }

    // ----- drag and drop -----

    /// The plot area as a drop target; see [`Ui::drag_drop_target`].
    pub fn drag_drop_target<R>(&mut self, f: impl FnOnce(&mut DropTarget<'_>) -> R) -> Option<R> {
        self.target(0, Axis::X1, f)
    }

    /// An axis as a drop target.
    pub fn axis_drag_drop_target<R>(&mut self, axis: Axis, f: impl FnOnce(&mut DropTarget<'_>) -> R) -> Option<R> {
        self.target(1, axis, f)
    }

    /// The legend as a drop target.
    pub fn legend_drag_drop_target<R>(&mut self, f: impl FnOnce(&mut DropTarget<'_>) -> R) -> Option<R> {
        self.target(2, Axis::X1, f)
    }

    fn target<R>(&mut self, which: i32, axis: Axis, f: impl FnOnce(&mut DropTarget<'_>) -> R) -> Option<R> {
        let api = self.lock();
        if !unsafe { (api.mqemu_implot_begin_drag_drop_target)(which, axis as i32) } {
            return None;
        }
        Some(dnd::target_scope(self.ui, api.mqemu_implot_end_drag_drop_target, f))
    }

    /// Dragging the plot area (with ctrl held) as a drag source; see [`Ui::drag_drop_source`].
    pub fn drag_drop_source<R>(&mut self, flags: DragDropFlags, f: impl FnOnce(&mut DragSource<'_>) -> R) -> Option<R> {
        self.source(0, Axis::X1, "", flags, f)
    }

    /// Dragging an axis (with ctrl held) as a drag source.
    pub fn axis_drag_drop_source<R>(&mut self, axis: Axis, flags: DragDropFlags, f: impl FnOnce(&mut DragSource<'_>) -> R) -> Option<R> {
        self.source(1, axis, "", flags, f)
    }

    /// Dragging `label`'s legend entry as a drag source.
    pub fn item_drag_drop_source<R>(&mut self, label: &str, flags: DragDropFlags, f: impl FnOnce(&mut DragSource<'_>) -> R) -> Option<R> {
        self.source(2, Axis::X1, label, flags, f)
    }

    fn source<R>(&mut self, which: i32, axis: Axis, label: &str, flags: DragDropFlags, f: impl FnOnce(&mut DragSource<'_>) -> R) -> Option<R> {
        let (l, ln) = s(label);
        let api = self.lock();
        if !unsafe { (api.mqemu_implot_begin_drag_drop_source)(which, axis as i32, l, ln, flags.bits()) } {
            return None;
        }
        Some(dnd::source_scope(self.ui, api.mqemu_implot_end_drag_drop_source, f))
    }
}

fn drag_state(changed: bool, [clicked, hovered, held]: [bool; 3]) -> DragState {
    DragState { changed, clicked, hovered, held }
}
