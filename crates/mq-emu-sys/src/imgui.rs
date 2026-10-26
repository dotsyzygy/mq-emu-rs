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
//!
//! # Drawing
//!
//! Draw lists ([`ImDrawList`]) come from [`mqemu_imgui_window_draw_list`] and
//! its foreground and background siblings, and are valid until the end of the
//! frame. Points are arrays of `[x, y]` float pairs in screen coordinates;
//! colors are packed `ImU32`s (red in the low byte, alpha in the high one).
//! Textures are `ImTextureID`s: on emu (Direct3D 9) texture pointers, such as
//! [`imgui_mq::mqemu_texture_id`](crate::imgui_mq::mqemu_texture_id) returns.
//!
//! ImPlot is in [`implot`](crate::implot), and what needs MacroQuest itself
//! (game textures, EQ fonts) in [`imgui_mq`](crate::imgui_mq).

use core::ffi::{c_char, c_void};
use core::marker::{PhantomData, PhantomPinned};

/// ImGui's `ImDrawList`.
#[repr(C)]
pub struct ImDrawList {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// ImGui's `ImFont`.
#[repr(C)]
pub struct ImFont {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// ImGui's `ImGuiID`.
pub type ImGuiID = u32;

/// ImGui's `ImGuiMultiSelectIO`: a multi-select scope's requests, read with
/// [`mqemu_imgui_multi_select_io`].
#[repr(C)]
pub struct ImGuiMultiSelectIO {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// ImGui's `ImGuiSelectionUserData_Invalid`: no item.
pub const ImGuiSelectionUserData_Invalid: i64 = -1;

/// A dock node, as [`mqemu_imgui_dock_node`] describes it.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct mqemu_dock_node_info {
    /// 0 for a root node.
    pub parent: ImGuiID,
    /// Both 0 unless it's split.
    pub children: [ImGuiID; 2],
    /// The central node of its tree (the part a dock space keeps for itself), or 0.
    pub central: ImGuiID,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Its own and inherited `ImGuiDockNodeFlags_`.
    pub flags: i32,
    /// Windows docked in it.
    pub windows: i32,
    pub dock_space: bool,
    pub central_node: bool,
    pub floating: bool,
    pub visible: bool,
}

/// Grows a text buffer for [`mqemu_imgui_input_text`] to hold at least `size`
/// bytes (including the NUL) and returns it, possibly moved, with its capacity
/// in `*capacity`. Null leaves the buffer as it was.
pub type mqemu_imgui_resize_fn = unsafe extern "C" fn(user: *mut core::ffi::c_void, size: usize, capacity: *mut usize) -> *mut c_char;

/// Length that marks an optional string argument as absent.
pub const NO_STRING: usize = usize::MAX;

function_table! {
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

    // ----- items and style -----
    /// The last item's rectangle, in screen coordinates.
    pub fn mqemu_imgui_item_rect(min_x: *mut f32, min_y: *mut f32, max_x: *mut f32, max_y: *mut f32);
    /// Writes the style's color `idx` (an `ImGuiCol_` value, unchecked) to `rgba`, 4 floats.
    pub fn mqemu_imgui_style_color(idx: i32, rgba: *mut f32);
    /// ImGui's own `ColorConvertFloat4ToU32` (no style alpha applied).
    pub fn mqemu_imgui_color_convert(r: f32, g: f32, b: f32, a: f32) -> u32;

    // ----- cursor, mouse and window geometry -----
    pub fn mqemu_imgui_cursor_screen_pos(x: *mut f32, y: *mut f32);
    /// Past the content, the content grows to there (ImGui's behavior before
    /// 1.89; this one throws when the scope ends without an item after it).
    pub fn mqemu_imgui_set_cursor_screen_pos(x: f32, y: f32);
    pub fn mqemu_imgui_window_pos(x: *mut f32, y: *mut f32);
    pub fn mqemu_imgui_window_size(w: *mut f32, h: *mut f32);
    /// The main viewport's work area: the game's window, less any menu bars.
    pub fn mqemu_imgui_main_viewport_rect(x: *mut f32, y: *mut f32, w: *mut f32, h: *mut f32);
    pub fn mqemu_imgui_mouse_pos(x: *mut f32, y: *mut f32);
    /// `button` 0 left, 1 right, 2 middle.
    pub fn mqemu_imgui_is_mouse_down(button: i32) -> bool;
    pub fn mqemu_imgui_is_mouse_clicked(button: i32) -> bool;
    pub fn mqemu_imgui_is_mouse_hovering_rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> bool;
    /// An item with an ID and size but nothing drawn. Neither `w` nor `h` may
    /// be 0 (an ImGui error). `flags` are `ImGuiButtonFlags_`.
    pub fn mqemu_imgui_invisible_button(id: *const c_char, id_len: usize, w: f32, h: f32, flags: i32) -> bool;

    // ----- images -----
    /// `uv` is 2 points (top left, bottom right texture coordinates); `bg`
    /// and `tint` are RGBA, 4 floats each.
    pub fn mqemu_imgui_image(texture: *mut c_void, w: f32, h: f32, uv: *const f32, bg: *const f32, tint: *const f32);
    pub fn mqemu_imgui_image_button(
        id: *const c_char,
        id_len: usize,
        texture: *mut c_void,
        w: f32,
        h: f32,
        uv: *const f32,
        bg: *const f32,
        tint: *const f32,
    ) -> bool;

    // ----- draw lists -----
    /// The current window's draw list: drawn with the window, clipped to it.
    pub fn mqemu_imgui_window_draw_list() -> *mut ImDrawList;
    /// Drawn over every window.
    pub fn mqemu_imgui_foreground_draw_list() -> *mut ImDrawList;
    /// Drawn under every window (and over the game).
    pub fn mqemu_imgui_background_draw_list() -> *mut ImDrawList;
    /// `p`: 2 points. Shapes with a thickness are outlines when it's > 0 and filled otherwise.
    pub fn mqemu_imgui_draw_line(list: *mut ImDrawList, p: *const f32, col: u32, thickness: f32);
    /// `p`: top left and bottom right. `flags` are `ImDrawFlags_RoundCorners*`.
    pub fn mqemu_imgui_draw_rect(list: *mut ImDrawList, p: *const f32, col: u32, rounding: f32, flags: i32, thickness: f32);
    /// Filled; `cols` (4) for the top left, top right, bottom right and bottom left corners.
    pub fn mqemu_imgui_draw_rect_multicolor(list: *mut ImDrawList, p: *const f32, cols: *const u32);
    /// `count` 3 (a triangle) or 4 (a quad) points, clockwise; other counts draw nothing.
    pub fn mqemu_imgui_draw_triangle_or_quad(list: *mut ImDrawList, p: *const f32, count: i32, col: u32, thickness: f32);
    /// A circle (`segments` 0 picks a smooth count) or, with `ngon`, a regular
    /// polygon of `segments` sides (at least 3, or an ImGui error).
    pub fn mqemu_imgui_draw_circle(list: *mut ImDrawList, x: f32, y: f32, radius: f32, col: u32, segments: i32, ngon: bool, thickness: f32);
    /// `rotation` in radians.
    pub fn mqemu_imgui_draw_ellipse(
        list: *mut ImDrawList,
        x: f32,
        y: f32,
        radius_x: f32,
        radius_y: f32,
        col: u32,
        rotation: f32,
        segments: i32,
        thickness: f32,
    );
    /// A cubic (`count` 4) or quadratic (3) Bezier curve.
    pub fn mqemu_imgui_draw_bezier(list: *mut ImDrawList, p: *const f32, count: i32, col: u32, thickness: f32, segments: i32);
    /// Lines through `count` points; `ImDrawFlags_Closed` joins the last to the first.
    pub fn mqemu_imgui_draw_polyline(list: *mut ImDrawList, p: *const f32, count: i32, col: u32, flags: i32, thickness: f32);
    /// A filled polygon (clockwise, no holes or crossings); `convex` for ImGui's cheaper fill.
    pub fn mqemu_imgui_draw_poly_filled(list: *mut ImDrawList, p: *const f32, count: i32, col: u32, convex: bool);
    /// Text as is, top left at (x, y). Null `font` and 0 `size` for the
    /// current ones; `wrap_width` > 0 wraps.
    pub fn mqemu_imgui_draw_text(
        list: *mut ImDrawList,
        font: *mut ImFont,
        size: f32,
        x: f32,
        y: f32,
        col: u32,
        text: *const c_char,
        len: usize,
        wrap_width: f32,
    );
    /// `p` and `uv`: 2 points each. Rounded when `rounding` > 0.
    pub fn mqemu_imgui_draw_image(list: *mut ImDrawList, texture: *mut c_void, p: *const f32, uv: *const f32, col: u32, rounding: f32, flags: i32);
    /// `p` and `uv`: 4 points each, clockwise from the top left.
    pub fn mqemu_imgui_draw_image_quad(list: *mut ImDrawList, texture: *mut c_void, p: *const f32, uv: *const f32, col: u32);
    /// The path: points added, then stroked or filled by [`mqemu_imgui_path_finish`], which clears it.
    pub fn mqemu_imgui_path_clear(list: *mut ImDrawList);
    pub fn mqemu_imgui_path_line_to(list: *mut ImDrawList, x: f32, y: f32);
    /// An arc of a circle (`radius_y` < 0) or of an ellipse rotated by
    /// `rotation`, from angle `from` to `to` (radians, clockwise from +x).
    pub fn mqemu_imgui_path_arc_to(
        list: *mut ImDrawList,
        x: f32,
        y: f32,
        radius_x: f32,
        radius_y: f32,
        rotation: f32,
        from: f32,
        to: f32,
        segments: i32,
    );
    /// A cubic (`count` 3 more points) or quadratic (2) Bezier curve from the path's last point.
    pub fn mqemu_imgui_path_bezier_to(list: *mut ImDrawList, p: *const f32, count: i32, segments: i32);
    pub fn mqemu_imgui_path_rect(list: *mut ImDrawList, p: *const f32, rounding: f32, flags: i32);
    /// Strokes the path (`thickness` > 0) or fills it, then clears it.
    pub fn mqemu_imgui_path_finish(list: *mut ImDrawList, col: u32, flags: i32, thickness: f32, convex: bool);
    /// Clips what's drawn next to `p` (2 points) until the matching pop.
    pub fn mqemu_imgui_draw_push_clip_rect(list: *mut ImDrawList, p: *const f32, intersect: bool);
    /// Popping ImGui's own clip rectangle is caught as an ImGui error.
    pub fn mqemu_imgui_draw_pop_clip_rect(list: *mut ImDrawList);
    /// Splits the list into `count` (2 or more) channels, merged in order.
    /// False (nothing split) if it's split already or `count` is too small.
    pub fn mqemu_imgui_draw_channels_split(list: *mut ImDrawList, count: i32) -> bool;
    /// False (nothing changed) unless `channel` is one of the split channels.
    pub fn mqemu_imgui_draw_channels_set_current(list: *mut ImDrawList, channel: i32) -> bool;
    pub fn mqemu_imgui_draw_channels_merge(list: *mut ImDrawList);

    // ----- drag and drop -----
    /// After the item to drag. Call [`mqemu_imgui_end_drag_drop_source`] only if this returns true.
    pub fn mqemu_imgui_begin_drag_drop_source(flags: i32) -> bool;
    /// Copies the payload. `type` is 1 to 32 bytes, without NUL, not starting
    /// with `_` (an ImGui error otherwise). Only inside a drag source.
    pub fn mqemu_imgui_set_drag_drop_payload(r#type: *const c_char, type_len: usize, data: *const c_void, size: usize, cond: i32) -> bool;
    pub fn mqemu_imgui_end_drag_drop_source();
    /// After the item to drop on. Call [`mqemu_imgui_end_drag_drop_target`] only if this returns true.
    pub fn mqemu_imgui_begin_drag_drop_target() -> bool;
    /// A payload of `type` over the target: its data (ImGui's, valid this
    /// frame), and whether it's hovering or dropped. Only inside a drop target.
    pub fn mqemu_imgui_accept_drag_drop_payload(
        r#type: *const c_char,
        type_len: usize,
        flags: i32,
        data: *mut *const c_void,
        size: *mut usize,
        preview: *mut bool,
        delivery: *mut bool,
    ) -> bool;
    pub fn mqemu_imgui_end_drag_drop_target();
    /// The payload being dragged anywhere, its type copied to `type` (NUL-terminated, truncated to `cap`).
    pub fn mqemu_imgui_drag_drop_payload(
        r#type: *mut c_char,
        cap: usize,
        data: *mut *const c_void,
        size: *mut usize,
        preview: *mut bool,
        delivery: *mut bool,
    ) -> bool;

    // ----- fonts -----
    /// MacroQuest's fonts: 0 default, 1 console, 2 large text. Null if absent.
    pub fn mqemu_imgui_mq_font(which: i32) -> *mut ImFont;
    /// Null `font` keeps the current one, `size` 0 the current size (unscaled
    /// pixels; negative is an ImGui error). Pop it with [`mqemu_imgui_pop_font`].
    pub fn mqemu_imgui_push_font(font: *mut ImFont, size: f32);
    pub fn mqemu_imgui_pop_font();
    pub fn mqemu_imgui_current_font() -> *mut ImFont;
    /// The size `font` (not null) was added at, its natural size.
    pub fn mqemu_imgui_font_own_size(font: *const ImFont) -> f32;
    /// The current font size in pixels, after global scaling.
    pub fn mqemu_imgui_font_size() -> f32;
    /// The current font size before global scaling, as [`mqemu_imgui_push_font`] takes it.
    pub fn mqemu_imgui_font_size_base() -> f32;
    /// Adds a TrueType/OpenType file to the font atlas. Null if it can't be read.
    pub fn mqemu_imgui_add_font_file(path: *const c_char, path_len: usize, size: f32) -> *mut ImFont;
    /// Removes a font [`mqemu_imgui_add_font_file`] added. Never while it's in use.
    pub fn mqemu_imgui_remove_font(font: *mut ImFont);

    // ----- docking -----
    /// Whether docking is on (a MacroQuest overlay setting).
    pub fn mqemu_imgui_docking_enabled() -> bool;
    /// The ID `id` gets on the current ID stack.
    pub fn mqemu_imgui_get_id(id: *const c_char, id_len: usize) -> ImGuiID;
    /// A dock space in the current window. `id` 0 is an ImGui error. Returns 0 when docking is off.
    pub fn mqemu_imgui_dock_space(id: ImGuiID, w: f32, h: f32, flags: i32) -> ImGuiID;
    /// A dock space over the main viewport (`id` 0 picks one).
    pub fn mqemu_imgui_dock_space_over_viewport(id: ImGuiID, flags: i32) -> ImGuiID;
    pub fn mqemu_imgui_set_next_window_dock_id(id: ImGuiID, cond: i32);
    pub fn mqemu_imgui_window_dock_id() -> ImGuiID;
    pub fn mqemu_imgui_is_window_docked() -> bool;

    // ----- docking: the dock builder -----
    // ImGui's internal DockBuilder* functions. Node 0 (ImGui's "every node")
    // is an ImGui error here, as is changing a node tree from inside a window
    // docked in it (ImGui would free nodes the window uses).
    /// A new node replacing any node `id` (0 picks an ID), or with
    /// `dock_space` a dock space node (`id` not 0). `flags` must not include
    /// `DockSpace` or `CentralNode`. Its ID; 0 for a dock space with docking off.
    pub fn mqemu_imgui_dock_builder_add_node(id: ImGuiID, flags: i32, dock_space: bool) -> ImGuiID;
    /// Removes the node and the nodes in it, undocking their windows.
    pub fn mqemu_imgui_dock_builder_remove_node(id: ImGuiID);
    /// Removes the nodes split from root node `id`, moving their windows into it.
    pub fn mqemu_imgui_dock_builder_remove_node_child_nodes(id: ImGuiID);
    pub fn mqemu_imgui_dock_builder_remove_node_docked_windows(id: ImGuiID, clear_settings_refs: bool);
    pub fn mqemu_imgui_dock_builder_set_node_pos(id: ImGuiID, x: f32, y: f32);
    /// Both sizes > 0. Set it before splitting a new node.
    pub fn mqemu_imgui_dock_builder_set_node_size(id: ImGuiID, w: f32, h: f32);
    /// Splits leaf node `id`: `ratio` of it towards `dir` (an `ImGuiDir_`, not
    /// None). Writes both new IDs; returns the one towards `dir`, 0 on an error.
    pub fn mqemu_imgui_dock_builder_split_node(id: ImGuiID, dir: i32, ratio: f32, at_dir: *mut ImGuiID, opposite: *mut ImGuiID) -> ImGuiID;
    /// From its next `Begin`; `id` 0 undocks it.
    pub fn mqemu_imgui_dock_builder_dock_window(name: *const c_char, name_len: usize, id: ImGuiID);
    pub fn mqemu_imgui_dock_builder_copy_window_settings(src: *const c_char, src_len: usize, dst: *const c_char, dst_len: usize);
    /// Replaces `dst` with a copy of `src`. Writes up to `cap` (source, copy)
    /// pairs to `pairs` (2 IDs each); returns how many there are, -1 on an error.
    pub fn mqemu_imgui_dock_builder_copy_node(src: ImGuiID, dst: ImGuiID, pairs: *mut ImGuiID, cap: i32) -> i32;
    /// `windows`/`window_lens`: `pair_count` (source, destination) window title pairs, 2 strings each.
    pub fn mqemu_imgui_dock_builder_copy_dock_space(src: ImGuiID, dst: ImGuiID, windows: *const *const c_char, window_lens: *const usize, pair_count: i32);
    pub fn mqemu_imgui_dock_builder_finish(id: ImGuiID);
    /// False if there's no node `id`.
    pub fn mqemu_imgui_dock_node(id: ImGuiID, out: *mut mqemu_dock_node_info) -> bool;
    /// The title of window `index` docked in node `id` (NUL-terminated, truncated to `cap`).
    pub fn mqemu_imgui_dock_node_window(id: ImGuiID, index: i32, out: *mut c_char, cap: usize) -> bool;

    // ----- multi-select -----
    /// Always end it, unless this returns null (an error). The IO holds the
    /// requests to apply now; read it before anything else begins a scope.
    /// -1 for an unknown selection size or item count.
    pub fn mqemu_imgui_begin_multi_select(flags: i32, selection_size: i32, items_count: i32) -> *mut ImGuiMultiSelectIO;
    /// The requests to apply afterwards (null on an error), valid until a scope next begins.
    pub fn mqemu_imgui_end_multi_select() -> *mut ImGuiMultiSelectIO;
    /// The IO's fields (items [`ImGuiSelectionUserData_Invalid`] for none); returns its request count.
    pub fn mqemu_imgui_multi_select_io(
        io: *const ImGuiMultiSelectIO,
        range_src_item: *mut i64,
        nav_id_item: *mut i64,
        nav_id_selected: *mut bool,
        items_count: *mut i32,
    ) -> i32;
    /// Request `index` (below the count): an `ImGuiSelectionRequestType_`,
    /// and for `SetRange` its inclusive range and direction.
    pub fn mqemu_imgui_multi_select_request(
        io: *const ImGuiMultiSelectIO,
        index: i32,
        r#type: *mut i32,
        selected: *mut bool,
        direction: *mut i32,
        first: *mut i64,
        last: *mut i64,
    );
    /// In the current scope (an ImGui error outside one).
    pub fn mqemu_imgui_multi_select_reset_range_src();
    pub fn mqemu_imgui_set_next_item_selection_user_data(data: i64);
    /// Only in a scope (an ImGui error outside one).
    pub fn mqemu_imgui_is_item_toggled_selection() -> bool;

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
        /// ImGui's own `SetCursorScreenPos`, which throws later if nothing follows it.
        pub fn mqemu_imgui_test_set_cursor_screen_pos_raw(x: f32, y: f32);
        /// Vertices in `list` so far.
        pub fn mqemu_imgui_test_list_vertices(list: *mut super::ImDrawList) -> i32;
        /// Whether any of `list`'s draw commands so far uses `texture`.
        pub fn mqemu_imgui_test_list_uses_texture(list: *mut super::ImDrawList, texture: *mut core::ffi::c_void) -> bool;
        /// `list`'s clip rectangle stack depth, path length and channel count.
        pub fn mqemu_imgui_test_list_state(list: *mut super::ImDrawList, clip_depth: *mut i32, path_points: *mut i32, channels: *mut i32);
        /// The font stack's depth.
        pub fn mqemu_imgui_test_font_depth() -> i32;
        /// Whether ImPlot is in a plot or subplots, and its style and colormap stack depths.
        pub fn mqemu_implot_test_state(in_plot: *mut bool, in_subplots: *mut bool, style_depth: *mut i32, colormap_depth: *mut i32);
        /// A fake texture ID, for drawing images headlessly.
        pub fn mqemu_imgui_test_texture() -> *mut core::ffi::c_void;
        /// The size of the payload being dragged, or -1.
        pub fn mqemu_imgui_test_payload_size() -> i32;
        /// Turns docking on or off, as MacroQuest's overlay setting does.
        pub fn mqemu_imgui_test_set_docking(enabled: bool);
        /// Holds ctrl and shift (or not), from the next frame.
        pub fn mqemu_imgui_test_modifiers(ctrl: bool, shift: bool);
        /// Presses or releases the key for `letter` (b'A' to b'Z'), from the next frame.
        pub fn mqemu_imgui_test_letter_key(letter: c_char, down: bool);
        /// ImPlot's own `EndPlot`, which keeps the plot's formatters and transforms.
        pub fn mqemu_implot_test_end_plot_raw();
        /// Whether `axis` of the plot titled `title` (NUL-terminated, in the
        /// current window) still has a formatter or a transform.
        pub fn mqemu_implot_test_axis_callbacks(title: *const c_char, axis: i32, formatter: *mut bool, transform: *mut bool);
    }
}

macro_rules! imgui_constants {
    ($list:ident, $header:literal; $($name:ident = $value:expr,)*) => {
        $( pub const $name: i32 = $value; )*

        #[doc = concat!("Every constant in this group with its name, for checking against `", $header, "`.")]
        #[doc(hidden)]
        pub const $list: &[(&str, i32)] = &[$( (stringify!($name), $name) ),*];
    };
}

// Values from MacroQuest's contrib/imgui/imgui.h (1.92.5), without obsolete
// and deprecated names. tests/imgui_constants.rs checks them against it.
imgui_constants! {
    ALL_CONSTANTS, "imgui.h";
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

    // ImGuiButtonFlags
    ImGuiButtonFlags_None = 0,
    ImGuiButtonFlags_MouseButtonLeft = 1 << 0,
    ImGuiButtonFlags_MouseButtonRight = 1 << 1,
    ImGuiButtonFlags_MouseButtonMiddle = 1 << 2,
    ImGuiButtonFlags_EnableNav = 1 << 3,

    // ImGuiMouseButton
    ImGuiMouseButton_Left = 0,
    ImGuiMouseButton_Right = 1,
    ImGuiMouseButton_Middle = 2,

    // ImGuiDragDropFlags
    ImGuiDragDropFlags_None = 0,
    ImGuiDragDropFlags_SourceNoPreviewTooltip = 1 << 0,
    ImGuiDragDropFlags_SourceNoDisableHover = 1 << 1,
    ImGuiDragDropFlags_SourceNoHoldToOpenOthers = 1 << 2,
    ImGuiDragDropFlags_SourceAllowNullID = 1 << 3,
    ImGuiDragDropFlags_SourceExtern = 1 << 4,
    ImGuiDragDropFlags_PayloadAutoExpire = 1 << 5,
    ImGuiDragDropFlags_PayloadNoCrossContext = 1 << 6,
    ImGuiDragDropFlags_PayloadNoCrossProcess = 1 << 7,
    ImGuiDragDropFlags_AcceptBeforeDelivery = 1 << 10,
    ImGuiDragDropFlags_AcceptNoDrawDefaultRect = 1 << 11,
    ImGuiDragDropFlags_AcceptNoPreviewTooltip = 1 << 12,
    ImGuiDragDropFlags_AcceptDrawAsHovered = 1 << 13,
    ImGuiDragDropFlags_AcceptPeekOnly = 0xc00,

    // ImGuiDockNodeFlags
    ImGuiDockNodeFlags_None = 0,
    ImGuiDockNodeFlags_KeepAliveOnly = 1 << 0,
    ImGuiDockNodeFlags_NoDockingOverCentralNode = 1 << 2,
    ImGuiDockNodeFlags_PassthruCentralNode = 1 << 3,
    ImGuiDockNodeFlags_NoDockingSplit = 1 << 4,
    ImGuiDockNodeFlags_NoResize = 1 << 5,
    ImGuiDockNodeFlags_AutoHideTabBar = 1 << 6,
    ImGuiDockNodeFlags_NoUndocking = 1 << 7,

    // ImGuiDir
    ImGuiDir_None = -1,
    ImGuiDir_Left = 0,
    ImGuiDir_Right = 1,
    ImGuiDir_Up = 2,
    ImGuiDir_Down = 3,

    // ImGuiMultiSelectFlags
    ImGuiMultiSelectFlags_None = 0,
    ImGuiMultiSelectFlags_SingleSelect = 1 << 0,
    ImGuiMultiSelectFlags_NoSelectAll = 1 << 1,
    ImGuiMultiSelectFlags_NoRangeSelect = 1 << 2,
    ImGuiMultiSelectFlags_NoAutoSelect = 1 << 3,
    ImGuiMultiSelectFlags_NoAutoClear = 1 << 4,
    ImGuiMultiSelectFlags_NoAutoClearOnReselect = 1 << 5,
    ImGuiMultiSelectFlags_BoxSelect1d = 1 << 6,
    ImGuiMultiSelectFlags_BoxSelect2d = 1 << 7,
    ImGuiMultiSelectFlags_BoxSelectNoScroll = 1 << 8,
    ImGuiMultiSelectFlags_ClearOnEscape = 1 << 9,
    ImGuiMultiSelectFlags_ClearOnClickVoid = 1 << 10,
    ImGuiMultiSelectFlags_ScopeWindow = 1 << 11,
    ImGuiMultiSelectFlags_ScopeRect = 1 << 12,
    ImGuiMultiSelectFlags_SelectOnClick = 1 << 13,
    ImGuiMultiSelectFlags_SelectOnClickRelease = 1 << 14,
    ImGuiMultiSelectFlags_NavWrapX = 1 << 16,
    ImGuiMultiSelectFlags_NoSelectOnRightClick = 1 << 17,

    // ImGuiSelectionRequestType
    ImGuiSelectionRequestType_None = 0,
    ImGuiSelectionRequestType_SetAll = 1,
    ImGuiSelectionRequestType_SetRange = 2,

    // ImGuiMod (the modifier keys of ImGuiKey)
    ImGuiMod_None = 0,
    ImGuiMod_Ctrl = 1 << 12,
    ImGuiMod_Shift = 1 << 13,
    ImGuiMod_Alt = 1 << 14,
    ImGuiMod_Super = 1 << 15,

    // ImDrawFlags
    ImDrawFlags_None = 0,
    ImDrawFlags_Closed = 1 << 0,
    ImDrawFlags_RoundCornersTopLeft = 1 << 4,
    ImDrawFlags_RoundCornersTopRight = 1 << 5,
    ImDrawFlags_RoundCornersBottomLeft = 1 << 6,
    ImDrawFlags_RoundCornersBottomRight = 1 << 7,
    ImDrawFlags_RoundCornersNone = 1 << 8,
    ImDrawFlags_RoundCornersTop = 0x30,
    ImDrawFlags_RoundCornersBottom = 0xc0,
    ImDrawFlags_RoundCornersLeft = 0x50,
    ImDrawFlags_RoundCornersRight = 0xa0,
    ImDrawFlags_RoundCornersAll = 0xf0,

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

// ImGui's internal dock node flags, from imgui_internal.h
// (`ImGuiDockNodeFlagsPrivate_`), for nodes made with the dock builder.
imgui_constants! {
    INTERNAL_CONSTANTS, "imgui_internal.h";
    ImGuiDockNodeFlags_DockSpace = 1 << 10,
    ImGuiDockNodeFlags_CentralNode = 1 << 11,
    ImGuiDockNodeFlags_NoTabBar = 1 << 12,
    ImGuiDockNodeFlags_HiddenTabBar = 1 << 13,
    ImGuiDockNodeFlags_NoWindowMenuButton = 1 << 14,
    ImGuiDockNodeFlags_NoCloseButton = 1 << 15,
    ImGuiDockNodeFlags_NoResizeX = 1 << 16,
    ImGuiDockNodeFlags_NoResizeY = 1 << 17,
    ImGuiDockNodeFlags_DockedWindowsInFocusRoute = 1 << 18,
    ImGuiDockNodeFlags_NoDockingSplitOther = 1 << 19,
    ImGuiDockNodeFlags_NoDockingOverMe = 1 << 20,
    ImGuiDockNodeFlags_NoDockingOverOther = 1 << 21,
    ImGuiDockNodeFlags_NoDockingOverEmpty = 1 << 22,
    ImGuiDockNodeFlags_NoDocking = 0x780010,
}
