//! C wrappers around Dear ImGui as MacroQuest builds it (`imgui.dll`, ImGui
//! 1.92, docking branch), from `shim/imgui.cpp`.
//!
//! MacroQuest calls a plugin's `OnUpdateImGui` inside its ImGui frame, with
//! its context current; these functions may only be called then. They map
//! closely onto `ImGui::*`, with C arguments: strings as `(pointer, length)`
//! (no NUL needed), vectors as separate floats, and display text never treated
//! as a printf format. Flags and enums are the `ImGui*_` constants below,
//! named as in `imgui.h`.
//!
//! # Errors and misuse
//!
//! MacroQuest's ImGui throws a C++ exception on user errors such as calling
//! `End` too many times. The wrappers catch it, so it never crosses into Rust,
//! and keep the message for [`mqemu_imgui_take_error`]. Plenty of misuse
//! isn't caught, though: a release `imgui.dll` has no asserts, so calling
//! [`mqemu_imgui_table_next_row`] outside a table, or [`mqemu_imgui_tree_pop`]
//! without a node, dereferences null or corrupts ImGui's stacks. Every
//! `begin`-style call that returns true (and `begin`/`begin_child` always)
//! must be matched by its `end`, in order. The safe API in `mq-emu` enforces
//! that; here it's the caller's job.
//!
//! Format strings (`format` arguments of sliders and drags) are printf
//! formats: they must contain exactly one conversion, of the value's type.

use core::ffi::c_char;

/// Grows a text buffer for [`mqemu_imgui_input_text`] to hold at least `size`
/// bytes (including the NUL) and returns it, possibly moved, with its capacity
/// in `*capacity`. Null leaves the buffer as it was.
pub type mqemu_imgui_resize_fn = unsafe extern "C" fn(user: *mut core::ffi::c_void, size: usize, capacity: *mut usize) -> *mut c_char;

/// Length that marks an optional string argument as absent.
pub const NO_STRING: usize = usize::MAX;

/// Declares the shim's functions, and [`Api`]: the same functions as a table
/// of pointers.
macro_rules! imgui_functions {
    ($( $(#[$meta:meta])* pub fn $name:ident($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)?; )*) => {
        unsafe extern "C" {
            $( $(#[$meta])* pub fn $name($($arg: $ty),*) $(-> $ret)?; )*
        }

        /// Every function of this module as a pointer.
        ///
        /// Calling through a table keeps `imgui.dll` out of programs that
        /// don't draw. Code calling the functions directly lands in some
        /// object file, and every program linking that object file imports
        /// `imgui.dll`, used or not; it then won't start without it. The
        /// pointers in [`API`] are only materialized where it's used, so code
        /// that takes it in a plugin (say, through a generic function only the
        /// plugin instantiates) adds the import to the plugin alone.
        #[derive(Clone, Copy)]
        pub struct Api {
            $( pub $name: unsafe extern "C" fn($($ty),*) $(-> $ret)?, )*
        }

        /// The real functions; see [`Api`].
        pub const API: Api = Api { $( $name, )* };
    };
}

imgui_functions! {
    /// Copies the first ImGui error caught since the last call (truncated,
    /// NUL-terminated) into `out` and forgets it. False if there was none.
    pub fn mqemu_imgui_take_error(out: *mut c_char, cap: usize) -> bool;

    // ----- windows -----
    /// `open` may be null (no close button). Call [`mqemu_imgui_end`] whatever this returns.
    pub fn mqemu_imgui_begin(name: *const c_char, name_len: usize, open: *mut bool, flags: i32) -> bool;
    pub fn mqemu_imgui_end();
    /// Call [`mqemu_imgui_end_child`] whatever this returns.
    pub fn mqemu_imgui_begin_child(id: *const c_char, id_len: usize, w: f32, h: f32, child_flags: i32, window_flags: i32) -> bool;
    pub fn mqemu_imgui_end_child();
    pub fn mqemu_imgui_set_next_window_pos(x: f32, y: f32, cond: i32, pivot_x: f32, pivot_y: f32);
    pub fn mqemu_imgui_set_next_window_size(w: f32, h: f32, cond: i32);
    pub fn mqemu_imgui_set_next_window_size_constraints(min_w: f32, min_h: f32, max_w: f32, max_h: f32);
    pub fn mqemu_imgui_set_next_window_collapsed(collapsed: bool, cond: i32);
    pub fn mqemu_imgui_is_window_appearing() -> bool;
    pub fn mqemu_imgui_content_region_avail(w: *mut f32, h: *mut f32);

    // ----- layout -----
    pub fn mqemu_imgui_separator();
    pub fn mqemu_imgui_separator_text(label: *const c_char, label_len: usize);
    /// ImGui's defaults: `offset_from_start_x` 0, `spacing` -1.
    pub fn mqemu_imgui_same_line(offset_from_start_x: f32, spacing: f32);
    pub fn mqemu_imgui_new_line();
    pub fn mqemu_imgui_spacing();
    pub fn mqemu_imgui_dummy(w: f32, h: f32);
    /// 0 for the style's indent spacing.
    pub fn mqemu_imgui_indent(width: f32);
    pub fn mqemu_imgui_unindent(width: f32);
    pub fn mqemu_imgui_begin_group();
    pub fn mqemu_imgui_end_group();
    pub fn mqemu_imgui_align_text_to_frame_padding();
    pub fn mqemu_imgui_set_next_item_width(width: f32);

    // ----- ID, style and state stacks -----
    pub fn mqemu_imgui_push_id_str(id: *const c_char, id_len: usize);
    pub fn mqemu_imgui_push_id_int(id: i32);
    pub fn mqemu_imgui_pop_id();
    /// `idx` must be an `ImGuiCol_` value: ImGui indexes its color array with it unchecked.
    pub fn mqemu_imgui_push_style_color(idx: i32, r: f32, g: f32, b: f32, a: f32);
    pub fn mqemu_imgui_pop_style_color(count: i32);
    /// `idx` must be an `ImGuiStyleVar_` value of a float variable.
    pub fn mqemu_imgui_push_style_var_float(idx: i32, value: f32);
    /// `idx` must be an `ImGuiStyleVar_` value of a vector variable.
    pub fn mqemu_imgui_push_style_var_vec2(idx: i32, x: f32, y: f32);
    pub fn mqemu_imgui_pop_style_var(count: i32);
    /// 0 wraps at the end of the window; < 0 doesn't wrap.
    pub fn mqemu_imgui_push_text_wrap_pos(wrap_local_pos_x: f32);
    pub fn mqemu_imgui_pop_text_wrap_pos();
    pub fn mqemu_imgui_begin_disabled(disabled: bool);
    pub fn mqemu_imgui_end_disabled();

    // ----- text -----
    /// `TextUnformatted`: the text as is.
    pub fn mqemu_imgui_text(text: *const c_char, len: usize);
    pub fn mqemu_imgui_label_text(label: *const c_char, label_len: usize, text: *const c_char, text_len: usize);
    pub fn mqemu_imgui_bullet();
    pub fn mqemu_imgui_text_link(label: *const c_char, label_len: usize) -> bool;
    pub fn mqemu_imgui_calc_text_size(text: *const c_char, len: usize, w: *mut f32, h: *mut f32);

    // ----- widgets -----
    pub fn mqemu_imgui_button(label: *const c_char, label_len: usize, w: f32, h: f32) -> bool;
    pub fn mqemu_imgui_small_button(label: *const c_char, label_len: usize) -> bool;
    pub fn mqemu_imgui_checkbox(label: *const c_char, label_len: usize, value: *mut bool) -> bool;
    pub fn mqemu_imgui_radio_button(label: *const c_char, label_len: usize, active: bool) -> bool;
    pub fn mqemu_imgui_selectable(label: *const c_char, label_len: usize, selected: bool, flags: i32, w: f32, h: f32) -> bool;
    pub fn mqemu_imgui_slider_float(
        label: *const c_char,
        label_len: usize,
        value: *mut f32,
        min: f32,
        max: f32,
        format: *const c_char,
        format_len: usize,
        flags: i32,
    ) -> bool;
    pub fn mqemu_imgui_slider_int(
        label: *const c_char,
        label_len: usize,
        value: *mut i32,
        min: i32,
        max: i32,
        format: *const c_char,
        format_len: usize,
        flags: i32,
    ) -> bool;
    pub fn mqemu_imgui_drag_float(
        label: *const c_char,
        label_len: usize,
        value: *mut f32,
        speed: f32,
        min: f32,
        max: f32,
        format: *const c_char,
        format_len: usize,
        flags: i32,
    ) -> bool;
    pub fn mqemu_imgui_drag_int(
        label: *const c_char,
        label_len: usize,
        value: *mut i32,
        speed: f32,
        min: i32,
        max: i32,
        format: *const c_char,
        format_len: usize,
        flags: i32,
    ) -> bool;
    pub fn mqemu_imgui_input_int(label: *const c_char, label_len: usize, value: *mut i32, step: i32, step_fast: i32, flags: i32) -> bool;
    pub fn mqemu_imgui_input_float(
        label: *const c_char,
        label_len: usize,
        value: *mut f32,
        step: f32,
        step_fast: f32,
        format: *const c_char,
        format_len: usize,
        flags: i32,
    ) -> bool;
    /// Edits the NUL-terminated text in `buffer` (`capacity` bytes), calling
    /// `resize(user, ...)` when ImGui needs more room. ImGui writes UTF-8.
    pub fn mqemu_imgui_input_text(
        label: *const c_char,
        label_len: usize,
        buffer: *mut c_char,
        capacity: usize,
        flags: i32,
        resize: mqemu_imgui_resize_fn,
        user: *mut core::ffi::c_void,
    ) -> bool;
    pub fn mqemu_imgui_input_text_multiline(
        label: *const c_char,
        label_len: usize,
        buffer: *mut c_char,
        capacity: usize,
        w: f32,
        h: f32,
        flags: i32,
        resize: mqemu_imgui_resize_fn,
        user: *mut core::ffi::c_void,
    ) -> bool;
    /// `rgba` points to 4 floats.
    pub fn mqemu_imgui_color_edit4(label: *const c_char, label_len: usize, rgba: *mut f32, flags: i32) -> bool;
    /// `overlay_len` [`NO_STRING`] for no overlay. ImGui's default size is (-f32::MIN_POSITIVE, 0).
    pub fn mqemu_imgui_progress_bar(fraction: f32, w: f32, h: f32, overlay: *const c_char, overlay_len: usize);
    /// `PlotLines`, or `PlotHistogram` with `histogram`. `values` holds `count`
    /// floats; `f32::MAX` scales fit the data.
    pub fn mqemu_imgui_plot_lines(
        label: *const c_char,
        label_len: usize,
        values: *const f32,
        count: i32,
        offset: i32,
        overlay: *const c_char,
        overlay_len: usize,
        scale_min: f32,
        scale_max: f32,
        w: f32,
        h: f32,
        histogram: bool,
    );

    // ----- combo boxes -----
    /// Call [`mqemu_imgui_end_combo`] only if this returns true.
    pub fn mqemu_imgui_begin_combo(label: *const c_char, label_len: usize, preview: *const c_char, preview_len: usize, flags: i32) -> bool;
    pub fn mqemu_imgui_end_combo();
    pub fn mqemu_imgui_set_item_default_focus();

    // ----- trees -----
    /// `TreeNodeEx`. Call [`mqemu_imgui_tree_pop`] only if this returns true
    /// (and `ImGuiTreeNodeFlags_NoTreePushOnOpen` isn't set).
    pub fn mqemu_imgui_tree_node(label: *const c_char, label_len: usize, flags: i32) -> bool;
    pub fn mqemu_imgui_tree_pop();
    /// Needs no pop.
    pub fn mqemu_imgui_collapsing_header(label: *const c_char, label_len: usize, flags: i32) -> bool;
    pub fn mqemu_imgui_set_next_item_open(open: bool, cond: i32);

    // ----- tabs -----
    /// Call [`mqemu_imgui_end_tab_bar`] only if this returns true.
    pub fn mqemu_imgui_begin_tab_bar(id: *const c_char, id_len: usize, flags: i32) -> bool;
    pub fn mqemu_imgui_end_tab_bar();
    /// Only between a tab bar's begin and end. Call [`mqemu_imgui_end_tab_item`] only if this returns true.
    pub fn mqemu_imgui_begin_tab_item(label: *const c_char, label_len: usize, open: *mut bool, flags: i32) -> bool;
    pub fn mqemu_imgui_end_tab_item();

    // ----- tables -----
    /// `columns` must be 1..=512. Call [`mqemu_imgui_end_table`] only if this returns true.
    pub fn mqemu_imgui_begin_table(id: *const c_char, id_len: usize, columns: i32, flags: i32, outer_w: f32, outer_h: f32, inner_width: f32) -> bool;
    pub fn mqemu_imgui_end_table();
    /// The table functions below may only be called inside a table.
    pub fn mqemu_imgui_table_setup_column(label: *const c_char, label_len: usize, flags: i32, init_width_or_weight: f32);
    pub fn mqemu_imgui_table_setup_scroll_freeze(columns: i32, rows: i32);
    pub fn mqemu_imgui_table_headers_row();
    pub fn mqemu_imgui_table_next_row(flags: i32, min_row_height: f32);
    pub fn mqemu_imgui_table_next_column() -> bool;
    /// `column` must be within the table's columns.
    pub fn mqemu_imgui_table_set_column_index(column: i32) -> bool;

    // ----- tooltips and popups -----
    pub fn mqemu_imgui_is_item_hovered(flags: i32) -> bool;
    pub fn mqemu_imgui_is_item_clicked(button: i32) -> bool;
    pub fn mqemu_imgui_is_item_active() -> bool;
    pub fn mqemu_imgui_set_tooltip(text: *const c_char, len: usize);
    /// Call [`mqemu_imgui_end_tooltip`] only if this returns true.
    pub fn mqemu_imgui_begin_tooltip() -> bool;
    /// A tooltip for the last item, if it's hovered. As [`mqemu_imgui_begin_tooltip`].
    pub fn mqemu_imgui_begin_item_tooltip() -> bool;
    pub fn mqemu_imgui_end_tooltip();
    pub fn mqemu_imgui_open_popup(id: *const c_char, id_len: usize, flags: i32);
    /// Call [`mqemu_imgui_end_popup`] only if this returns true.
    pub fn mqemu_imgui_begin_popup(id: *const c_char, id_len: usize, flags: i32) -> bool;
    pub fn mqemu_imgui_end_popup();
    pub fn mqemu_imgui_close_current_popup();

    // ----- menus -----
    /// Needs `ImGuiWindowFlags_MenuBar` on the window. Call [`mqemu_imgui_end_menu_bar`] only if this returns true.
    pub fn mqemu_imgui_begin_menu_bar() -> bool;
    pub fn mqemu_imgui_end_menu_bar();
    /// Call [`mqemu_imgui_end_menu`] only if this returns true.
    pub fn mqemu_imgui_begin_menu(label: *const c_char, label_len: usize, enabled: bool) -> bool;
    pub fn mqemu_imgui_end_menu();
    /// `shortcut_len` [`NO_STRING`] for no shortcut text.
    pub fn mqemu_imgui_menu_item(
        label: *const c_char,
        label_len: usize,
        shortcut: *const c_char,
        shortcut_len: usize,
        selected: bool,
        enabled: bool,
    ) -> bool;

    // ----- items, style and drawing -----
    /// The last item's rectangle, in screen coordinates.
    pub fn mqemu_imgui_item_rect(min_x: *mut f32, min_y: *mut f32, max_x: *mut f32, max_y: *mut f32);
    /// Writes the style's color `idx` (an `ImGuiCol_` value, unchecked) to `rgba`, 4 floats.
    pub fn mqemu_imgui_style_color(idx: i32, rgba: *mut f32);
    /// The `draw_*` functions draw on the current window's draw list, in
    /// screen coordinates, without affecting layout.
    pub fn mqemu_imgui_draw_line(x1: f32, y1: f32, x2: f32, y2: f32, r: f32, g: f32, b: f32, a: f32, thickness: f32);
    pub fn mqemu_imgui_draw_rect(
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        r: f32,
        g: f32,
        b: f32,
        a: f32,
        rounding: f32,
        thickness: f32,
        filled: bool,
    );
    pub fn mqemu_imgui_draw_circle(x: f32, y: f32, radius: f32, r: f32, g: f32, b: f32, a: f32, thickness: f32, filled: bool);
    pub fn mqemu_imgui_draw_text(x: f32, y: f32, r: f32, g: f32, b: f32, a: f32, text: *const c_char, len: usize);

    // ----- frame info -----
    pub fn mqemu_imgui_frame_count() -> i32;
    /// Seconds since ImGui started.
    pub fn mqemu_imgui_time() -> f64;
    /// ImGui's estimate of frames per second.
    pub fn mqemu_imgui_framerate() -> f32;
    /// Seconds since the last frame.
    pub fn mqemu_imgui_delta_time() -> f32;
}

/// A headless ImGui context for tests (`shim/imgui_test.cpp`): drives the
/// wrappers against MacroQuest's real `imgui.dll` without a game. ImGui's
/// context is process-global, so tests must not use it from two threads at once.
#[doc(hidden)]
pub mod testing {
    use core::ffi::c_char;

    unsafe extern "C" {
        /// Creates the context, once: 1280x800, no ini file.
        pub fn mqemu_imgui_test_init();
        /// Starts a frame with the mouse at (x, y), its left button down or not.
        pub fn mqemu_imgui_test_new_frame(mouse_x: f32, mouse_y: f32, mouse_down: bool) -> bool;
        /// Ends the frame with `ImGui::Render`, as MacroQuest does. False (and
        /// the message in `error`) if it threw, which in game pauses every
        /// plugin's ImGui; the context is then recreated next frame.
        pub fn mqemu_imgui_test_end_frame(error: *mut c_char, cap: usize) -> bool;
        pub fn mqemu_imgui_test_last_item_rect(min_x: *mut f32, min_y: *mut f32, max_x: *mut f32, max_y: *mut f32);
        /// Windows begun and not ended, counting ImGui's implicit one.
        pub fn mqemu_imgui_test_window_depth() -> i32;
        /// The current window's ID stack depth.
        pub fn mqemu_imgui_test_id_depth() -> i32;
        /// Types NUL-terminated UTF-8 `text` into the focused widget, next frame.
        pub fn mqemu_imgui_test_type(text: *const c_char);
        /// Vertices in the current window's draw list so far.
        pub fn mqemu_imgui_test_window_vertices() -> i32;
    }
}

macro_rules! imgui_constants {
    ($($name:ident = $value:expr,)*) => {
        $( pub const $name: i32 = $value; )*

        /// Every constant above with its name, for checking against `imgui.h`.
        #[doc(hidden)]
        pub const ALL_CONSTANTS: &[(&str, i32)] = &[$( (stringify!($name), $name) ),*];
    };
}

// Values from MacroQuest's contrib/imgui/imgui.h (1.92.5), without obsolete
// and deprecated names. tests/imgui_constants.rs checks them against it.
imgui_constants! {
    // ImGuiWindowFlags
    ImGuiWindowFlags_None = 0,
    ImGuiWindowFlags_NoTitleBar = 1 << 0,
    ImGuiWindowFlags_NoResize = 1 << 1,
    ImGuiWindowFlags_NoMove = 1 << 2,
    ImGuiWindowFlags_NoScrollbar = 1 << 3,
    ImGuiWindowFlags_NoScrollWithMouse = 1 << 4,
    ImGuiWindowFlags_NoCollapse = 1 << 5,
    ImGuiWindowFlags_AlwaysAutoResize = 1 << 6,
    ImGuiWindowFlags_NoBackground = 1 << 7,
    ImGuiWindowFlags_NoSavedSettings = 1 << 8,
    ImGuiWindowFlags_NoMouseInputs = 1 << 9,
    ImGuiWindowFlags_MenuBar = 1 << 10,
    ImGuiWindowFlags_HorizontalScrollbar = 1 << 11,
    ImGuiWindowFlags_NoFocusOnAppearing = 1 << 12,
    ImGuiWindowFlags_NoBringToFrontOnFocus = 1 << 13,
    ImGuiWindowFlags_AlwaysVerticalScrollbar = 1 << 14,
    ImGuiWindowFlags_AlwaysHorizontalScrollbar = 1 << 15,
    ImGuiWindowFlags_NoNavInputs = 1 << 16,
    ImGuiWindowFlags_NoNavFocus = 1 << 17,
    ImGuiWindowFlags_UnsavedDocument = 1 << 18,
    ImGuiWindowFlags_NoDocking = 1 << 19,
    ImGuiWindowFlags_NoNav = 0x30000,
    ImGuiWindowFlags_NoDecoration = 0x2b,
    ImGuiWindowFlags_NoInputs = 0x30200,
    ImGuiWindowFlags_DockNodeHost = 1 << 23,
    ImGuiWindowFlags_ChildWindow = 1 << 24,
    ImGuiWindowFlags_Tooltip = 1 << 25,
    ImGuiWindowFlags_Popup = 1 << 26,
    ImGuiWindowFlags_Modal = 1 << 27,
    ImGuiWindowFlags_ChildMenu = 1 << 28,

    // ImGuiChildFlags
    ImGuiChildFlags_None = 0,
    ImGuiChildFlags_Borders = 1 << 0,
    ImGuiChildFlags_AlwaysUseWindowPadding = 1 << 1,
    ImGuiChildFlags_ResizeX = 1 << 2,
    ImGuiChildFlags_ResizeY = 1 << 3,
    ImGuiChildFlags_AutoResizeX = 1 << 4,
    ImGuiChildFlags_AutoResizeY = 1 << 5,
    ImGuiChildFlags_AlwaysAutoResize = 1 << 6,
    ImGuiChildFlags_FrameStyle = 1 << 7,
    ImGuiChildFlags_NavFlattened = 1 << 8,

    // ImGuiCond
    ImGuiCond_None = 0,
    ImGuiCond_Always = 1,
    ImGuiCond_Once = 2,
    ImGuiCond_FirstUseEver = 4,
    ImGuiCond_Appearing = 8,

    // ImGuiTableFlags
    ImGuiTableFlags_None = 0,
    ImGuiTableFlags_Resizable = 1 << 0,
    ImGuiTableFlags_Reorderable = 1 << 1,
    ImGuiTableFlags_Hideable = 1 << 2,
    ImGuiTableFlags_Sortable = 1 << 3,
    ImGuiTableFlags_NoSavedSettings = 1 << 4,
    ImGuiTableFlags_ContextMenuInBody = 1 << 5,
    ImGuiTableFlags_RowBg = 1 << 6,
    ImGuiTableFlags_BordersInnerH = 1 << 7,
    ImGuiTableFlags_BordersOuterH = 1 << 8,
    ImGuiTableFlags_BordersInnerV = 1 << 9,
    ImGuiTableFlags_BordersOuterV = 1 << 10,
    ImGuiTableFlags_BordersH = 0x180,
    ImGuiTableFlags_BordersV = 0x600,
    ImGuiTableFlags_BordersInner = 0x280,
    ImGuiTableFlags_BordersOuter = 0x500,
    ImGuiTableFlags_Borders = 0x780,
    ImGuiTableFlags_NoBordersInBody = 1 << 11,
    ImGuiTableFlags_NoBordersInBodyUntilResize = 1 << 12,
    ImGuiTableFlags_SizingFixedFit = 1 << 13,
    ImGuiTableFlags_SizingFixedSame = 1 << 14,
    ImGuiTableFlags_SizingStretchProp = 0x6000,
    ImGuiTableFlags_SizingStretchSame = 1 << 15,
    ImGuiTableFlags_NoHostExtendX = 1 << 16,
    ImGuiTableFlags_NoHostExtendY = 1 << 17,
    ImGuiTableFlags_NoKeepColumnsVisible = 1 << 18,
    ImGuiTableFlags_PreciseWidths = 1 << 19,
    ImGuiTableFlags_NoClip = 1 << 20,
    ImGuiTableFlags_PadOuterX = 1 << 21,
    ImGuiTableFlags_NoPadOuterX = 1 << 22,
    ImGuiTableFlags_NoPadInnerX = 1 << 23,
    ImGuiTableFlags_ScrollX = 1 << 24,
    ImGuiTableFlags_ScrollY = 1 << 25,
    ImGuiTableFlags_SortMulti = 1 << 26,
    ImGuiTableFlags_SortTristate = 1 << 27,
    ImGuiTableFlags_HighlightHoveredColumn = 1 << 28,

    // ImGuiTableColumnFlags
    ImGuiTableColumnFlags_None = 0,
    ImGuiTableColumnFlags_Disabled = 1 << 0,
    ImGuiTableColumnFlags_DefaultHide = 1 << 1,
    ImGuiTableColumnFlags_DefaultSort = 1 << 2,
    ImGuiTableColumnFlags_WidthStretch = 1 << 3,
    ImGuiTableColumnFlags_WidthFixed = 1 << 4,
    ImGuiTableColumnFlags_NoResize = 1 << 5,
    ImGuiTableColumnFlags_NoReorder = 1 << 6,
    ImGuiTableColumnFlags_NoHide = 1 << 7,
    ImGuiTableColumnFlags_NoClip = 1 << 8,
    ImGuiTableColumnFlags_NoSort = 1 << 9,
    ImGuiTableColumnFlags_NoSortAscending = 1 << 10,
    ImGuiTableColumnFlags_NoSortDescending = 1 << 11,
    ImGuiTableColumnFlags_NoHeaderLabel = 1 << 12,
    ImGuiTableColumnFlags_NoHeaderWidth = 1 << 13,
    ImGuiTableColumnFlags_PreferSortAscending = 1 << 14,
    ImGuiTableColumnFlags_PreferSortDescending = 1 << 15,
    ImGuiTableColumnFlags_IndentEnable = 1 << 16,
    ImGuiTableColumnFlags_IndentDisable = 1 << 17,
    ImGuiTableColumnFlags_AngledHeader = 1 << 18,
    ImGuiTableColumnFlags_IsEnabled = 1 << 24,
    ImGuiTableColumnFlags_IsVisible = 1 << 25,
    ImGuiTableColumnFlags_IsSorted = 1 << 26,
    ImGuiTableColumnFlags_IsHovered = 1 << 27,
    ImGuiTableColumnFlags_WidthAlwaysAutoResize = 0x30,

    // ImGuiTableRowFlags
    ImGuiTableRowFlags_None = 0,
    ImGuiTableRowFlags_Headers = 1 << 0,

    // ImGuiTreeNodeFlags
    ImGuiTreeNodeFlags_None = 0,
    ImGuiTreeNodeFlags_Selected = 1 << 0,
    ImGuiTreeNodeFlags_Framed = 1 << 1,
    ImGuiTreeNodeFlags_AllowOverlap = 1 << 2,
    ImGuiTreeNodeFlags_NoTreePushOnOpen = 1 << 3,
    ImGuiTreeNodeFlags_NoAutoOpenOnLog = 1 << 4,
    ImGuiTreeNodeFlags_DefaultOpen = 1 << 5,
    ImGuiTreeNodeFlags_OpenOnDoubleClick = 1 << 6,
    ImGuiTreeNodeFlags_OpenOnArrow = 1 << 7,
    ImGuiTreeNodeFlags_Leaf = 1 << 8,
    ImGuiTreeNodeFlags_Bullet = 1 << 9,
    ImGuiTreeNodeFlags_FramePadding = 1 << 10,
    ImGuiTreeNodeFlags_SpanAvailWidth = 1 << 11,
    ImGuiTreeNodeFlags_SpanFullWidth = 1 << 12,
    ImGuiTreeNodeFlags_SpanLabelWidth = 1 << 13,
    ImGuiTreeNodeFlags_SpanAllColumns = 1 << 14,
    ImGuiTreeNodeFlags_LabelSpanAllColumns = 1 << 15,
    ImGuiTreeNodeFlags_NavLeftJumpsToParent = 1 << 17,
    ImGuiTreeNodeFlags_CollapsingHeader = 0x1a,
    ImGuiTreeNodeFlags_DrawLinesNone = 1 << 18,
    ImGuiTreeNodeFlags_DrawLinesFull = 1 << 19,
    ImGuiTreeNodeFlags_DrawLinesToNodes = 1 << 20,

    // ImGuiSelectableFlags
    ImGuiSelectableFlags_None = 0,
    ImGuiSelectableFlags_NoAutoClosePopups = 1 << 0,
    ImGuiSelectableFlags_SpanAllColumns = 1 << 1,
    ImGuiSelectableFlags_AllowDoubleClick = 1 << 2,
    ImGuiSelectableFlags_Disabled = 1 << 3,
    ImGuiSelectableFlags_AllowOverlap = 1 << 4,
    ImGuiSelectableFlags_Highlight = 1 << 5,
    ImGuiSelectableFlags_SelectOnNav = 1 << 6,

    // ImGuiTabBarFlags
    ImGuiTabBarFlags_None = 0,
    ImGuiTabBarFlags_Reorderable = 1 << 0,
    ImGuiTabBarFlags_AutoSelectNewTabs = 1 << 1,
    ImGuiTabBarFlags_TabListPopupButton = 1 << 2,
    ImGuiTabBarFlags_NoCloseWithMiddleMouseButton = 1 << 3,
    ImGuiTabBarFlags_NoTabListScrollingButtons = 1 << 4,
    ImGuiTabBarFlags_NoTooltip = 1 << 5,
    ImGuiTabBarFlags_DrawSelectedOverline = 1 << 6,
    ImGuiTabBarFlags_FittingPolicyMixed = 1 << 7,
    ImGuiTabBarFlags_FittingPolicyShrink = 1 << 8,
    ImGuiTabBarFlags_FittingPolicyScroll = 1 << 9,

    // ImGuiTabItemFlags
    ImGuiTabItemFlags_None = 0,
    ImGuiTabItemFlags_UnsavedDocument = 1 << 0,
    ImGuiTabItemFlags_SetSelected = 1 << 1,
    ImGuiTabItemFlags_NoCloseWithMiddleMouseButton = 1 << 2,
    ImGuiTabItemFlags_NoPushId = 1 << 3,
    ImGuiTabItemFlags_NoTooltip = 1 << 4,
    ImGuiTabItemFlags_NoReorder = 1 << 5,
    ImGuiTabItemFlags_Leading = 1 << 6,
    ImGuiTabItemFlags_Trailing = 1 << 7,
    ImGuiTabItemFlags_NoAssumedClosure = 1 << 8,

    // ImGuiComboFlags
    ImGuiComboFlags_None = 0,
    ImGuiComboFlags_PopupAlignLeft = 1 << 0,
    ImGuiComboFlags_HeightSmall = 1 << 1,
    ImGuiComboFlags_HeightRegular = 1 << 2,
    ImGuiComboFlags_HeightLarge = 1 << 3,
    ImGuiComboFlags_HeightLargest = 1 << 4,
    ImGuiComboFlags_NoArrowButton = 1 << 5,
    ImGuiComboFlags_NoPreview = 1 << 6,
    ImGuiComboFlags_WidthFitPreview = 1 << 7,

    // ImGuiInputTextFlags
    ImGuiInputTextFlags_None = 0,
    ImGuiInputTextFlags_CharsDecimal = 1 << 0,
    ImGuiInputTextFlags_CharsHexadecimal = 1 << 1,
    ImGuiInputTextFlags_CharsScientific = 1 << 2,
    ImGuiInputTextFlags_CharsUppercase = 1 << 3,
    ImGuiInputTextFlags_CharsNoBlank = 1 << 4,
    ImGuiInputTextFlags_AllowTabInput = 1 << 5,
    ImGuiInputTextFlags_EnterReturnsTrue = 1 << 6,
    ImGuiInputTextFlags_EscapeClearsAll = 1 << 7,
    ImGuiInputTextFlags_CtrlEnterForNewLine = 1 << 8,
    ImGuiInputTextFlags_ReadOnly = 1 << 9,
    ImGuiInputTextFlags_Password = 1 << 10,
    ImGuiInputTextFlags_AlwaysOverwrite = 1 << 11,
    ImGuiInputTextFlags_AutoSelectAll = 1 << 12,
    ImGuiInputTextFlags_ParseEmptyRefVal = 1 << 13,
    ImGuiInputTextFlags_DisplayEmptyRefVal = 1 << 14,
    ImGuiInputTextFlags_NoHorizontalScroll = 1 << 15,
    ImGuiInputTextFlags_NoUndoRedo = 1 << 16,
    ImGuiInputTextFlags_ElideLeft = 1 << 17,
    ImGuiInputTextFlags_CallbackCompletion = 1 << 18,
    ImGuiInputTextFlags_CallbackHistory = 1 << 19,
    ImGuiInputTextFlags_CallbackAlways = 1 << 20,
    ImGuiInputTextFlags_CallbackCharFilter = 1 << 21,
    ImGuiInputTextFlags_CallbackResize = 1 << 22,
    ImGuiInputTextFlags_CallbackEdit = 1 << 23,
    ImGuiInputTextFlags_WordWrap = 1 << 24,

    // ImGuiSliderFlags
    ImGuiSliderFlags_None = 0,
    ImGuiSliderFlags_Logarithmic = 1 << 5,
    ImGuiSliderFlags_NoRoundToFormat = 1 << 6,
    ImGuiSliderFlags_NoInput = 1 << 7,
    ImGuiSliderFlags_WrapAround = 1 << 8,
    ImGuiSliderFlags_ClampOnInput = 1 << 9,
    ImGuiSliderFlags_ClampZeroRange = 1 << 10,
    ImGuiSliderFlags_NoSpeedTweaks = 1 << 11,
    ImGuiSliderFlags_AlwaysClamp = 0x600,

    // ImGuiHoveredFlags
    ImGuiHoveredFlags_None = 0,
    ImGuiHoveredFlags_ChildWindows = 1 << 0,
    ImGuiHoveredFlags_RootWindow = 1 << 1,
    ImGuiHoveredFlags_AnyWindow = 1 << 2,
    ImGuiHoveredFlags_NoPopupHierarchy = 1 << 3,
    ImGuiHoveredFlags_DockHierarchy = 1 << 4,
    ImGuiHoveredFlags_AllowWhenBlockedByPopup = 1 << 5,
    ImGuiHoveredFlags_AllowWhenBlockedByActiveItem = 1 << 7,
    ImGuiHoveredFlags_AllowWhenOverlappedByItem = 1 << 8,
    ImGuiHoveredFlags_AllowWhenOverlappedByWindow = 1 << 9,
    ImGuiHoveredFlags_AllowWhenDisabled = 1 << 10,
    ImGuiHoveredFlags_NoNavOverride = 1 << 11,
    ImGuiHoveredFlags_AllowWhenOverlapped = 0x300,
    ImGuiHoveredFlags_RectOnly = 0x3a0,
    ImGuiHoveredFlags_RootAndChildWindows = 3,
    ImGuiHoveredFlags_ForTooltip = 1 << 12,
    ImGuiHoveredFlags_Stationary = 1 << 13,
    ImGuiHoveredFlags_DelayNone = 1 << 14,
    ImGuiHoveredFlags_DelayShort = 1 << 15,
    ImGuiHoveredFlags_DelayNormal = 1 << 16,
    ImGuiHoveredFlags_NoSharedDelay = 1 << 17,

    // ImGuiPopupFlags
    ImGuiPopupFlags_None = 0,
    ImGuiPopupFlags_MouseButtonLeft = 0,
    ImGuiPopupFlags_MouseButtonRight = 1 << 0,
    ImGuiPopupFlags_MouseButtonMiddle = 1 << 1,
    ImGuiPopupFlags_NoReopen = 1 << 5,
    ImGuiPopupFlags_NoOpenOverExistingPopup = 1 << 7,
    ImGuiPopupFlags_NoOpenOverItems = 1 << 8,
    ImGuiPopupFlags_AnyPopupId = 1 << 10,
    ImGuiPopupFlags_AnyPopupLevel = 1 << 11,
    ImGuiPopupFlags_AnyPopup = 0xc00,

    // ImGuiCol
    ImGuiCol_Text = 0,
    ImGuiCol_TextDisabled = 1,
    ImGuiCol_WindowBg = 2,
    ImGuiCol_ChildBg = 3,
    ImGuiCol_PopupBg = 4,
    ImGuiCol_Border = 5,
    ImGuiCol_BorderShadow = 6,
    ImGuiCol_FrameBg = 7,
    ImGuiCol_FrameBgHovered = 8,
    ImGuiCol_FrameBgActive = 9,
    ImGuiCol_TitleBg = 10,
    ImGuiCol_TitleBgActive = 11,
    ImGuiCol_TitleBgCollapsed = 12,
    ImGuiCol_MenuBarBg = 13,
    ImGuiCol_ScrollbarBg = 14,
    ImGuiCol_ScrollbarGrab = 15,
    ImGuiCol_ScrollbarGrabHovered = 16,
    ImGuiCol_ScrollbarGrabActive = 17,
    ImGuiCol_CheckMark = 18,
    ImGuiCol_SliderGrab = 19,
    ImGuiCol_SliderGrabActive = 20,
    ImGuiCol_Button = 21,
    ImGuiCol_ButtonHovered = 22,
    ImGuiCol_ButtonActive = 23,
    ImGuiCol_Header = 24,
    ImGuiCol_HeaderHovered = 25,
    ImGuiCol_HeaderActive = 26,
    ImGuiCol_Separator = 27,
    ImGuiCol_SeparatorHovered = 28,
    ImGuiCol_SeparatorActive = 29,
    ImGuiCol_ResizeGrip = 30,
    ImGuiCol_ResizeGripHovered = 31,
    ImGuiCol_ResizeGripActive = 32,
    ImGuiCol_InputTextCursor = 33,
    ImGuiCol_TabHovered = 34,
    ImGuiCol_Tab = 35,
    ImGuiCol_TabSelected = 36,
    ImGuiCol_TabSelectedOverline = 37,
    ImGuiCol_TabDimmed = 38,
    ImGuiCol_TabDimmedSelected = 39,
    ImGuiCol_TabDimmedSelectedOverline = 40,
    ImGuiCol_DockingPreview = 41,
    ImGuiCol_DockingEmptyBg = 42,
    ImGuiCol_PlotLines = 43,
    ImGuiCol_PlotLinesHovered = 44,
    ImGuiCol_PlotHistogram = 45,
    ImGuiCol_PlotHistogramHovered = 46,
    ImGuiCol_TableHeaderBg = 47,
    ImGuiCol_TableBorderStrong = 48,
    ImGuiCol_TableBorderLight = 49,
    ImGuiCol_TableRowBg = 50,
    ImGuiCol_TableRowBgAlt = 51,
    ImGuiCol_TextLink = 52,
    ImGuiCol_TextSelectedBg = 53,
    ImGuiCol_TreeLines = 54,
    ImGuiCol_DragDropTarget = 55,
    ImGuiCol_DragDropTargetBg = 56,
    ImGuiCol_UnsavedMarker = 57,
    ImGuiCol_NavCursor = 58,
    ImGuiCol_NavWindowingHighlight = 59,
    ImGuiCol_NavWindowingDimBg = 60,
    ImGuiCol_ModalWindowDimBg = 61,

    // ImGuiStyleVar
    ImGuiStyleVar_Alpha = 0,
    ImGuiStyleVar_DisabledAlpha = 1,
    ImGuiStyleVar_WindowPadding = 2,
    ImGuiStyleVar_WindowRounding = 3,
    ImGuiStyleVar_WindowBorderSize = 4,
    ImGuiStyleVar_WindowMinSize = 5,
    ImGuiStyleVar_WindowTitleAlign = 6,
    ImGuiStyleVar_ChildRounding = 7,
    ImGuiStyleVar_ChildBorderSize = 8,
    ImGuiStyleVar_PopupRounding = 9,
    ImGuiStyleVar_PopupBorderSize = 10,
    ImGuiStyleVar_FramePadding = 11,
    ImGuiStyleVar_FrameRounding = 12,
    ImGuiStyleVar_FrameBorderSize = 13,
    ImGuiStyleVar_ItemSpacing = 14,
    ImGuiStyleVar_ItemInnerSpacing = 15,
    ImGuiStyleVar_IndentSpacing = 16,
    ImGuiStyleVar_CellPadding = 17,
    ImGuiStyleVar_ScrollbarSize = 18,
    ImGuiStyleVar_ScrollbarRounding = 19,
    ImGuiStyleVar_ScrollbarPadding = 20,
    ImGuiStyleVar_GrabMinSize = 21,
    ImGuiStyleVar_GrabRounding = 22,
    ImGuiStyleVar_LayoutAlign = 23,
    ImGuiStyleVar_ImageBorderSize = 24,
    ImGuiStyleVar_TabRounding = 25,
    ImGuiStyleVar_TabBorderSize = 26,
    ImGuiStyleVar_TabMinWidthBase = 27,
    ImGuiStyleVar_TabMinWidthShrink = 28,
    ImGuiStyleVar_TabBarBorderSize = 29,
    ImGuiStyleVar_TabBarOverlineSize = 30,
    ImGuiStyleVar_TableAngledHeadersAngle = 31,
    ImGuiStyleVar_TableAngledHeadersTextAlign = 32,
    ImGuiStyleVar_TreeLinesSize = 33,
    ImGuiStyleVar_TreeLinesRounding = 34,
    ImGuiStyleVar_ButtonTextAlign = 35,
    ImGuiStyleVar_SelectableTextAlign = 36,
    ImGuiStyleVar_SeparatorTextBorderSize = 37,
    ImGuiStyleVar_SeparatorTextAlign = 38,
    ImGuiStyleVar_SeparatorTextPadding = 39,
    ImGuiStyleVar_DockingSeparatorSize = 40,

    // ImGuiColorEditFlags
    ImGuiColorEditFlags_None = 0,
    ImGuiColorEditFlags_NoAlpha = 1 << 1,
    ImGuiColorEditFlags_NoPicker = 1 << 2,
    ImGuiColorEditFlags_NoOptions = 1 << 3,
    ImGuiColorEditFlags_NoSmallPreview = 1 << 4,
    ImGuiColorEditFlags_NoInputs = 1 << 5,
    ImGuiColorEditFlags_NoTooltip = 1 << 6,
    ImGuiColorEditFlags_NoLabel = 1 << 7,
    ImGuiColorEditFlags_NoSidePreview = 1 << 8,
    ImGuiColorEditFlags_NoDragDrop = 1 << 9,
    ImGuiColorEditFlags_NoBorder = 1 << 10,
    ImGuiColorEditFlags_AlphaOpaque = 1 << 11,
    ImGuiColorEditFlags_AlphaNoBg = 1 << 12,
    ImGuiColorEditFlags_AlphaPreviewHalf = 1 << 13,
    ImGuiColorEditFlags_AlphaBar = 1 << 16,
    ImGuiColorEditFlags_HDR = 1 << 19,
    ImGuiColorEditFlags_DisplayRGB = 1 << 20,
    ImGuiColorEditFlags_DisplayHSV = 1 << 21,
    ImGuiColorEditFlags_DisplayHex = 1 << 22,
    ImGuiColorEditFlags_Uint8 = 1 << 23,
    ImGuiColorEditFlags_Float = 1 << 24,
    ImGuiColorEditFlags_PickerHueBar = 1 << 25,
    ImGuiColorEditFlags_PickerHueWheel = 1 << 26,
    ImGuiColorEditFlags_InputRGB = 1 << 27,
    ImGuiColorEditFlags_InputHSV = 1 << 28,
}
