//! Textures and images: image files loaded through the game's renderer, the
//! game's own UI textures (item and spell icons), and widgets that show them.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::marker::PhantomData;
use std::ptr::NonNull;

use mq_emu_sys::imgui_mq as mq_sys;

use super::draw::{DrawList, Vec2, pack_color};
use super::{Color, Ui, s};

/// A texture as ImGui draws it (`ImTextureID`), valid for as long as `'t`:
/// from [`Texture::id`], borrowing the texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureId<'t> {
    raw: NonNull<c_void>,
    _texture: PhantomData<&'t Texture>,
}

impl TextureId<'_> {
    /// A texture from elsewhere: on emu, an `IDirect3DTexture9` pointer.
    ///
    /// # Safety
    /// It must stay a valid texture until every frame drawing it has been
    /// rendered: after the plugin's next `on_update_imgui` starts, not when
    /// this one ends.
    pub const unsafe fn from_raw(raw: NonNull<c_void>) -> TextureId<'static> {
        TextureId { raw, _texture: PhantomData }
    }

    pub fn as_raw(self) -> *mut c_void {
        self.raw.as_ptr()
    }

    pub(super) fn raw(self) -> *mut c_void {
        self.raw.as_ptr()
    }
}

/// How an image is drawn ([`Ui::image_with`], [`Ui::image_button_with`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageOptions {
    /// Texture coordinates of the top-left and bottom-right corners (0 to 1).
    pub uv: [Vec2; 2],
    /// Drawn behind the image (transparent for none).
    pub background: Color,
    /// Multiplies the image's colors (white leaves them).
    pub tint: Color,
}

impl Default for ImageOptions {
    fn default() -> Self {
        ImageOptions { uv: [[0.0, 0.0], [1.0, 1.0]], background: [0.0; 4], tint: [1.0; 4] }
    }
}

thread_local! {
    /// Textures dropped, freed when the next frame starts: the frame that
    /// last drew them may not have been rendered when they were dropped.
    static RETIRED: RefCell<Vec<(NonNull<mq_sys::MQTexture>, &'static mq_sys::Api)>> = const { RefCell::new(Vec::new()) };
    /// Bumped when the game unloads its UI, which frees the textures
    /// [`TextureAnimation`]s point into.
    static UI_GENERATION: Cell<u64> = const { Cell::new(0) };
}

/// Frees the textures dropped before now. The runtime calls this as a frame
/// starts (by then the frame that drew them has been rendered) and at shutdown.
pub(crate) fn free_retired_textures() {
    let retired = RETIRED.with(|r| std::mem::take(&mut *r.borrow_mut()));
    for (texture, api) in retired {
        unsafe { (api.mqemu_texture_destroy)(texture.as_ptr()) };
    }
}

/// Which loading of the game's UI this is: what it points into is gone once
/// this changes.
pub(super) fn ui_generation() -> u64 {
    UI_GENERATION.with(Cell::get)
}

/// The game is unloading its UI: [`TextureAnimation`]s found before now
/// stop drawing.
pub(crate) fn ui_unloaded() {
    UI_GENERATION.with(|g| g.set(g.get() + 1));
}

/// An image file loaded as a texture through the game's renderer
/// ([`Ui::load_texture`]). Draw it with [`Ui::image`] or
/// [`DrawList::add_image`](super::DrawList::add_image) through [`Texture::id`].
///
/// Dropping it frees it once the frame that may still draw it has been
/// rendered.
pub struct Texture {
    raw: NonNull<mq_sys::MQTexture>,
    api: &'static mq_sys::Api,
}

impl Texture {
    /// What ImGui draws, now. None while the game's graphics device is lost
    /// (alt-tab in full screen, say): MacroQuest recreates the texture
    /// afterwards, so ask again each frame rather than keeping it.
    pub fn id(&self) -> Option<TextureId<'_>> {
        let raw = unsafe { (self.api.mqemu_texture_id)(self.raw.as_ptr()) };
        NonNull::new(raw).map(|raw| TextureId { raw, _texture: PhantomData })
    }

    /// Its size in pixels, `[w, h]`.
    pub fn size(&self) -> Vec2 {
        let (mut w, mut h) = (0, 0);
        unsafe { (self.api.mqemu_texture_size)(self.raw.as_ptr(), &mut w, &mut h) };
        [w as f32, h as f32]
    }
}

impl Drop for Texture {
    fn drop(&mut self) {
        let entry = (self.raw, self.api);
        RETIRED.with(|r| r.borrow_mut().push(entry));
    }
}

impl std::fmt::Debug for Texture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Texture").field("size", &self.size()).finish_non_exhaustive()
    }
}

/// The game's UI animation holding item icons ([`TextureAnimation::set_item_icon`]).
pub const ITEM_ICONS: &str = "A_DragItem";
/// The game's UI animation holding spell icons ([`TextureAnimation::set_cell`]
/// with [`Spell::icon`](crate::Spell::icon)).
pub const SPELL_ICONS: &str = "A_SpellIcons";

/// One of the game's UI textures (from its SIDL files), as
/// [`Ui::find_texture_animation`] found it: a picture, an animation, or a
/// grid of cells such as an icon sheet ([`ITEM_ICONS`], [`SPELL_ICONS`]).
///
/// It points into textures the game owns, which it frees when it unloads its
/// UI (`/loadskin`, character select): from then on this draws nothing; find
/// it again after [`Plugin::on_reload_ui`](crate::Plugin::on_reload_ui).
pub struct TextureAnimation {
    raw: NonNull<mq_sys::CTextureAnimation>,
    api: &'static mq_sys::Api,
    generation: u64,
}

impl TextureAnimation {
    /// The animation, unless the UI it came from was unloaded.
    pub(crate) fn current(&self) -> Option<*const mq_sys::CTextureAnimation> {
        (self.generation == UI_GENERATION.with(Cell::get)).then_some(self.raw.as_ptr())
    }

    /// Whether the UI it came from is still loaded, so it can draw.
    pub fn is_current(&self) -> bool {
        self.current().is_some()
    }

    /// Which cell of a grid to draw (-1 for none). For [`SPELL_ICONS`], a
    /// spell's [`icon`](crate::Spell::icon).
    pub fn set_cell(&mut self, cell: i32) {
        if self.is_current() {
            unsafe { (self.api.mqemu_texture_animation_set_cell)(self.raw.as_ptr(), cell) }
        }
    }

    /// For [`ITEM_ICONS`]: shows the icon `icon`, an [`Item::icon`](crate::Item::icon) number.
    pub fn set_item_icon(&mut self, icon: i32) {
        self.set_cell(icon - mq_sys::ITEM_ICON_OFFSET);
    }

    pub fn cell(&self) -> i32 {
        unsafe { (self.api.mqemu_texture_animation_cell)(self.raw.as_ptr()) }
    }

    /// Its size and, for a grid, a cell's size (None otherwise), in pixels.
    pub fn size(&self) -> (Vec2, Option<Vec2>) {
        let (mut w, mut h, mut cw, mut ch) = (0, 0, 0, 0);
        unsafe { (self.api.mqemu_texture_animation_size)(self.raw.as_ptr(), &mut w, &mut h, &mut cw, &mut ch) };
        ([w as f32, h as f32], (cw > 0).then_some([cw as f32, ch as f32]))
    }

    /// How many frames it has: one for a picture (a grid's cells are all
    /// in its one frame), more for an animation.
    pub fn frame_count(&self) -> usize {
        usize::try_from(unsafe { (self.api.mqemu_texture_animation_frames)(self.raw.as_ptr()) }).unwrap_or(0)
    }

    /// Frame `index`'s picture, as a piece of its texture file. None past
    /// its frames, or once the UI it came from was unloaded.
    pub fn frame(&self, index: usize) -> Option<TexturePiece> {
        let raw = self.current()?;
        let index = i32::try_from(index).ok()?;
        let piece = NonNull::new(unsafe { (self.api.mqemu_texture_animation_piece)(raw, index) })?;
        Some(TexturePiece { raw: piece, api: self.api, generation: self.generation })
    }
}

/// Rounds a pixel position or size for MacroQuest's integer rectangles.
fn pixels([x, y]: Vec2) -> [i32; 2] {
    [x.round() as i32, y.round() as i32]
}

/// How a [`UiTexture`] or [`TexturePiece`] is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UiImageOptions {
    /// Its size on screen; None for its own: the piece's, or the source
    /// rectangle's.
    pub size: Option<Vec2>,
    /// Multiplies its colors (white leaves them).
    pub tint: Color,
    /// A frame around it, if any.
    pub border: Option<Color>,
}

impl Default for UiImageOptions {
    fn default() -> Self {
        UiImageOptions { size: None, tint: [1.0; 4], border: None }
    }
}

impl UiImageOptions {
    fn raw_size(&self) -> [i32; 2] {
        self.size.map_or([0, 0], pixels)
    }

    fn raw_border(&self) -> u32 {
        self.border.map_or(0, pack_color)
    }
}

/// A rectangle in a texture as the shim takes it: the whole texture for None.
fn source_rect(source: Option<[Vec2; 2]>) -> [i32; 4] {
    source.map_or([0, 0, -1, -1], |[min, max]| {
        let ([x0, y0], [x1, y1]) = (pixels(min), pixels(max));
        [x0, y0, x1, y1]
    })
}

/// One of the game's UI texture files (`AttackIndicator.tga`, say), as
/// [`Ui::find_ui_texture`] loaded it. Like a [`TextureAnimation`], it stops
/// drawing once the game unloads its UI.
pub struct UiTexture {
    raw: NonNull<mq_sys::CUITextureInfo>,
    api: &'static mq_sys::Api,
    generation: u64,
}

impl UiTexture {
    fn current(&self) -> Option<*mut mq_sys::CUITextureInfo> {
        (self.generation == UI_GENERATION.with(Cell::get)).then_some(self.raw.as_ptr())
    }

    /// Whether the UI it came from is still loaded, so it can draw.
    pub fn is_current(&self) -> bool {
        self.current().is_some()
    }

    /// Its file name.
    pub fn name(&self) -> String {
        let mut buffer = [0 as std::ffi::c_char; 260];
        let len = unsafe { (self.api.mqemu_ui_texture_name)(self.raw.as_ptr(), buffer.as_mut_ptr(), buffer.len()) };
        if len < buffer.len() {
            return crate::text::from_c_array(&buffer);
        }
        let mut long = vec![0 as std::ffi::c_char; len + 1];
        unsafe { (self.api.mqemu_ui_texture_name)(self.raw.as_ptr(), long.as_mut_ptr(), long.len()) };
        crate::text::from_c_array(&long)
    }

    /// Its size in pixels, which rectangles of it are measured in.
    pub fn size(&self) -> Vec2 {
        let (mut w, mut h) = (0, 0);
        unsafe { (self.api.mqemu_ui_texture_size)(self.raw.as_ptr(), &mut w, &mut h) };
        [w as f32, h as f32]
    }

    /// The rectangle of it from `min` to `max` (in pixels), to draw on its
    /// own. None once the UI it came from was unloaded.
    pub fn piece(&self, min: Vec2, max: Vec2) -> Option<TexturePiece> {
        let raw = self.current()?;
        let ([x0, y0], [x1, y1]) = (pixels(min), pixels(max));
        let piece = NonNull::new(unsafe { (self.api.mqemu_texture_piece_new)(raw, x0, y0, x1, y1) })?;
        Some(TexturePiece { raw: piece, api: self.api, generation: self.generation })
    }
}

impl Drop for UiTexture {
    fn drop(&mut self) {
        // Our copy: the game keeps the texture itself.
        unsafe { (self.api.mqemu_ui_texture_free)(self.raw.as_ptr()) }
    }
}

impl std::fmt::Debug for UiTexture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiTexture").field("name", &self.name()).field("size", &self.size()).field("current", &self.is_current()).finish()
    }
}

/// A rectangle of one of the game's UI texture files: a frame of a
/// [`TextureAnimation`] ([`TextureAnimation::frame`]) or one you choose
/// ([`UiTexture::piece`]). Stops drawing once the game unloads its UI.
pub struct TexturePiece {
    raw: NonNull<mq_sys::CUITexturePiece>,
    api: &'static mq_sys::Api,
    generation: u64,
}

impl TexturePiece {
    fn current(&self) -> Option<*mut mq_sys::CUITexturePiece> {
        (self.generation == UI_GENERATION.with(Cell::get)).then_some(self.raw.as_ptr())
    }

    pub fn is_current(&self) -> bool {
        self.current().is_some()
    }

    /// Where it is in its texture, top left and bottom right, in pixels.
    pub fn rect(&self) -> [Vec2; 2] {
        let (mut x0, mut y0, mut x1, mut y1) = (0, 0, 0, 0);
        unsafe { (self.api.mqemu_texture_piece_rect)(self.raw.as_ptr(), &mut x0, &mut y0, &mut x1, &mut y1) };
        [[x0 as f32, y0 as f32], [x1 as f32, y1 as f32]]
    }

    /// Its size in pixels, what it's drawn at by default.
    pub fn size(&self) -> Vec2 {
        let [[x0, y0], [x1, y1]] = self.rect();
        [x1 - x0, y1 - y0]
    }

    /// The texture file it's a piece of. None once the UI was unloaded.
    pub fn texture(&self) -> Option<UiTexture> {
        let raw = self.current()?;
        let texture = NonNull::new(unsafe { (self.api.mqemu_texture_piece_texture)(raw) })?;
        Some(UiTexture { raw: texture, api: self.api, generation: self.generation })
    }
}

impl Drop for TexturePiece {
    fn drop(&mut self) {
        unsafe { (self.api.mqemu_texture_piece_free)(self.raw.as_ptr()) }
    }
}

impl std::fmt::Debug for TexturePiece {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TexturePiece").field("rect", &self.rect()).field("current", &self.is_current()).finish()
    }
}

impl Drop for TextureAnimation {
    fn drop(&mut self) {
        // A copy: what it points to stays the game's, and draw data doesn't
        // refer to the copy itself.
        unsafe { (self.api.mqemu_texture_animation_free)(self.raw.as_ptr()) }
    }
}

impl std::fmt::Debug for TextureAnimation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextureAnimation").field("cell", &self.cell()).field("current", &self.is_current()).finish_non_exhaustive()
    }
}

impl Ui {
    /// Loads an image file (`.dds`, `.png`, `.bmp`, `.tga`, ...; relative
    /// paths are from the game's directory) as a texture. None if it can't be
    /// read, or the game isn't drawing yet. Load once and keep it.
    pub fn load_texture(&self, path: &str) -> Option<Texture> {
        let (p, n) = s(path);
        let raw = NonNull::new(unsafe { (self.mq.mqemu_texture_create)(p, n) })?;
        Some(Texture { raw, api: self.mq })
    }

    /// A copy of the game's UI texture `name` (such as [`ITEM_ICONS`]); None
    /// if there's none, or the UI isn't loaded. Find once and keep it (until
    /// the UI reloads).
    pub fn find_texture_animation(&self, name: &str) -> Option<TextureAnimation> {
        let (p, n) = s(name);
        let raw = NonNull::new(unsafe { (self.mq.mqemu_texture_animation_find)(p, n) })?;
        Some(TextureAnimation { raw, api: self.mq, generation: UI_GENERATION.with(Cell::get) })
    }

    /// Shows `texture`, `size` pixels.
    pub fn image(&mut self, texture: TextureId<'_>, size: Vec2) {
        self.image_with(texture, size, ImageOptions::default());
    }

    pub fn image_with(&mut self, texture: TextureId<'_>, size: Vec2, options: ImageOptions) {
        let ImageOptions { uv, background, tint } = options;
        unsafe { (self.api.mqemu_imgui_image)(texture.raw(), size[0], size[1], uv.as_ptr().cast(), background.as_ptr(), tint.as_ptr()) }
    }

    /// A button showing `texture`; true when clicked. `id` identifies it (it
    /// shows no label).
    pub fn image_button(&mut self, id: &str, texture: TextureId<'_>, size: Vec2) -> bool {
        self.image_button_with(id, texture, size, ImageOptions::default())
    }

    pub fn image_button_with(&mut self, id: &str, texture: TextureId<'_>, size: Vec2, options: ImageOptions) -> bool {
        let (i, n) = s(id);
        let ImageOptions { uv, background, tint } = options;
        unsafe {
            (self.api.mqemu_imgui_image_button)(i, n, texture.raw(), size[0], size[1], uv.as_ptr().cast(), background.as_ptr(), tint.as_ptr())
        }
    }

    /// Shows one of the game's UI textures as an item (hover checks, tooltips
    /// and drag sources work on it), `size` pixels (None for its own size),
    /// with a light border if `border`. False if nothing was drawn: the
    /// animation is out of date, has no cell set, or is scrolled off.
    pub fn texture_animation(&mut self, animation: &TextureAnimation, size: Option<Vec2>, border: bool) -> bool {
        self.texture_animation_with(animation, size, [1.0; 4], border.then_some([1.0, 1.0, 1.0, 0.5]))
    }

    /// One of the game's UI texture files (from the skin's directory, or
    /// the default one), loaded: `"AttackIndicator.tga"`, say. None if the
    /// game can't load it, or its UI isn't loaded. Find once and keep it
    /// (until the UI reloads).
    pub fn find_ui_texture(&self, name: &str) -> Option<UiTexture> {
        let (p, n) = s(name);
        let raw = NonNull::new(unsafe { (self.mq.mqemu_ui_texture_find)(p, n) })?;
        Some(UiTexture { raw, api: self.mq, generation: UI_GENERATION.with(Cell::get) })
    }

    /// Shows the rectangle `source` of `texture` (from its top left to
    /// bottom right, in pixels; None for all of it) as an item. False if
    /// nothing was drawn: the texture is out of date, or scrolled off.
    pub fn ui_texture(&mut self, texture: &UiTexture, source: Option<[Vec2; 2]>, options: UiImageOptions) -> bool {
        let Some(raw) = texture.current() else { return false };
        let ([w, h], [sx0, sy0, sx1, sy1]) = (options.raw_size(), source_rect(source));
        unsafe { (self.mq.mqemu_imgui_ui_texture)(raw, w, h, sx0, sy0, sx1, sy1, pack_color(options.tint), options.raw_border()) }
    }

    /// Shows `piece` as an item, its own size unless `options` says.
    pub fn texture_piece(&mut self, piece: &TexturePiece, options: UiImageOptions) -> bool {
        let Some(raw) = piece.current() else { return false };
        let [w, h] = options.raw_size();
        unsafe { (self.mq.mqemu_imgui_texture_piece)(raw, w, h, pack_color(options.tint), options.raw_border()) }
    }

    /// [`Ui::texture_animation`] tinted, with a border of any color.
    pub fn texture_animation_with(&mut self, animation: &TextureAnimation, size: Option<Vec2>, tint: Color, border: Option<Color>) -> bool {
        let Some(raw) = animation.current() else { return false };
        let [w, h] = size.map_or([0, 0], |[w, h]| [w.round() as i32, h.round() as i32]);
        unsafe { (self.mq.mqemu_imgui_texture_animation)(raw, w, h, pack_color(tint), border.map_or(0, pack_color)) }
    }
}

impl DrawList<'_> {
    /// [`Ui::ui_texture`] on the list, its top left at `pos`.
    pub fn add_ui_texture(&self, texture: &UiTexture, pos: Vec2, source: Option<[Vec2; 2]>, options: UiImageOptions) -> bool {
        let (Some(raw), Some(list)) = (texture.current(), self.raw()) else { return false };
        let ([x, y], [w, h], [sx0, sy0, sx1, sy1]) = (pixels(pos), options.raw_size(), source_rect(source));
        let (tint, border) = (pack_color(options.tint), options.raw_border());
        unsafe { (self.mq_api().mqemu_imgui_draw_ui_texture)(list, raw, x, y, w, h, sx0, sy0, sx1, sy1, tint, border) }
    }

    /// [`Ui::texture_piece`] on the list, its top left at `pos`.
    pub fn add_texture_piece(&self, piece: &TexturePiece, pos: Vec2, options: UiImageOptions) -> bool {
        let (Some(raw), Some(list)) = (piece.current(), self.raw()) else { return false };
        let ([x, y], [w, h]) = (pixels(pos), options.raw_size());
        unsafe { (self.mq_api().mqemu_imgui_draw_texture_piece)(list, raw, x, y, w, h, pack_color(options.tint), options.raw_border()) }
    }
}
