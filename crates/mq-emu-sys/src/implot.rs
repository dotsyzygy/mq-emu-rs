//! C wrappers around ImPlot 0.17, which MacroQuest builds into `imgui.dll`,
//! from `shim/implot.cpp`.
//!
//! MacroQuest creates one ImPlot context and keeps it current while plugins
//! draw, so these work wherever the [`imgui`](crate::imgui) functions do, with
//! the same conventions and error handling ([`mqemu_imgui_take_error`]
//! reports ImPlot's errors too).
//!
//! ImPlot's plotting functions are templates; here each takes a `kind`
//! ([`MQEMU_PLOT_F32`] and so on) saying what its arrays hold, and arrays are
//! contiguous. Every plot item, setup call and query must come between a
//! [`mqemu_implot_begin_plot`] that returned true and its end; setup calls
//! must come before the first item or query. Optional colors are null for
//! ImPlot's automatic choice; axes passed to queries are `ImAxis_` values or
//! [`IMPLOT_AUTO`].
//!
//! Format strings (`format`) are printf formats for one `double`.
//!
//! [`mqemu_imgui_take_error`]: crate::imgui::mqemu_imgui_take_error

use core::ffi::{c_char, c_void};

use crate::imgui::ImDrawList;

/// `float` values.
pub const MQEMU_PLOT_F32: i32 = 0;
/// `double` values.
pub const MQEMU_PLOT_F64: i32 = 1;
pub const MQEMU_PLOT_I8: i32 = 2;
pub const MQEMU_PLOT_U8: i32 = 3;
pub const MQEMU_PLOT_I16: i32 = 4;
pub const MQEMU_PLOT_U16: i32 = 5;
pub const MQEMU_PLOT_I32: i32 = 6;
pub const MQEMU_PLOT_U32: i32 = 7;
pub const MQEMU_PLOT_I64: i32 = 8;
pub const MQEMU_PLOT_U64: i32 = 9;

/// ImPlot's "choose automatically" (`IMPLOT_AUTO`), for axes, sizes and weights.
pub const IMPLOT_AUTO: i32 = -1;

function_table! {
    // ----- plots -----
    /// Call [`mqemu_implot_end_plot`] only if this returns true.
    pub fn mqemu_implot_begin_plot(title: *const c_char, title_len: usize, w: f32, h: f32, flags: i32) -> bool;
    pub fn mqemu_implot_end_plot();
    /// A `rows` x `cols` grid of plots, each begun with [`mqemu_implot_begin_plot`].
    /// `row_ratios`/`col_ratios` (`rows`/`cols` floats, or null) are updated
    /// as the user resizes. Call [`mqemu_implot_end_subplots`] only if this returns true.
    pub fn mqemu_implot_begin_subplots(
        title: *const c_char,
        title_len: usize,
        rows: i32,
        cols: i32,
        w: f32,
        h: f32,
        flags: i32,
        row_ratios: *mut f32,
        col_ratios: *mut f32,
    ) -> bool;
    pub fn mqemu_implot_end_subplots();
    /// Lines up the axes of the plots until [`mqemu_implot_end_aligned_plots`]
    /// (call only if this returns true).
    pub fn mqemu_implot_begin_aligned_plots(group: *const c_char, group_len: usize, vertical: bool) -> bool;
    pub fn mqemu_implot_end_aligned_plots();

    // ----- setup -----
    /// `label_len` [`NO_STRING`](crate::imgui::NO_STRING) for no label.
    pub fn mqemu_implot_setup_axis(axis: i32, label: *const c_char, label_len: usize, flags: i32);
    pub fn mqemu_implot_setup_axis_limits(axis: i32, min: f64, max: f64, cond: i32);
    pub fn mqemu_implot_setup_axis_format(axis: i32, format: *const c_char, format_len: usize);
    /// Ticks at `count` `values`, or with `values` null evenly from `min` to
    /// `max`; `labels`/`label_lens` (`count` each, or null) name them.
    pub fn mqemu_implot_setup_axis_ticks(
        axis: i32,
        values: *const f64,
        min: f64,
        max: f64,
        count: i32,
        labels: *const *const c_char,
        label_lens: *const usize,
        keep_default: bool,
    );
    /// `scale` an `ImPlotScale_` value.
    pub fn mqemu_implot_setup_axis_scale(axis: i32, scale: i32);
    pub fn mqemu_implot_setup_axis_limits_constraints(axis: i32, min: f64, max: f64);
    pub fn mqemu_implot_setup_axis_zoom_constraints(axis: i32, min: f64, max: f64);
    /// `location` an `ImPlotLocation_` value.
    pub fn mqemu_implot_setup_legend(location: i32, flags: i32);
    pub fn mqemu_implot_setup_mouse_text(location: i32, flags: i32);
    pub fn mqemu_implot_setup_finish();
    /// Before [`mqemu_implot_begin_plot`]: the next plot's limits, or fit to its data.
    pub fn mqemu_implot_set_next_axis_limits(axis: i32, min: f64, max: f64, cond: i32);
    pub fn mqemu_implot_set_next_axis_to_fit(axis: i32);

    // ----- items -----
    /// With `xs` null, `ys` against x = xstart + i * xscale.
    pub fn mqemu_implot_line(
        label: *const c_char,
        label_len: usize,
        kind: i32,
        xs: *const c_void,
        ys: *const c_void,
        count: i32,
        xscale: f64,
        xstart: f64,
        flags: i32,
    );
    pub fn mqemu_implot_scatter(
        label: *const c_char,
        label_len: usize,
        kind: i32,
        xs: *const c_void,
        ys: *const c_void,
        count: i32,
        xscale: f64,
        xstart: f64,
        flags: i32,
    );
    pub fn mqemu_implot_stairs(
        label: *const c_char,
        label_len: usize,
        kind: i32,
        xs: *const c_void,
        ys: *const c_void,
        count: i32,
        xscale: f64,
        xstart: f64,
        flags: i32,
    );
    /// Between `ys` and `ys2` (needs `xs`), or `ys` and y = `yref`.
    pub fn mqemu_implot_shaded(
        label: *const c_char,
        label_len: usize,
        kind: i32,
        xs: *const c_void,
        ys: *const c_void,
        ys2: *const c_void,
        count: i32,
        yref: f64,
        xscale: f64,
        xstart: f64,
        flags: i32,
    );
    /// With `xs` null, bar i at x = shift + i.
    pub fn mqemu_implot_bars(
        label: *const c_char,
        label_len: usize,
        kind: i32,
        xs: *const c_void,
        ys: *const c_void,
        count: i32,
        bar_size: f64,
        shift: f64,
        flags: i32,
    );
    /// `values`: `item_count` rows of `group_count`; `labels` names the items.
    pub fn mqemu_implot_bar_groups(
        labels: *const *const c_char,
        label_lens: *const usize,
        kind: i32,
        values: *const c_void,
        item_count: i32,
        group_count: i32,
        group_size: f64,
        shift: f64,
        flags: i32,
    );
    /// `pos` null for symmetric bars of `neg`.
    pub fn mqemu_implot_error_bars(
        label: *const c_char,
        label_len: usize,
        kind: i32,
        xs: *const c_void,
        ys: *const c_void,
        neg: *const c_void,
        pos: *const c_void,
        count: i32,
        flags: i32,
    );
    pub fn mqemu_implot_stems(
        label: *const c_char,
        label_len: usize,
        kind: i32,
        xs: *const c_void,
        ys: *const c_void,
        count: i32,
        reference: f64,
        scale: f64,
        start: f64,
        flags: i32,
    );
    pub fn mqemu_implot_inf_lines(label: *const c_char, label_len: usize, kind: i32, values: *const c_void, count: i32, flags: i32);
    /// `format_len` [`NO_STRING`](crate::imgui::NO_STRING) for no slice labels.
    pub fn mqemu_implot_pie_chart(
        labels: *const *const c_char,
        label_lens: *const usize,
        kind: i32,
        values: *const c_void,
        count: i32,
        x: f64,
        y: f64,
        radius: f64,
        format: *const c_char,
        format_len: usize,
        angle0: f64,
        flags: i32,
    );
    /// `bounds`: min x, min y, max x, max y.
    pub fn mqemu_implot_heatmap(
        label: *const c_char,
        label_len: usize,
        kind: i32,
        values: *const c_void,
        rows: i32,
        cols: i32,
        scale_min: f64,
        scale_max: f64,
        format: *const c_char,
        format_len: usize,
        bounds: *const f64,
        flags: i32,
    );
    /// `bins` > 0, or an `ImPlotBin_` method. Returns the tallest bin's height.
    pub fn mqemu_implot_histogram(
        label: *const c_char,
        label_len: usize,
        kind: i32,
        values: *const c_void,
        count: i32,
        bins: i32,
        bar_scale: f64,
        range_min: f64,
        range_max: f64,
        flags: i32,
    ) -> f64;
    /// `range`: min x, max x, min y, max y (all 0 for the data's).
    pub fn mqemu_implot_histogram_2d(
        label: *const c_char,
        label_len: usize,
        kind: i32,
        xs: *const c_void,
        ys: *const c_void,
        count: i32,
        x_bins: i32,
        y_bins: i32,
        range: *const f64,
        flags: i32,
    ) -> f64;
    pub fn mqemu_implot_digital(label: *const c_char, label_len: usize, kind: i32, xs: *const c_void, ys: *const c_void, count: i32, flags: i32);
    /// `bounds` as for the heatmap; `uv` 2 points; `tint` 4 floats.
    pub fn mqemu_implot_image(
        label: *const c_char,
        label_len: usize,
        texture: *mut c_void,
        bounds: *const f64,
        uv: *const f32,
        tint: *const f32,
        flags: i32,
    );
    pub fn mqemu_implot_text(text: *const c_char, len: usize, x: f64, y: f64, dx: f32, dy: f32, flags: i32);
    pub fn mqemu_implot_dummy(label: *const c_char, label_len: usize, flags: i32);

    // ----- tools -----
    /// True while dragged; `state` (3 bools) gets clicked, hovered, held.
    pub fn mqemu_implot_drag_point(id: i32, x: *mut f64, y: *mut f64, col: *const f32, size: f32, flags: i32, state: *mut bool) -> bool;
    pub fn mqemu_implot_drag_line(
        id: i32,
        horizontal: bool,
        value: *mut f64,
        col: *const f32,
        thickness: f32,
        flags: i32,
        state: *mut bool,
    ) -> bool;
    /// `rect`: x1, y1, x2, y2.
    pub fn mqemu_implot_drag_rect(id: i32, rect: *mut f64, col: *const f32, flags: i32, state: *mut bool) -> bool;
    /// `text_len` [`NO_STRING`](crate::imgui::NO_STRING) shows the y value.
    pub fn mqemu_implot_annotation(x: f64, y: f64, col: *const f32, dx: f32, dy: f32, clamp: bool, text: *const c_char, text_len: usize);
    /// On the y axis with `y_axis`; `text_len` [`NO_STRING`](crate::imgui::NO_STRING) shows the value.
    pub fn mqemu_implot_tag(y_axis: bool, value: f64, col: *const f32, text: *const c_char, text_len: usize);

    // ----- queries -----
    /// `y_axis` -1 sets only the x axis.
    pub fn mqemu_implot_set_axes(x_axis: i32, y_axis: i32);
    pub fn mqemu_implot_pixels_to_plot(px: f32, py: f32, x_axis: i32, y_axis: i32, x: *mut f64, y: *mut f64);
    pub fn mqemu_implot_plot_to_pixels(x: f64, y: f64, x_axis: i32, y_axis: i32, px: *mut f32, py: *mut f32);
    pub fn mqemu_implot_plot_rect(x: *mut f32, y: *mut f32, w: *mut f32, h: *mut f32);
    pub fn mqemu_implot_mouse_pos(x_axis: i32, y_axis: i32, x: *mut f64, y: *mut f64);
    /// `out` (4): min x, max x, min y, max y.
    pub fn mqemu_implot_limits(x_axis: i32, y_axis: i32, out: *mut f64);
    pub fn mqemu_implot_is_plot_hovered() -> bool;
    pub fn mqemu_implot_is_axis_hovered(axis: i32) -> bool;
    pub fn mqemu_implot_is_subplots_hovered() -> bool;
    /// The box the user selected, if any, into `out` as for [`mqemu_implot_limits`].
    pub fn mqemu_implot_selection(x_axis: i32, y_axis: i32, out: *mut f64) -> bool;
    pub fn mqemu_implot_cancel_selection();
    pub fn mqemu_implot_hide_next_item(hidden: bool, cond: i32);
    pub fn mqemu_implot_is_legend_entry_hovered(label: *const c_char, label_len: usize) -> bool;
    /// Call [`mqemu_implot_end_legend_popup`] only if this returns true.
    pub fn mqemu_implot_begin_legend_popup(label: *const c_char, label_len: usize, button: i32) -> bool;
    pub fn mqemu_implot_end_legend_popup();

    // ----- drawing in a plot -----
    pub fn mqemu_implot_draw_list() -> *mut ImDrawList;
    pub fn mqemu_implot_push_clip_rect(expand: f32);
    pub fn mqemu_implot_pop_clip_rect();

    // ----- drag and drop -----
    /// `which`: 0 the plot area, 1 the axis `axis`, 2 the legend. End only if this returns true.
    pub fn mqemu_implot_begin_drag_drop_target(which: i32, axis: i32) -> bool;
    pub fn mqemu_implot_end_drag_drop_target();
    /// `which`: 0 the plot area, 1 the axis `axis`, 2 the legend item `label`. End only if this returns true.
    pub fn mqemu_implot_begin_drag_drop_source(which: i32, axis: i32, label: *const c_char, label_len: usize, flags: i32) -> bool;
    pub fn mqemu_implot_end_drag_drop_source();

    // ----- style -----
    /// `idx` an `ImPlotCol_` value (checked); `rgba` 4 floats.
    pub fn mqemu_implot_push_style_color(idx: i32, rgba: *const f32);
    pub fn mqemu_implot_pop_style_color(count: i32);
    /// `idx` an `ImPlotStyleVar_` value of the matching kind.
    pub fn mqemu_implot_push_style_var_float(idx: i32, value: f32);
    pub fn mqemu_implot_push_style_var_int(idx: i32, value: i32);
    pub fn mqemu_implot_push_style_var_vec2(idx: i32, x: f32, y: f32);
    pub fn mqemu_implot_pop_style_var(count: i32);
    /// Null colors and negative numbers leave ImPlot's choice.
    pub fn mqemu_implot_set_next_line_style(col: *const f32, weight: f32);
    pub fn mqemu_implot_set_next_fill_style(col: *const f32, alpha: f32);
    /// `marker` an `ImPlotMarker_` value; -1 is both none and automatic.
    pub fn mqemu_implot_set_next_marker_style(marker: i32, size: f32, fill: *const f32, weight: f32, outline: *const f32);
    pub fn mqemu_implot_set_next_error_bar_style(col: *const f32, size: f32, weight: f32);
    pub fn mqemu_implot_last_item_color(out: *mut f32);

    // ----- colormaps -----
    /// `cmap` arguments: an `ImPlotColormap_` value, an added one, or -1 for
    /// the current one; others are an ImGui error.
    pub fn mqemu_implot_colormap_count() -> i32;
    /// `colors`: `count` RGBA colors. -1 if `name` is taken.
    pub fn mqemu_implot_add_colormap(name: *const c_char, name_len: usize, colors: *const f32, count: i32, qualitative: bool) -> i32;
    /// The colormap's name (NUL-terminated, static), or null.
    pub fn mqemu_implot_colormap_name(cmap: i32) -> *const c_char;
    pub fn mqemu_implot_colormap_index(name: *const c_char, name_len: usize) -> i32;
    pub fn mqemu_implot_colormap_size(cmap: i32) -> i32;
    /// Color `index` (wrapping), or with `sample` the color at `t` (0 to 1), into `out` (4 floats).
    pub fn mqemu_implot_colormap_color(cmap: i32, index: i32, sample: bool, t: f32, out: *mut f32);
    pub fn mqemu_implot_next_colormap_color(out: *mut f32);
    /// `cmap` must not be -1 here.
    pub fn mqemu_implot_push_colormap(cmap: i32);
    pub fn mqemu_implot_pop_colormap(count: i32);
    pub fn mqemu_implot_colormap_scale(
        label: *const c_char,
        label_len: usize,
        min: f64,
        max: f64,
        w: f32,
        h: f32,
        format: *const c_char,
        format_len: usize,
        flags: i32,
        cmap: i32,
    );
    /// `out` (4 floats) may be null. `format` is a float slider's format.
    pub fn mqemu_implot_colormap_slider(
        label: *const c_char,
        label_len: usize,
        t: *mut f32,
        out: *mut f32,
        format: *const c_char,
        format_len: usize,
        cmap: i32,
    ) -> bool;
    pub fn mqemu_implot_colormap_button(label: *const c_char, label_len: usize, w: f32, h: f32, cmap: i32) -> bool;
}

macro_rules! implot_constants {
    ($($name:ident = $value:expr,)*) => {
        $( pub const $name: i32 = $value; )*

        /// Every constant above with its name, for checking against `implot.h`.
        #[doc(hidden)]
        pub const ALL_CONSTANTS: &[(&str, i32)] = &[$( (stringify!($name), $name) ),*];
    };
}

// Values from MacroQuest's src/imgui/implot/implot.h (0.17). The tests check
// them against it.
implot_constants! {
    // ImAxis
    ImAxis_X1 = 0,
    ImAxis_X2 = 1,
    ImAxis_X3 = 2,
    ImAxis_Y1 = 3,
    ImAxis_Y2 = 4,
    ImAxis_Y3 = 5,

    // ImPlotFlags
    ImPlotFlags_None = 0,
    ImPlotFlags_NoTitle = 1 << 0,
    ImPlotFlags_NoLegend = 1 << 1,
    ImPlotFlags_NoMouseText = 1 << 2,
    ImPlotFlags_NoInputs = 1 << 3,
    ImPlotFlags_NoMenus = 1 << 4,
    ImPlotFlags_NoBoxSelect = 1 << 5,
    ImPlotFlags_NoFrame = 1 << 6,
    ImPlotFlags_Equal = 1 << 7,
    ImPlotFlags_Crosshairs = 1 << 8,
    ImPlotFlags_CanvasOnly = 0x37,

    // ImPlotAxisFlags
    ImPlotAxisFlags_None = 0,
    ImPlotAxisFlags_NoLabel = 1 << 0,
    ImPlotAxisFlags_NoGridLines = 1 << 1,
    ImPlotAxisFlags_NoTickMarks = 1 << 2,
    ImPlotAxisFlags_NoTickLabels = 1 << 3,
    ImPlotAxisFlags_NoInitialFit = 1 << 4,
    ImPlotAxisFlags_NoMenus = 1 << 5,
    ImPlotAxisFlags_NoSideSwitch = 1 << 6,
    ImPlotAxisFlags_NoHighlight = 1 << 7,
    ImPlotAxisFlags_Opposite = 1 << 8,
    ImPlotAxisFlags_Foreground = 1 << 9,
    ImPlotAxisFlags_Invert = 1 << 10,
    ImPlotAxisFlags_AutoFit = 1 << 11,
    ImPlotAxisFlags_RangeFit = 1 << 12,
    ImPlotAxisFlags_PanStretch = 1 << 13,
    ImPlotAxisFlags_LockMin = 1 << 14,
    ImPlotAxisFlags_LockMax = 1 << 15,
    ImPlotAxisFlags_Lock = 0xc000,
    ImPlotAxisFlags_NoDecorations = 0xf,
    ImPlotAxisFlags_AuxDefault = 0x102,

    // ImPlotSubplotFlags
    ImPlotSubplotFlags_None = 0,
    ImPlotSubplotFlags_NoTitle = 1 << 0,
    ImPlotSubplotFlags_NoLegend = 1 << 1,
    ImPlotSubplotFlags_NoMenus = 1 << 2,
    ImPlotSubplotFlags_NoResize = 1 << 3,
    ImPlotSubplotFlags_NoAlign = 1 << 4,
    ImPlotSubplotFlags_ShareItems = 1 << 5,
    ImPlotSubplotFlags_LinkRows = 1 << 6,
    ImPlotSubplotFlags_LinkCols = 1 << 7,
    ImPlotSubplotFlags_LinkAllX = 1 << 8,
    ImPlotSubplotFlags_LinkAllY = 1 << 9,
    ImPlotSubplotFlags_ColMajor = 1 << 10,

    // ImPlotLegendFlags
    ImPlotLegendFlags_None = 0,
    ImPlotLegendFlags_NoButtons = 1 << 0,
    ImPlotLegendFlags_NoHighlightItem = 1 << 1,
    ImPlotLegendFlags_NoHighlightAxis = 1 << 2,
    ImPlotLegendFlags_NoMenus = 1 << 3,
    ImPlotLegendFlags_Outside = 1 << 4,
    ImPlotLegendFlags_Horizontal = 1 << 5,
    ImPlotLegendFlags_Sort = 1 << 6,
    ImPlotLegendFlags_Reverse = 1 << 7,

    // ImPlotMouseTextFlags
    ImPlotMouseTextFlags_None = 0,
    ImPlotMouseTextFlags_NoAuxAxes = 1 << 0,
    ImPlotMouseTextFlags_NoFormat = 1 << 1,
    ImPlotMouseTextFlags_ShowAlways = 1 << 2,

    // ImPlotDragToolFlags
    ImPlotDragToolFlags_None = 0,
    ImPlotDragToolFlags_NoCursors = 1 << 0,
    ImPlotDragToolFlags_NoFit = 1 << 1,
    ImPlotDragToolFlags_NoInputs = 1 << 2,
    ImPlotDragToolFlags_Delayed = 1 << 3,

    // ImPlotColormapScaleFlags
    ImPlotColormapScaleFlags_None = 0,
    ImPlotColormapScaleFlags_NoLabel = 1 << 0,
    ImPlotColormapScaleFlags_Opposite = 1 << 1,
    ImPlotColormapScaleFlags_Invert = 1 << 2,

    // ImPlotItemFlags
    ImPlotItemFlags_None = 0,
    ImPlotItemFlags_NoLegend = 1 << 0,
    ImPlotItemFlags_NoFit = 1 << 1,

    // ImPlotLineFlags
    ImPlotLineFlags_None = 0,
    ImPlotLineFlags_Segments = 1 << 10,
    ImPlotLineFlags_Loop = 1 << 11,
    ImPlotLineFlags_SkipNaN = 1 << 12,
    ImPlotLineFlags_NoClip = 1 << 13,
    ImPlotLineFlags_Shaded = 1 << 14,

    // ImPlotScatterFlags
    ImPlotScatterFlags_None = 0,
    ImPlotScatterFlags_NoClip = 1 << 10,

    // ImPlotStairsFlags
    ImPlotStairsFlags_None = 0,
    ImPlotStairsFlags_PreStep = 1 << 10,
    ImPlotStairsFlags_Shaded = 1 << 11,

    // ImPlotShadedFlags
    ImPlotShadedFlags_None = 0,

    // ImPlotBarsFlags
    ImPlotBarsFlags_None = 0,
    ImPlotBarsFlags_Horizontal = 1 << 10,

    // ImPlotBarGroupsFlags
    ImPlotBarGroupsFlags_None = 0,
    ImPlotBarGroupsFlags_Horizontal = 1 << 10,
    ImPlotBarGroupsFlags_Stacked = 1 << 11,

    // ImPlotErrorBarsFlags
    ImPlotErrorBarsFlags_None = 0,
    ImPlotErrorBarsFlags_Horizontal = 1 << 10,

    // ImPlotStemsFlags
    ImPlotStemsFlags_None = 0,
    ImPlotStemsFlags_Horizontal = 1 << 10,

    // ImPlotInfLinesFlags
    ImPlotInfLinesFlags_None = 0,
    ImPlotInfLinesFlags_Horizontal = 1 << 10,

    // ImPlotPieChartFlags
    ImPlotPieChartFlags_None = 0,
    ImPlotPieChartFlags_Normalize = 1 << 10,
    ImPlotPieChartFlags_IgnoreHidden = 1 << 11,
    ImPlotPieChartFlags_Exploding = 1 << 12,

    // ImPlotHeatmapFlags
    ImPlotHeatmapFlags_None = 0,
    ImPlotHeatmapFlags_ColMajor = 1 << 10,

    // ImPlotHistogramFlags
    ImPlotHistogramFlags_None = 0,
    ImPlotHistogramFlags_Horizontal = 1 << 10,
    ImPlotHistogramFlags_Cumulative = 1 << 11,
    ImPlotHistogramFlags_Density = 1 << 12,
    ImPlotHistogramFlags_NoOutliers = 1 << 13,
    ImPlotHistogramFlags_ColMajor = 1 << 14,

    // ImPlotDigitalFlags
    ImPlotDigitalFlags_None = 0,

    // ImPlotImageFlags
    ImPlotImageFlags_None = 0,

    // ImPlotTextFlags
    ImPlotTextFlags_None = 0,
    ImPlotTextFlags_Vertical = 1 << 10,

    // ImPlotDummyFlags
    ImPlotDummyFlags_None = 0,

    // ImPlotCond
    ImPlotCond_None = 0,
    ImPlotCond_Always = 1,
    ImPlotCond_Once = 2,

    // ImPlotCol
    ImPlotCol_Line = 0,
    ImPlotCol_Fill = 1,
    ImPlotCol_MarkerOutline = 2,
    ImPlotCol_MarkerFill = 3,
    ImPlotCol_ErrorBar = 4,
    ImPlotCol_FrameBg = 5,
    ImPlotCol_PlotBg = 6,
    ImPlotCol_PlotBorder = 7,
    ImPlotCol_LegendBg = 8,
    ImPlotCol_LegendBorder = 9,
    ImPlotCol_LegendText = 10,
    ImPlotCol_TitleText = 11,
    ImPlotCol_InlayText = 12,
    ImPlotCol_AxisText = 13,
    ImPlotCol_AxisGrid = 14,
    ImPlotCol_AxisTick = 15,
    ImPlotCol_AxisBg = 16,
    ImPlotCol_AxisBgHovered = 17,
    ImPlotCol_AxisBgActive = 18,
    ImPlotCol_Selection = 19,
    ImPlotCol_Crosshairs = 20,

    // ImPlotStyleVar
    ImPlotStyleVar_LineWeight = 0,
    ImPlotStyleVar_Marker = 1,
    ImPlotStyleVar_MarkerSize = 2,
    ImPlotStyleVar_MarkerWeight = 3,
    ImPlotStyleVar_FillAlpha = 4,
    ImPlotStyleVar_ErrorBarSize = 5,
    ImPlotStyleVar_ErrorBarWeight = 6,
    ImPlotStyleVar_DigitalBitHeight = 7,
    ImPlotStyleVar_DigitalBitGap = 8,
    ImPlotStyleVar_PlotBorderSize = 9,
    ImPlotStyleVar_MinorAlpha = 10,
    ImPlotStyleVar_MajorTickLen = 11,
    ImPlotStyleVar_MinorTickLen = 12,
    ImPlotStyleVar_MajorTickSize = 13,
    ImPlotStyleVar_MinorTickSize = 14,
    ImPlotStyleVar_MajorGridSize = 15,
    ImPlotStyleVar_MinorGridSize = 16,
    ImPlotStyleVar_PlotPadding = 17,
    ImPlotStyleVar_LabelPadding = 18,
    ImPlotStyleVar_LegendPadding = 19,
    ImPlotStyleVar_LegendInnerPadding = 20,
    ImPlotStyleVar_LegendSpacing = 21,
    ImPlotStyleVar_MousePosPadding = 22,
    ImPlotStyleVar_AnnotationPadding = 23,
    ImPlotStyleVar_FitPadding = 24,
    ImPlotStyleVar_PlotDefaultSize = 25,
    ImPlotStyleVar_PlotMinSize = 26,

    // ImPlotScale
    ImPlotScale_Linear = 0,
    ImPlotScale_Time = 1,
    ImPlotScale_Log10 = 2,
    ImPlotScale_SymLog = 3,

    // ImPlotMarker
    ImPlotMarker_None = -1,
    ImPlotMarker_Circle = 0,
    ImPlotMarker_Square = 1,
    ImPlotMarker_Diamond = 2,
    ImPlotMarker_Up = 3,
    ImPlotMarker_Down = 4,
    ImPlotMarker_Left = 5,
    ImPlotMarker_Right = 6,
    ImPlotMarker_Cross = 7,
    ImPlotMarker_Plus = 8,
    ImPlotMarker_Asterisk = 9,

    // ImPlotColormap
    ImPlotColormap_Deep = 0,
    ImPlotColormap_Dark = 1,
    ImPlotColormap_Pastel = 2,
    ImPlotColormap_Paired = 3,
    ImPlotColormap_Viridis = 4,
    ImPlotColormap_Plasma = 5,
    ImPlotColormap_Hot = 6,
    ImPlotColormap_Cool = 7,
    ImPlotColormap_Pink = 8,
    ImPlotColormap_Jet = 9,
    ImPlotColormap_Twilight = 10,
    ImPlotColormap_RdBu = 11,
    ImPlotColormap_BrBG = 12,
    ImPlotColormap_PiYG = 13,
    ImPlotColormap_Spectral = 14,
    ImPlotColormap_Greys = 15,

    // ImPlotLocation
    ImPlotLocation_Center = 0,
    ImPlotLocation_North = 1 << 0,
    ImPlotLocation_South = 1 << 1,
    ImPlotLocation_West = 1 << 2,
    ImPlotLocation_East = 1 << 3,
    ImPlotLocation_NorthWest = 5,
    ImPlotLocation_NorthEast = 9,
    ImPlotLocation_SouthWest = 6,
    ImPlotLocation_SouthEast = 10,

    // ImPlotBin
    ImPlotBin_Sqrt = -1,
    ImPlotBin_Sturges = -2,
    ImPlotBin_Rice = -3,
    ImPlotBin_Scott = -4,
}
