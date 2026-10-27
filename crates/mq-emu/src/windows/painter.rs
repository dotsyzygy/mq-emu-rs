//! Drawing on an EQ window as the game draws its own: colored rectangles,
//! text in EQ's fonts and the game's UI textures, from a window's
//! [`on_draw`](super::WindowHooks::on_draw) hook.

use std::fmt;

use mq_emu_sys::windows::{Api, mqemu_rect};

use super::{Window, WindowRect};
use crate::imgui::{EqTextFlags, TextureAnimation};
use crate::text::to_c;

/// Draws on a window while the game draws it, for
/// [`WindowHooks::on_draw`](super::WindowHooks::on_draw). Places are on
/// screen (as [`Window::screen_rect`] places windows), and what's drawn is
/// clipped to the window's client area as far as it shows
/// ([`clip_rect`](Self::clip_rect)), or less ([`clipped`](Self::clipped)).
/// Colors are ARGB, as [`Window::bg_color`] reads them.
pub struct Painter<'a> {
    window: Window<'a>,
    clip: WindowRect,
}

impl<'a> Painter<'a> {
    pub(super) fn new(window: Window<'a>) -> Painter<'a> {
        let mut clip = mqemu_rect::default();
        // Nothing shows if the window can't say where its client area is.
        if !unsafe { (window.api.mqemu_window_client_clip_rect)(window.ptr(), &mut clip) } {
            clip = mqemu_rect::default();
        }
        Painter { window, clip: WindowRect { left: clip.left, top: clip.top, right: clip.right, bottom: clip.bottom } }
    }

    fn api(&self) -> &'static Api {
        self.window.api
    }

    /// The window being drawn.
    pub fn window(&self) -> Window<'a> {
        self.window
    }

    /// Where drawing shows: the window's client area on screen (inside its
    /// border and title bar), as far as it shows.
    pub fn clip_rect(&self) -> WindowRect {
        self.clip
    }

    /// A painter that only draws inside `rect` too.
    pub fn clipped(&self, rect: WindowRect) -> Painter<'a> {
        let clip = WindowRect {
            left: self.clip.left.max(rect.left),
            top: self.clip.top.max(rect.top),
            right: self.clip.right.min(rect.right),
            bottom: self.clip.bottom.min(rect.bottom),
        };
        Painter { window: self.window, clip }
    }

    fn clip(&self) -> mqemu_rect {
        rect(self.clip)
    }

    /// Fills `rect` with `color` (`CXWnd::DrawColoredRect`).
    pub fn fill_rect(&self, rect_: WindowRect, color: u32) {
        let (r, clip) = (rect(rect_), self.clip());
        unsafe { (self.api().mqemu_draw_fill_rect)(&r, color, &clip) };
    }

    /// Draws `text` in `rect` in EQ font style `font_style` (0 to 9, as in
    /// the game's font options), wrapped unless `flags` say otherwise. False
    /// if the game has no such font.
    pub fn text(&self, font_style: u8, text: &str, rect_: WindowRect, color: u32, flags: EqTextFlags) -> bool {
        let (r, clip, text) = (rect(rect_), self.clip(), to_c(text));
        unsafe { (self.api().mqemu_draw_text)(i32::from(font_style), text.as_ptr(), &r, &clip, color, flags.bits() as u32) }
    }

    /// How wide `text` is on one line in EQ font style `font_style`, in
    /// pixels. None without the font.
    pub fn text_width(&self, font_style: u8, text: &str) -> Option<i32> {
        let text = to_c(text);
        let width = unsafe { (self.api().mqemu_text_width)(i32::from(font_style), text.as_ptr()) };
        (width >= 0).then_some(width)
    }

    /// How tall a line of EQ font style `font_style` is, in pixels. None
    /// without the font.
    pub fn line_height(&self, font_style: u8) -> Option<i32> {
        let height = unsafe { (self.api().mqemu_font_height)(i32::from(font_style)) };
        (height >= 0).then_some(height)
    }

    /// Draws one of the game's UI textures (such as an icon, with its cell
    /// set) stretched to `rect`, tinted `tint` (0xffffffff for none). False
    /// if the UI it came from was unloaded.
    pub fn texture(&self, animation: &TextureAnimation, rect_: WindowRect, tint: u32) -> bool {
        let Some(raw) = animation.current() else { return false };
        let (r, clip) = (rect(rect_), self.clip());
        unsafe { (self.api().mqemu_draw_texture)(raw.cast_mut().cast(), &r, &clip, tint, 0xff00_0000) }
    }
}

fn rect(rect: WindowRect) -> mqemu_rect {
    mqemu_rect { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom }
}

impl fmt::Debug for Painter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Painter").field("window", &self.window.name()).field("clip", &self.clip).finish()
    }
}
