//! ImGui drawing that needs MacroQuest itself, from `shim/imgui_mq.cpp`:
//! textures loaded through the game's renderer, the game's own UI textures
//! (item and spell icons, window art), EQ's fonts in ImGui, and MacroQuest's
//! EQ-styled widgets (spell gems, item link text, text in EQ's fonts).
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
    /// `text` as the game draws it in font style `style`, wrapped and clipped
    /// to (x0, y0)-(x1, y1); `flags` are `DrawText_` values. Nothing without the font.
    pub fn mqemu_imgui_draw_eq_text(
        list: *mut ImDrawList,
        style: i32,
        text: *const c_char,
        len: usize,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        col: u32,
        flags: i32,
    );

    // ----- EQ-styled widgets -----
    /// Spell gem `gem` (0-based, below [`NUM_SPELL_GEMS`](crate::NUM_SPELL_GEMS)) as the game draws it, as an item; clicks
    /// cast, unmemorize or pick up the spell as `flags` (`ImGuiSpellGemFlags_`)
    /// allow. True when clicked; `*drawn` false if there's no such gem (no UI).
    pub fn mqemu_imgui_spell_gem(id: *const c_char, id_len: usize, gem: i32, flags: i32, drawn: *mut bool) -> bool;
    /// Spell gem `gem` on `list` at (x, y). False if there's no such gem.
    pub fn mqemu_imgui_draw_spell_gem(list: *mut ImDrawList, gem: i32, x: i32, y: i32, hovered: bool) -> bool;
    /// Text as is, styled like an item link (`col`, `hovered` under the mouse).
    /// Its ID is the text's. True when clicked.
    pub fn mqemu_imgui_item_link_text(text: *const c_char, len: usize, col: u32, hovered: u32) -> bool;
}

macro_rules! mq_constants {
    ($list:ident, $header:literal; $($name:ident = $value:expr,)*) => {
        $( pub const $name: i32 = $value; )*

        #[doc = concat!("Every constant in this group with its name, for checking against `", $header, "`.")]
        #[doc(hidden)]
        pub const $list: &[(&str, i32)] = &[$( (stringify!($name), $name) ),*];
    };
}

// From MacroQuest's include/mq/imgui/Widgets.h. tests/imgui_constants.rs
// checks these against it, and the text flags against eqlib's
// graphics/GraphicsEngine.h.
mq_constants! {
    SPELL_GEM_CONSTANTS, "mq/imgui/Widgets.h";
    ImGuiSpellGemFlags_None = 0,
    ImGuiSpellGemFlags_AllowSpellCast = 1 << 0,
    ImGuiSpellGemFlags_AllowUnmemorize = 1 << 1,
    ImGuiSpellGemFlags_AllowSpellDrag = 1 << 2,
    ImGuiSpellGemFlags_AllowAll = 7,
}

mq_constants! {
    DRAW_TEXT_CONSTANTS, "eqlib/graphics/GraphicsEngine.h";
    DrawText_NoWrap = 0x01,
    DrawText_CalcOnly = 0x02,
    DrawText_Elide = 0x04,
    DrawText_RightJustify = 0x08,
    DrawText_HCenter = 0x10,
    DrawText_VCenter = 0x20,
}
