//! Draw lists: shapes, text, images and paths drawn without affecting layout.

use std::marker::PhantomData;
use std::ptr;

use mq_emu_sys::imgui as sys;
use mq_emu_sys::imgui_mq as mq_sys;

use super::{Color, Font, OnDrop, TextureAnimation, TextureId, Ui, s};

/// A point or size, `[x, y]` in pixels.
pub type Vec2 = [f32; 2];

/// Packs a color as ImGui's draw lists take it (`ImGui::ColorConvertFloat4ToU32`):
/// each component clamped to 0..=1 and rounded to a byte, red lowest.
pub fn pack_color(color: Color) -> u32 {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    byte(color[0]) | byte(color[1]) << 8 | byte(color[2]) << 16 | byte(color[3]) << 24
}

flags! {
    /// Which corners of a rectangle a rounding applies to (`ImDrawFlags_RoundCorners*`).
    /// [`Corners::NONE`] here means ImGui's default: all of them.
    Corners {
        TOP_LEFT = ImDrawFlags_RoundCornersTopLeft,
        TOP_RIGHT = ImDrawFlags_RoundCornersTopRight,
        BOTTOM_LEFT = ImDrawFlags_RoundCornersBottomLeft,
        BOTTOM_RIGHT = ImDrawFlags_RoundCornersBottomRight,
        /// Square corners even with a rounding.
        SQUARE = ImDrawFlags_RoundCornersNone,
        TOP = ImDrawFlags_RoundCornersTop,
        BOTTOM = ImDrawFlags_RoundCornersBottom,
        LEFT = ImDrawFlags_RoundCornersLeft,
        RIGHT = ImDrawFlags_RoundCornersRight,
        ALL = ImDrawFlags_RoundCornersAll,
    }
}

/// Draws on one of ImGui's draw lists, in screen coordinates, without affecting
/// layout. From [`Ui::draw_list`] (the current window), [`Ui::foreground_draw_list`],
/// [`Ui::background_draw_list`] or [`Plot::draw`](super::Plot::draw).
///
/// Filled shapes should list their points clockwise: ImGui's anti-aliasing
/// assumes it.
pub struct DrawList<'ui> {
    api: &'static sys::Api,
    mq: &'static mq_sys::Api,
    list: *mut sys::ImDrawList,
    _ui: PhantomData<&'ui Ui>,
}

impl<'ui> DrawList<'ui> {
    pub(super) fn new(ui: &'ui Ui, list: *mut sys::ImDrawList) -> DrawList<'ui> {
        DrawList { api: ui.api, mq: ui.mq, list, _ui: PhantomData }
    }

    #[cfg(test)]
    pub(crate) fn raw_list(&self) -> *mut sys::ImDrawList {
        self.list
    }

    /// The list, unless ImGui gave none (it reported an error instead).
    pub(super) fn raw(&self) -> Option<*mut sys::ImDrawList> {
        (!self.list.is_null()).then_some(self.list)
    }

    pub(super) fn mq_api(&self) -> &'static mq_sys::Api {
        self.mq
    }

    /// Calls `f` with the list, unless ImGui gave none (it reported an error instead).
    fn with(&self, f: impl FnOnce(&sys::Api, *mut sys::ImDrawList)) {
        if !self.list.is_null() {
            f(self.api, self.list);
        }
    }

    pub fn add_line(&self, from: Vec2, to: Vec2, color: Color, thickness: f32) {
        let p = [from, to];
        self.with(|api, list| unsafe { (api.mqemu_imgui_draw_line)(list, p.as_ptr().cast(), pack_color(color), thickness) });
    }

    /// A rectangle's outline from `min` (top left) to `max`.
    pub fn add_rect(&self, min: Vec2, max: Vec2, color: Color, rounding: f32, thickness: f32) {
        self.rect(min, max, color, rounding, Corners::NONE, thickness.max(f32::MIN_POSITIVE));
    }

    pub fn add_rect_filled(&self, min: Vec2, max: Vec2, color: Color, rounding: f32) {
        self.rect(min, max, color, rounding, Corners::NONE, 0.0);
    }

    /// [`DrawList::add_rect`] rounding only some corners.
    pub fn add_rect_corners(&self, min: Vec2, max: Vec2, color: Color, rounding: f32, corners: Corners, thickness: f32) {
        self.rect(min, max, color, rounding, corners, thickness.max(f32::MIN_POSITIVE));
    }

    pub fn add_rect_filled_corners(&self, min: Vec2, max: Vec2, color: Color, rounding: f32, corners: Corners) {
        self.rect(min, max, color, rounding, corners, 0.0);
    }

    fn rect(&self, min: Vec2, max: Vec2, color: Color, rounding: f32, corners: Corners, thickness: f32) {
        let p = [min, max];
        self.with(|api, list| unsafe {
            (api.mqemu_imgui_draw_rect)(list, p.as_ptr().cast(), pack_color(color), rounding, corners.bits(), thickness)
        });
    }

    /// A filled rectangle shaded between a color per corner: top left, top
    /// right, bottom right, bottom left.
    pub fn add_rect_filled_multicolor(&self, min: Vec2, max: Vec2, colors: [Color; 4]) {
        let (p, colors) = ([min, max], colors.map(pack_color));
        self.with(|api, list| unsafe { (api.mqemu_imgui_draw_rect_multicolor)(list, p.as_ptr().cast(), colors.as_ptr()) });
    }

    pub fn add_triangle(&self, points: [Vec2; 3], color: Color, thickness: f32) {
        self.polygon(&points, color, thickness.max(f32::MIN_POSITIVE));
    }

    pub fn add_triangle_filled(&self, points: [Vec2; 3], color: Color) {
        self.polygon(&points, color, 0.0);
    }

    pub fn add_quad(&self, points: [Vec2; 4], color: Color, thickness: f32) {
        self.polygon(&points, color, thickness.max(f32::MIN_POSITIVE));
    }

    pub fn add_quad_filled(&self, points: [Vec2; 4], color: Color) {
        self.polygon(&points, color, 0.0);
    }

    fn polygon(&self, points: &[Vec2], color: Color, thickness: f32) {
        self.with(|api, list| unsafe {
            (api.mqemu_imgui_draw_triangle_or_quad)(list, points.as_ptr().cast(), points.len() as i32, pack_color(color), thickness)
        });
    }

    pub fn add_circle(&self, center: Vec2, radius: f32, color: Color, thickness: f32) {
        self.circle(center, radius, color, 0, false, thickness.max(f32::MIN_POSITIVE));
    }

    pub fn add_circle_filled(&self, center: Vec2, radius: f32, color: Color) {
        self.circle(center, radius, color, 0, false, 0.0);
    }

    /// A regular polygon with `sides` sides (at least 3), its corners on the circle.
    ///
    /// # Panics
    /// With fewer than 3 sides.
    pub fn add_ngon(&self, center: Vec2, radius: f32, color: Color, sides: u32, thickness: f32) {
        self.circle(center, radius, color, ngon_sides(sides), true, thickness.max(f32::MIN_POSITIVE));
    }

    /// # Panics
    /// With fewer than 3 sides.
    pub fn add_ngon_filled(&self, center: Vec2, radius: f32, color: Color, sides: u32) {
        self.circle(center, radius, color, ngon_sides(sides), true, 0.0);
    }

    fn circle(&self, center: Vec2, radius: f32, color: Color, segments: i32, ngon: bool, thickness: f32) {
        let [x, y] = center;
        self.with(|api, list| unsafe { (api.mqemu_imgui_draw_circle)(list, x, y, radius, pack_color(color), segments, ngon, thickness) });
    }

    /// An ellipse with `radii` `[x, y]`, rotated by `rotation` radians.
    pub fn add_ellipse(&self, center: Vec2, radii: Vec2, color: Color, rotation: f32, thickness: f32) {
        self.ellipse(center, radii, color, rotation, thickness.max(f32::MIN_POSITIVE));
    }

    pub fn add_ellipse_filled(&self, center: Vec2, radii: Vec2, color: Color, rotation: f32) {
        self.ellipse(center, radii, color, rotation, 0.0);
    }

    fn ellipse(&self, [x, y]: Vec2, [rx, ry]: Vec2, color: Color, rotation: f32, thickness: f32) {
        self.with(|api, list| unsafe { (api.mqemu_imgui_draw_ellipse)(list, x, y, rx, ry, pack_color(color), rotation, 0, thickness) });
    }

    /// A cubic Bezier curve from `points[0]` to `points[3]`, pulled toward the middle two.
    pub fn add_bezier_cubic(&self, points: [Vec2; 4], color: Color, thickness: f32) {
        self.bezier(&points, color, thickness);
    }

    /// A quadratic Bezier curve from `points[0]` to `points[2]`, pulled toward `points[1]`.
    pub fn add_bezier_quadratic(&self, points: [Vec2; 3], color: Color, thickness: f32) {
        self.bezier(&points, color, thickness);
    }

    fn bezier(&self, points: &[Vec2], color: Color, thickness: f32) {
        self.with(|api, list| unsafe {
            (api.mqemu_imgui_draw_bezier)(list, points.as_ptr().cast(), points.len() as i32, pack_color(color), thickness, 0)
        });
    }

    /// Lines through `points`, back to the first if `closed`.
    pub fn add_polyline(&self, points: &[Vec2], color: Color, closed: bool, thickness: f32) {
        let flags = if closed { sys::ImDrawFlags_Closed } else { 0 };
        self.with(|api, list| unsafe {
            (api.mqemu_imgui_draw_polyline)(list, points.as_ptr().cast(), count(points), pack_color(color), flags, thickness)
        });
    }

    /// A filled convex polygon (the cheap fill; concave ones come out wrong).
    pub fn add_convex_poly_filled(&self, points: &[Vec2], color: Color) {
        self.poly_filled(points, color, true);
    }

    /// A filled polygon of any shape without holes or self-crossings.
    pub fn add_concave_poly_filled(&self, points: &[Vec2], color: Color) {
        self.poly_filled(points, color, false);
    }

    fn poly_filled(&self, points: &[Vec2], color: Color, convex: bool) {
        self.with(|api, list| unsafe {
            (api.mqemu_imgui_draw_poly_filled)(list, points.as_ptr().cast(), count(points), pack_color(color), convex)
        });
    }

    /// `text` with its top left at `pos`, as is (no formatting), in the current font.
    pub fn add_text(&self, pos: Vec2, color: Color, text: impl AsRef<str>) {
        self.text(ptr::null_mut(), 0.0, pos, color, text.as_ref(), 0.0);
    }

    /// [`DrawList::add_text`] in `font` at `size` pixels (0 for the current
    /// size), wrapped at `wrap_width` pixels if that's above 0.
    pub fn add_text_with(&self, font: Font, size: f32, pos: Vec2, color: Color, text: impl AsRef<str>, wrap_width: f32) {
        self.text(font.raw(self.api, self.mq), size.max(0.0), pos, color, text.as_ref(), wrap_width);
    }

    fn text(&self, font: *mut sys::ImFont, size: f32, [x, y]: Vec2, color: Color, text: &str, wrap_width: f32) {
        let (t, n) = s(text);
        self.with(|api, list| unsafe { (api.mqemu_imgui_draw_text)(list, font, size, x, y, pack_color(color), t, n, wrap_width) });
    }

    /// The whole texture from `min` to `max`.
    pub fn add_image(&self, texture: TextureId<'_>, min: Vec2, max: Vec2) {
        self.add_image_with(texture, min, max, [[0.0, 0.0], [1.0, 1.0]], [1.0; 4], 0.0, Corners::NONE);
    }

    /// Part of the texture (`uv`: top-left and bottom-right texture
    /// coordinates, 0 to 1), tinted, with corners rounded by `rounding`.
    #[allow(clippy::too_many_arguments)]
    pub fn add_image_with(&self, texture: TextureId<'_>, min: Vec2, max: Vec2, uv: [Vec2; 2], tint: Color, rounding: f32, corners: Corners) {
        let p = [min, max];
        self.with(|api, list| unsafe {
            (api.mqemu_imgui_draw_image)(list, texture.raw(), p.as_ptr().cast(), uv.as_ptr().cast(), pack_color(tint), rounding, corners.bits())
        });
    }

    /// The texture on any four corners (clockwise from the top left), with a
    /// texture coordinate for each.
    pub fn add_image_quad(&self, texture: TextureId<'_>, corners: [Vec2; 4], uv: [Vec2; 4], tint: Color) {
        self.with(|api, list| unsafe {
            (api.mqemu_imgui_draw_image_quad)(list, texture.raw(), corners.as_ptr().cast(), uv.as_ptr().cast(), pack_color(tint))
        });
    }

    /// One of the game's UI textures (an item or spell icon, say) with its
    /// top left at `pos`, `size` pixels (None for its own size), tinted, with
    /// a border unless `border` is None. False if nothing was drawn: the
    /// animation is out of date (see [`TextureAnimation`]) or has no cell set.
    pub fn add_texture_animation(&self, animation: &TextureAnimation, pos: Vec2, size: Option<Vec2>, tint: Color, border: Option<Color>) -> bool {
        let Some(raw) = animation.current() else { return false };
        if self.list.is_null() {
            return false;
        }
        let [w, h] = size.map_or([0, 0], |[w, h]| [w.round() as i32, h.round() as i32]);
        let border = border.map_or(0, pack_color);
        unsafe { (self.mq.mqemu_imgui_draw_texture_animation)(self.list, raw, pos[0].round() as i32, pos[1].round() as i32, w, h, pack_color(tint), border) }
    }

    /// Builds a path of lines, arcs and curves, then strokes or fills it.
    ///
    /// ```no_run
    /// # fn draw(ui: &mut mq_emu::Ui) {
    /// let draw = ui.draw_list();
    /// let mut path = draw.path();
    /// path.line_to([10.0, 10.0]).arc_to([50.0, 10.0], 20.0, std::f32::consts::PI, 0.0).line_to([90.0, 10.0]);
    /// path.stroke([1.0, 1.0, 0.0, 1.0], false, 2.0);
    /// # }
    /// ```
    ///
    /// The path lives in the draw list: build one at a time per list.
    pub fn path(&self) -> Path<'_, 'ui> {
        if !self.list.is_null() {
            unsafe { (self.api.mqemu_imgui_path_clear)(self.list) };
        }
        Path { draw: self, finished: false }
    }

    /// Draws `f` clipped to `min`..`max` (and, if `intersect`, to the current clip rectangle).
    pub fn with_clip_rect<R>(&self, min: Vec2, max: Vec2, intersect: bool, f: impl FnOnce(&DrawList<'ui>) -> R) -> R {
        let p = [min, max];
        self.with(|api, list| unsafe { (api.mqemu_imgui_draw_push_clip_rect)(list, p.as_ptr().cast(), intersect) });
        let (api, list) = (self.api, self.list);
        let _pop = OnDrop(move || {
            if !list.is_null() {
                unsafe { (api.mqemu_imgui_draw_pop_clip_rect)(list) }
            }
        });
        f(self)
    }

    /// Splits the list into `count` channels (layers) for `f`: whatever is
    /// drawn on channel 0 ends up under channel 1, and so on, whatever the
    /// order of the calls. Channel 0 is current to start with. None (and `f`
    /// doesn't run) if the list is already split, or `count` is under 2.
    pub fn channels<R>(&self, count: usize, f: impl FnOnce(&mut Channels<'_, 'ui>) -> R) -> Option<R> {
        let count = i32::try_from(count).unwrap_or(i32::MAX);
        if self.list.is_null() || !unsafe { (self.api.mqemu_imgui_draw_channels_split)(self.list, count) } {
            return None;
        }
        let (api, list) = (self.api, self.list);
        let _merge = OnDrop(move || unsafe { (api.mqemu_imgui_draw_channels_merge)(list) });
        Some(f(&mut Channels { draw: self }))
    }
}

/// Counts points for ImGui, which takes an `int`; more than `i32::MAX`
/// points aren't drawn past that.
fn count(points: &[Vec2]) -> i32 {
    i32::try_from(points.len()).unwrap_or(i32::MAX)
}

fn ngon_sides(sides: u32) -> i32 {
    assert!(sides >= 3, "a regular polygon needs at least 3 sides, not {sides}");
    i32::try_from(sides).unwrap_or(i32::MAX)
}

/// A draw list split into channels; see [`DrawList::channels`]. Draws on the
/// current channel like a [`DrawList`].
pub struct Channels<'d, 'ui> {
    draw: &'d DrawList<'ui>,
}

impl Channels<'_, '_> {
    /// Draws on channel `index` from now on. False (nothing changes) if
    /// there's no such channel.
    pub fn set_current(&mut self, index: usize) -> bool {
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        unsafe { (self.draw.api.mqemu_imgui_draw_channels_set_current)(self.draw.list, index) }
    }
}

impl<'ui> std::ops::Deref for Channels<'_, 'ui> {
    type Target = DrawList<'ui>;
    fn deref(&self) -> &DrawList<'ui> {
        self.draw
    }
}

/// A path being built on a draw list; see [`DrawList::path`]. Each method
/// adds to it; [`Path::stroke`] or a fill draws it. Dropped unfinished, it's
/// discarded.
#[must_use = "a path is drawn by stroking or filling it"]
pub struct Path<'d, 'ui> {
    draw: &'d DrawList<'ui>,
    finished: bool,
}

impl Path<'_, '_> {
    fn add(&mut self, f: impl FnOnce(&sys::Api, *mut sys::ImDrawList)) -> &mut Self {
        self.draw.with(f);
        self
    }

    pub fn line_to(&mut self, point: Vec2) -> &mut Self {
        self.add(|api, list| unsafe { (api.mqemu_imgui_path_line_to)(list, point[0], point[1]) })
    }

    /// An arc of a circle, from angle `from` to `to` in radians (0 is +x,
    /// increasing clockwise on screen).
    pub fn arc_to(&mut self, center: Vec2, radius: f32, from: f32, to: f32) -> &mut Self {
        let [x, y] = center;
        self.add(|api, list| unsafe { (api.mqemu_imgui_path_arc_to)(list, x, y, radius, -1.0, 0.0, from, to, 0) })
    }

    /// An arc of an ellipse with `radii` `[x, y]`, rotated by `rotation` radians.
    pub fn elliptical_arc_to(&mut self, center: Vec2, radii: Vec2, rotation: f32, from: f32, to: f32) -> &mut Self {
        let ([x, y], [rx, ry]) = (center, radii);
        self.add(|api, list| unsafe { (api.mqemu_imgui_path_arc_to)(list, x, y, rx, ry.max(0.0), rotation, from, to, 0) })
    }

    /// A cubic Bezier curve from the last point to `points[2]`, pulled toward the first two.
    pub fn bezier_cubic_to(&mut self, points: [Vec2; 3]) -> &mut Self {
        self.add(|api, list| unsafe { (api.mqemu_imgui_path_bezier_to)(list, points.as_ptr().cast(), 3, 0) })
    }

    /// A quadratic Bezier curve from the last point to `points[1]`, pulled toward `points[0]`.
    pub fn bezier_quadratic_to(&mut self, points: [Vec2; 2]) -> &mut Self {
        self.add(|api, list| unsafe { (api.mqemu_imgui_path_bezier_to)(list, points.as_ptr().cast(), 2, 0) })
    }

    /// A rectangle's outline, rounded at `corners`.
    pub fn rect(&mut self, min: Vec2, max: Vec2, rounding: f32, corners: Corners) -> &mut Self {
        let p = [min, max];
        self.add(|api, list| unsafe { (api.mqemu_imgui_path_rect)(list, p.as_ptr().cast(), rounding, corners.bits()) })
    }

    /// Draws lines along the path (back to its start if `closed`).
    pub fn stroke(mut self, color: Color, closed: bool, thickness: f32) {
        let flags = if closed { sys::ImDrawFlags_Closed } else { 0 };
        self.finish(color, flags, thickness.max(f32::MIN_POSITIVE), true);
    }

    /// Fills the path as a convex polygon (points clockwise).
    pub fn fill_convex(mut self, color: Color) {
        self.finish(color, 0, 0.0, true);
    }

    /// Fills the path as a polygon of any shape without holes or self-crossings.
    pub fn fill_concave(mut self, color: Color) {
        self.finish(color, 0, 0.0, false);
    }

    fn finish(&mut self, color: Color, flags: i32, thickness: f32, convex: bool) {
        self.finished = true;
        self.draw.with(|api, list| unsafe { (api.mqemu_imgui_path_finish)(list, pack_color(color), flags, thickness, convex) });
    }
}

impl Drop for Path<'_, '_> {
    fn drop(&mut self) {
        if !self.finished {
            self.draw.with(|api, list| unsafe { (api.mqemu_imgui_path_clear)(list) });
        }
    }
}
