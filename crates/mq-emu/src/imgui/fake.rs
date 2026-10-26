//! Test-only stand-ins for MacroQuest's drawing functions (textures, the
//! game's UI textures, EQ fonts), which only exist in the game. They record
//! what was asked of them, per thread.

use std::cell::RefCell;
use std::ffi::{c_char, c_void};

use mq_emu_sys::imgui::{ImDrawList, ImFont};
use mq_emu_sys::imgui_mq::{Api, CTextureAnimation, MQTexture};

use super::Apis;

/// What the fake functions saw.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct Log {
    pub textures_created: Vec<String>,
    pub textures_destroyed: Vec<String>,
    pub animations_freed: usize,
    /// `(cell, w, h, tint, border, on a draw list)` per animation drawn.
    pub animations_drawn: Vec<(i32, i32, i32, u32, u32, bool)>,
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

/// No EQ fonts outside the game.
unsafe extern "C" fn eq_font(_: i32) -> *mut ImFont {
    std::ptr::null_mut()
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
    mqemu_imgui_eq_font: eq_font,
};

/// Real ImGui and ImPlot (imgui.dll), fake MacroQuest.
pub(crate) const APIS: Apis = Apis { imgui: &mq_emu_sys::imgui::API, implot: &mq_emu_sys::implot::API, mq: &FAKE_MQ };
