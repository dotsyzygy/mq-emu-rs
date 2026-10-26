//! Fonts: MacroQuest's, EverQuest's, and font files a plugin loads.

use std::cell::RefCell;
use std::ptr::{self, NonNull};

use mq_emu_sys::imgui as sys;
use mq_emu_sys::imgui_mq as mq_sys;

use super::{EndOnDrop, Ui, s};

/// A font to draw text in ([`Ui::with_font`], [`DrawList::add_text_with`](super::DrawList::add_text_with)).
/// A font that isn't available (an EQ font before the game made it, say)
/// leaves the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Font {
    /// MacroQuest's default: Roboto 16 px, with Font Awesome and Material
    /// Design icons.
    Default,
    /// MacroQuest's console font: Lucida Console 13 px.
    Console,
    /// MacroQuest's large text font: Roboto 22 px, with the icons.
    Large,
    /// EverQuest's font style 0 to 9, as in the game's font options.
    Eq(u8),
    /// A font file the plugin loaded ([`Ui::load_font`]).
    Loaded(LoadedFont),
}

impl Font {
    /// The font, or null if it isn't available.
    pub(super) fn raw(self, api: &sys::Api, mq: &mq_sys::Api) -> *mut sys::ImFont {
        unsafe {
            match self {
                Font::Default => (api.mqemu_imgui_mq_font)(0),
                Font::Console => (api.mqemu_imgui_mq_font)(1),
                Font::Large => (api.mqemu_imgui_mq_font)(2),
                Font::Eq(style) => (mq.mqemu_imgui_eq_font)(i32::from(style)),
                Font::Loaded(font) => font.0.as_ptr(),
            }
        }
    }
}

/// A font file loaded with [`Ui::load_font`]. It stays loaded until the
/// plugin unloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoadedFont(NonNull<sys::ImFont>);

/// A font file loaded, by path and size.
struct Loaded {
    path: String,
    size: u32,
    font: NonNull<sys::ImFont>,
    api: &'static sys::Api,
}

thread_local! {
    /// The plugin's loaded fonts, removed at shutdown.
    static LOADED: RefCell<Vec<Loaded>> = const { RefCell::new(Vec::new()) };
}

/// Removes every font the plugin loaded. Only at shutdown: nothing may use them afterwards.
pub(crate) fn remove_loaded_fonts() {
    let loaded = LOADED.with(|l| std::mem::take(&mut *l.borrow_mut()));
    for font in loaded {
        unsafe { (font.api.mqemu_imgui_remove_font)(font.font.as_ptr()) };
    }
}

fn check_size(size: f32) {
    assert!(size.is_finite() && size >= 0.0, "a font size is 0 (keep the current one) or more pixels, not {size}");
}

impl Ui {
    /// Loads a TrueType or OpenType font file (relative paths are from the
    /// game's directory) at `size` pixels. None if it can't be read.
    ///
    /// Loading the same file at the same size again returns the same font, so
    /// this may be called every frame; fonts stay loaded until the plugin
    /// unloads.
    ///
    /// # Panics
    /// If `size` isn't a finite number above 0.
    pub fn load_font(&mut self, path: &str, size: f32) -> Option<LoadedFont> {
        assert!(size.is_finite() && size > 0.0, "a font is loaded at a size above 0, not {size}");
        let key = size.to_bits();
        if let Some(font) = LOADED.with(|l| l.borrow().iter().find(|f| f.path == path && f.size == key).map(|f| f.font)) {
            return Some(LoadedFont(font));
        }
        let (p, n) = s(path);
        let font = NonNull::new(unsafe { (self.api.mqemu_imgui_add_font_file)(p, n, size) })?;
        LOADED.with(|l| l.borrow_mut().push(Loaded { path: path.to_owned(), size: key, font, api: self.api }));
        Some(LoadedFont(font))
    }

    /// Draws `f` in `font`, at the font's own size.
    pub fn with_font<R>(&mut self, font: Font, f: impl FnOnce(&mut Ui) -> R) -> R {
        let raw = font.raw(self.api, self.mq);
        let size = if raw.is_null() { 0.0 } else { unsafe { (self.api.mqemu_imgui_font_own_size)(raw) } };
        self.push_font(raw, size, f)
    }

    /// Draws `f` in `font` at `size` pixels (before MacroQuest's global
    /// scaling; 0 keeps the current size).
    ///
    /// # Panics
    /// If `size` is negative or not a number.
    pub fn with_font_sized<R>(&mut self, font: Font, size: f32, f: impl FnOnce(&mut Ui) -> R) -> R {
        check_size(size);
        let raw = font.raw(self.api, self.mq);
        self.push_font(raw, size, f)
    }

    /// Draws `f` in the current font at `size` pixels, e.g.
    /// `ui.font_size_base() * 1.5` for half again as big.
    ///
    /// # Panics
    /// If `size` is negative or not a number.
    pub fn with_font_size<R>(&mut self, size: f32, f: impl FnOnce(&mut Ui) -> R) -> R {
        check_size(size);
        self.push_font(ptr::null_mut(), size, f)
    }

    fn push_font<R>(&mut self, font: *mut sys::ImFont, size: f32, f: impl FnOnce(&mut Ui) -> R) -> R {
        unsafe { (self.api.mqemu_imgui_push_font)(font, size) };
        let _pop = EndOnDrop(self.api.mqemu_imgui_pop_font);
        f(self)
    }

    /// The current font's size in pixels, as drawn (after global scaling).
    pub fn font_size(&self) -> f32 {
        unsafe { (self.api.mqemu_imgui_font_size)() }
    }

    /// The current font's size before global scaling: what
    /// [`Ui::with_font_size`] takes.
    pub fn font_size_base(&self) -> f32 {
        unsafe { (self.api.mqemu_imgui_font_size_base)() }
    }

    /// Whether `font` is available now.
    pub fn has_font(&self, font: Font) -> bool {
        !font.raw(self.api, self.mq).is_null()
    }
}
