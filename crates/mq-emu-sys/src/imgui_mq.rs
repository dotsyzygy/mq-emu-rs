//! ImGui drawing that needs MacroQuest itself, from `shim/imgui_mq.cpp`:
//! textures loaded through the game's renderer, the game's own UI textures
//! (item and spell icons, window art), and EQ's fonts in ImGui.
//!
//! These import `MQ2Main.dll` and `eqlib.dll`, so unlike [`imgui`](crate::imgui)
//! they only run inside the game. Conventions as in [`imgui`](crate::imgui).

use core::ffi::{c_char, c_void};
use core::marker::{PhantomData, PhantomPinned};

use crate::imgui::{ImDrawList, ImFont};

/// MacroQuest's `mq::MQTexture`: an image file loaded as a texture.
#[repr(C)]
pub struct MQTexture {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// eqlib's `CTextureAnimation`: a texture from the game's UI, possibly a grid
/// of cells (icon sheets) or animated frames.
#[repr(C)]
pub struct CTextureAnimation {
    _opaque: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

/// What [`mqemu_texture_animation_set_cell`] takes for an item's icon:
/// `IconNumber` minus this (`A_DragItem` starts at icon 500).
pub const ITEM_ICON_OFFSET: i32 = 500;

function_table! {
    // ----- textures from files -----
    /// Loads an image file (bmp, dds, png, tga, ...) as a texture. Null if it
    /// can't be loaded, or the game has no renderer yet.
    pub fn mqemu_texture_create(path: *const c_char, path_len: usize) -> *mut MQTexture;
    /// Frees it at once: nothing may draw with it afterwards, including the
    /// current frame's draw data that hasn't been rendered yet.
    pub fn mqemu_texture_destroy(texture: *mut MQTexture);
    /// Its `ImTextureID` now, or null while the device is lost. Ask every
    /// frame: MacroQuest recreates textures after a device reset.
    pub fn mqemu_texture_id(texture: *const MQTexture) -> *mut c_void;
    pub fn mqemu_texture_size(texture: *const MQTexture, w: *mut i32, h: *mut i32);

    // ----- the game's UI textures -----
    /// A copy of the UI's animation `name` (e.g. `"A_DragItem"`,
    /// `"A_SpellIcons"`), to free with [`mqemu_texture_animation_free`]. Null
    /// if there's none, or no UI yet.
    pub fn mqemu_texture_animation_find(name: *const c_char, name_len: usize) -> *mut CTextureAnimation;
    pub fn mqemu_texture_animation_free(animation: *mut CTextureAnimation);
    /// For grids: the cell to draw (-1 for none).
    pub fn mqemu_texture_animation_set_cell(animation: *mut CTextureAnimation, cell: i32);
    pub fn mqemu_texture_animation_cell(animation: *const CTextureAnimation) -> i32;
    /// Its size, and a grid's cell size (0 when it isn't a grid).
    pub fn mqemu_texture_animation_size(animation: *const CTextureAnimation, w: *mut i32, h: *mut i32, cell_w: *mut i32, cell_h: *mut i32);
    /// Draws it at the cursor as an item `w` x `h` (0 for its own size),
    /// tinted with `tint`, framed with `border` unless that's transparent.
    pub fn mqemu_imgui_texture_animation(animation: *const CTextureAnimation, w: i32, h: i32, tint: u32, border: u32) -> bool;
    /// Draws it on `list` at (x, y), as [`mqemu_imgui_texture_animation`].
    pub fn mqemu_imgui_draw_texture_animation(
        list: *mut ImDrawList,
        animation: *const CTextureAnimation,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        tint: u32,
        border: u32,
    ) -> bool;

    // ----- EQ fonts -----
    /// The ImGui copy of EQ font style `style` (0 to 9). Null if there's none.
    pub fn mqemu_imgui_eq_font(style: i32) -> *mut ImFont;
}
