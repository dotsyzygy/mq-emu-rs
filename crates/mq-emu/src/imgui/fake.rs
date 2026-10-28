//! Test-only stand-ins for MacroQuest's drawing functions (textures, the
//! game's UI textures, EQ fonts), which only exist in the game. They record
//! what was asked of them, per thread.

use std::cell::RefCell;
use std::ffi::{c_char, c_void};

use mq_emu_sys::imgui::{ImDrawList, ImFont};
use mq_emu_sys::imgui_mq::{Api, CScreenPieceTemplate, CTextureAnimation, CUITextureInfo, CUITexturePiece, MQTexture};

use super::Apis;

/// What the fake functions saw.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Log {
    pub textures_created: Vec<String>,
    pub textures_destroyed: Vec<String>,
    pub animations_freed: usize,
    /// `(cell, w, h, tint, border, on a draw list)` per animation drawn.
    pub animations_drawn: Vec<(i32, i32, i32, u32, u32, bool)>,
    /// `(gem, flags, at)` per spell gem drawn: flags -1 and the position for
    /// one on a draw list.
    pub spell_gems: Vec<(i32, i32, Option<[i32; 2]>)>,
    /// `(text, color, hovered color)` per item link.
    pub item_links: Vec<(String, u32, u32)>,
    /// `(font style, text, rectangle, color, flags)` per EQ text drawn.
    pub eq_texts: Vec<(i32, String, [i32; 4], u32, i32)>,
    /// UI texture and piece copies freed.
    pub ui_textures_freed: usize,
    pub pieces_freed: usize,
    /// Per UI texture or piece drawn: what, where on a draw list (None as
    /// an item), the size asked, the source (the piece's rectangle for a
    /// piece), tint and border.
    pub ui_drawn: Vec<UiDrawn>,
    /// `(name, rectangle)` per screen piece drawn.
    pub screen_pieces: Vec<(String, [i32; 4])>,
}

/// One [`Log::ui_drawn`] entry.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UiDrawn {
    pub texture: String,
    pub piece: bool,
    pub at: Option<[i32; 2]>,
    pub size: [i32; 2],
    pub source: [i32; 4],
    pub tint: u32,
    pub border: u32,
}

thread_local! {
    static LOG: RefCell<Log> = RefCell::new(Log::default());
}

/// What the fakes saw on this thread so far.
pub(crate) fn log() -> Log {
    LOG.with(|l| l.borrow().clone())
}

struct FakeTexture {
    path: String,
}

struct FakeAnimation {
    cell: i32,
}

/// A path the fake can't load.
pub(crate) const MISSING: &str = "missing.png";

unsafe extern "C" fn texture_create(path: *const c_char, len: usize) -> *mut MQTexture {
    let path = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(path.cast(), len) }).into_owned();
    if path == MISSING {
        return std::ptr::null_mut();
    }
    LOG.with(|l| l.borrow_mut().textures_created.push(path.clone()));
    Box::into_raw(Box::new(FakeTexture { path })).cast()
}

unsafe extern "C" fn texture_destroy(texture: *mut MQTexture) {
    let texture = unsafe { Box::from_raw(texture.cast::<FakeTexture>()) };
    LOG.with(|l| l.borrow_mut().textures_destroyed.push(texture.path));
}

/// The texture's own address: ImGui only records it.
unsafe extern "C" fn texture_id(texture: *const MQTexture) -> *mut c_void {
    texture.cast_mut().cast()
}

unsafe extern "C" fn texture_size(_: *const MQTexture, w: *mut i32, h: *mut i32) {
    unsafe {
        *w = 64;
        *h = 32;
    }
}

unsafe extern "C" fn animation_find(name: *const c_char, len: usize) -> *mut CTextureAnimation {
    let name = unsafe { std::slice::from_raw_parts(name.cast::<u8>(), len) };
    if name != super::ITEM_ICONS.as_bytes() && name != super::SPELL_ICONS.as_bytes() {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(FakeAnimation { cell: -1 })).cast()
}

unsafe extern "C" fn animation_free(animation: *mut CTextureAnimation) {
    drop(unsafe { Box::from_raw(animation.cast::<FakeAnimation>()) });
    LOG.with(|l| l.borrow_mut().animations_freed += 1);
}

unsafe extern "C" fn animation_set_cell(animation: *mut CTextureAnimation, cell: i32) {
    unsafe { (*animation.cast::<FakeAnimation>()).cell = cell };
}

unsafe extern "C" fn animation_cell(animation: *const CTextureAnimation) -> i32 {
    unsafe { (*animation.cast::<FakeAnimation>()).cell }
}

unsafe extern "C" fn animation_size(_: *const CTextureAnimation, w: *mut i32, h: *mut i32, cw: *mut i32, ch: *mut i32) {
    unsafe {
        (*w, *h, *cw, *ch) = (256, 256, 40, 40);
    }
}

unsafe extern "C" fn draw_animation(animation: *const CTextureAnimation, w: i32, h: i32, tint: u32, border: u32) -> bool {
    let cell = unsafe { animation_cell(animation) };
    LOG.with(|l| l.borrow_mut().animations_drawn.push((cell, w, h, tint, border, false)));
    true
}

unsafe extern "C" fn draw_list_animation(
    _: *mut ImDrawList,
    animation: *const CTextureAnimation,
    _: i32,
    _: i32,
    w: i32,
    h: i32,
    tint: u32,
    border: u32,
) -> bool {
    let cell = unsafe { animation_cell(animation) };
    LOG.with(|l| l.borrow_mut().animations_drawn.push((cell, w, h, tint, border, true)));
    true
}

/// The texture the fake's animations' frames are pieces of, and its size.
pub(crate) const ANIMATION_TEXTURE: &str = "dragitem1.tga";
pub(crate) const FAKE_TEXTURE_SIZE: [i32; 2] = [256, 128];
/// A screen piece the fake has that MacroQuest draws, and one it doesn't.
pub(crate) const DRAWN_PIECE: &str = "Inv_Seperator1";
pub(crate) const TEXT_PIECE: &str = "Inv_Label";

struct FakeUiTexture {
    name: String,
}

struct FakePiece {
    texture: String,
    rect: [i32; 4],
}

struct FakeScreenPiece {
    name: &'static str,
    kind: i32,
    draws: bool,
}

static SCREEN_PIECES: [FakeScreenPiece; 2] = [
    FakeScreenPiece { name: DRAWN_PIECE, kind: mq_emu_sys::imgui_mq::StaticScreenPieceAnimation, draws: true },
    FakeScreenPiece { name: TEXT_PIECE, kind: mq_emu_sys::imgui_mq::StaticScreenPieceText, draws: false },
];

/// One frame, a 40 x 40 piece.
unsafe extern "C" fn animation_frames(_: *const CTextureAnimation) -> i32 {
    1
}

unsafe extern "C" fn animation_piece(_: *const CTextureAnimation, frame: i32) -> *mut CUITexturePiece {
    if frame != 0 {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(FakePiece { texture: ANIMATION_TEXTURE.to_owned(), rect: [0, 0, 40, 40] })).cast()
}

unsafe extern "C" fn ui_texture_find(name: *const c_char, len: usize) -> *mut CUITextureInfo {
    let name = text(name, len);
    if name == MISSING || name.is_empty() {
        return std::ptr::null_mut();
    }
    Box::into_raw(Box::new(FakeUiTexture { name })).cast()
}

unsafe extern "C" fn ui_texture_free(texture: *mut CUITextureInfo) {
    drop(unsafe { Box::from_raw(texture.cast::<FakeUiTexture>()) });
    LOG.with(|l| l.borrow_mut().ui_textures_freed += 1);
}

unsafe extern "C" fn ui_texture_size(_: *const CUITextureInfo, w: *mut i32, h: *mut i32) {
    unsafe { [*w, *h] = FAKE_TEXTURE_SIZE };
}

unsafe extern "C" fn ui_texture_name(texture: *const CUITextureInfo, out: *mut c_char, cap: usize) -> usize {
    let name = unsafe { &(*texture.cast::<FakeUiTexture>()).name };
    let n = name.len().min(cap.saturating_sub(1));
    unsafe {
        std::ptr::copy_nonoverlapping(name.as_ptr().cast(), out, n);
        *out.add(n) = 0;
    }
    name.len()
}

unsafe extern "C" fn piece_new(texture: *const CUITextureInfo, x0: i32, y0: i32, x1: i32, y1: i32) -> *mut CUITexturePiece {
    let texture = unsafe { (*texture.cast::<FakeUiTexture>()).name.clone() };
    Box::into_raw(Box::new(FakePiece { texture, rect: [x0, y0, x1, y1] })).cast()
}

unsafe extern "C" fn piece_free(piece: *mut CUITexturePiece) {
    drop(unsafe { Box::from_raw(piece.cast::<FakePiece>()) });
    LOG.with(|l| l.borrow_mut().pieces_freed += 1);
}

unsafe extern "C" fn piece_rect(piece: *const CUITexturePiece, x0: *mut i32, y0: *mut i32, x1: *mut i32, y1: *mut i32) {
    unsafe { [*x0, *y0, *x1, *y1] = (*piece.cast::<FakePiece>()).rect };
}

unsafe extern "C" fn piece_texture(piece: *const CUITexturePiece) -> *mut CUITextureInfo {
    let name = unsafe { (*piece.cast::<FakePiece>()).texture.clone() };
    Box::into_raw(Box::new(FakeUiTexture { name })).cast()
}

fn log_ui(drawn: UiDrawn) -> bool {
    LOG.with(|l| l.borrow_mut().ui_drawn.push(drawn));
    true
}

unsafe extern "C" fn draw_ui_texture(texture: *mut CUITextureInfo, w: i32, h: i32, sx0: i32, sy0: i32, sx1: i32, sy1: i32, tint: u32, border: u32) -> bool {
    let texture = unsafe { (*texture.cast::<FakeUiTexture>()).name.clone() };
    log_ui(UiDrawn { texture, piece: false, at: None, size: [w, h], source: [sx0, sy0, sx1, sy1], tint, border })
}

unsafe extern "C" fn draw_list_ui_texture(
    _: *mut ImDrawList,
    texture: *mut CUITextureInfo,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    sx0: i32,
    sy0: i32,
    sx1: i32,
    sy1: i32,
    tint: u32,
    border: u32,
) -> bool {
    let texture = unsafe { (*texture.cast::<FakeUiTexture>()).name.clone() };
    log_ui(UiDrawn { texture, piece: false, at: Some([x, y]), size: [w, h], source: [sx0, sy0, sx1, sy1], tint, border })
}

unsafe extern "C" fn draw_piece(piece: *mut CUITexturePiece, w: i32, h: i32, tint: u32, border: u32) -> bool {
    let piece = unsafe { &*piece.cast::<FakePiece>() };
    log_ui(UiDrawn { texture: piece.texture.clone(), piece: true, at: None, size: [w, h], source: piece.rect, tint, border })
}

unsafe extern "C" fn draw_list_piece(_: *mut ImDrawList, piece: *mut CUITexturePiece, x: i32, y: i32, w: i32, h: i32, tint: u32, border: u32) -> bool {
    let piece = unsafe { &*piece.cast::<FakePiece>() };
    log_ui(UiDrawn { texture: piece.texture.clone(), piece: true, at: Some([x, y]), size: [w, h], source: piece.rect, tint, border })
}

unsafe extern "C" fn screen_piece_find(name: *const c_char, len: usize) -> *mut CScreenPieceTemplate {
    let name = text(name, len);
    SCREEN_PIECES.iter().find(|p| p.name == name).map_or(std::ptr::null_mut(), |p| std::ptr::from_ref(p).cast_mut().cast())
}

fn screen_piece(piece: *const CScreenPieceTemplate) -> &'static FakeScreenPiece {
    unsafe { &*piece.cast::<FakeScreenPiece>() }
}

unsafe extern "C" fn screen_piece_kind(piece: *const CScreenPieceTemplate) -> i32 {
    screen_piece(piece).kind
}

unsafe extern "C" fn screen_piece_draws(piece: *const CScreenPieceTemplate) -> bool {
    screen_piece(piece).draws
}

unsafe extern "C" fn screen_piece_rect(_: *const CScreenPieceTemplate, x0: *mut i32, y0: *mut i32, x1: *mut i32, y1: *mut i32) {
    unsafe { [*x0, *y0, *x1, *y1] = [1, 117, 101, 119] };
}

unsafe extern "C" fn draw_screen_piece(_: *mut ImDrawList, piece: *const CScreenPieceTemplate, x0: i32, y0: i32, x1: i32, y1: i32) -> bool {
    let piece = screen_piece(piece);
    if piece.draws {
        LOG.with(|l| l.borrow_mut().screen_pieces.push((piece.name.to_owned(), [x0, y0, x1, y1])));
    }
    piece.draws
}

/// No EQ fonts outside the game.
unsafe extern "C" fn eq_font(_: i32) -> *mut ImFont {
    std::ptr::null_mut()
}

fn text(text: *const c_char, len: usize) -> String {
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(text.cast(), len) }).into_owned()
}

unsafe extern "C" fn eq_text(_: *mut ImDrawList, style: i32, t: *const c_char, len: usize, x0: i32, y0: i32, x1: i32, y1: i32, col: u32, flags: i32) {
    let entry = (style, text(t, len), [x0, y0, x1, y1], col, flags);
    LOG.with(|l| l.borrow_mut().eq_texts.push(entry));
}

/// Gems the fake has: the first 8 (a level 1 character's).
pub(crate) const FAKE_GEMS: i32 = 8;

unsafe extern "C" fn spell_gem(_: *const c_char, _: usize, gem: i32, flags: i32, drawn: *mut bool) -> bool {
    unsafe { *drawn = gem < FAKE_GEMS };
    if gem < FAKE_GEMS {
        LOG.with(|l| l.borrow_mut().spell_gems.push((gem, flags, None)));
    }
    false
}

unsafe extern "C" fn draw_spell_gem(_: *mut ImDrawList, gem: i32, x: i32, y: i32, _: bool) -> bool {
    if gem < FAKE_GEMS {
        LOG.with(|l| l.borrow_mut().spell_gems.push((gem, -1, Some([x, y]))));
    }
    gem < FAKE_GEMS
}

unsafe extern "C" fn item_link_text(t: *const c_char, len: usize, col: u32, hovered: u32) -> bool {
    let entry = (text(t, len), col, hovered);
    LOG.with(|l| l.borrow_mut().item_links.push(entry));
    false
}

pub(crate) const FAKE_MQ: Api = Api {
    mqemu_texture_create: texture_create,
    mqemu_texture_destroy: texture_destroy,
    mqemu_texture_id: texture_id,
    mqemu_texture_size: texture_size,
    mqemu_texture_animation_find: animation_find,
    mqemu_texture_animation_free: animation_free,
    mqemu_texture_animation_set_cell: animation_set_cell,
    mqemu_texture_animation_cell: animation_cell,
    mqemu_texture_animation_size: animation_size,
    mqemu_imgui_texture_animation: draw_animation,
    mqemu_imgui_draw_texture_animation: draw_list_animation,
    mqemu_texture_animation_frames: animation_frames,
    mqemu_texture_animation_piece: animation_piece,
    mqemu_ui_texture_find: ui_texture_find,
    mqemu_ui_texture_free: ui_texture_free,
    mqemu_ui_texture_size: ui_texture_size,
    mqemu_ui_texture_name: ui_texture_name,
    mqemu_texture_piece_new: piece_new,
    mqemu_texture_piece_free: piece_free,
    mqemu_texture_piece_rect: piece_rect,
    mqemu_texture_piece_texture: piece_texture,
    mqemu_imgui_ui_texture: draw_ui_texture,
    mqemu_imgui_draw_ui_texture: draw_list_ui_texture,
    mqemu_imgui_texture_piece: draw_piece,
    mqemu_imgui_draw_texture_piece: draw_list_piece,
    mqemu_screen_piece_find: screen_piece_find,
    mqemu_screen_piece_kind: screen_piece_kind,
    mqemu_screen_piece_draws: screen_piece_draws,
    mqemu_screen_piece_rect: screen_piece_rect,
    mqemu_imgui_draw_screen_piece: draw_screen_piece,
    mqemu_imgui_eq_font: eq_font,
    mqemu_imgui_draw_eq_text: eq_text,
    mqemu_imgui_spell_gem: spell_gem,
    mqemu_imgui_draw_spell_gem: draw_spell_gem,
    mqemu_imgui_item_link_text: item_link_text,
};

/// Real ImGui, ImPlot and add-ons (imgui.dll), fake MacroQuest.
pub(crate) const APIS: Apis =
    Apis { imgui: &mq_emu_sys::imgui::API, implot: &mq_emu_sys::implot::API, mq: &FAKE_MQ, addons: &mq_emu_sys::imgui_addons::API };
