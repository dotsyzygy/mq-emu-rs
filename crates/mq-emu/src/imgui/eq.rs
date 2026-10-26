//! MacroQuest's widgets in EverQuest's style: spell gems as the game draws
//! them, text styled like an item link, and text laid out in EQ's fonts.

use mq_emu_sys::imgui_mq as mq_sys;

use super::draw::{DrawList, Vec2, pack_color};
use super::{Color, Ui, s};

flags! {
    /// What clicking a [`Ui::spell_gem`] does (`ImGuiSpellGemFlags_`); by
    /// default, nothing but report the click.
    SpellGemFlags {
        /// A left click casts the spell, as clicking the game's gem would.
        ALLOW_SPELL_CAST = ImGuiSpellGemFlags_AllowSpellCast,
        /// A right click forgets the spell.
        ALLOW_UNMEMORIZE = ImGuiSpellGemFlags_AllowUnmemorize,
        /// Holding the left button picks the spell up, to drop elsewhere.
        ALLOW_SPELL_DRAG = ImGuiSpellGemFlags_AllowSpellDrag,
        ALLOW_ALL = ImGuiSpellGemFlags_AllowAll,
    }
    in mq_sys
}

flags! {
    /// How [`DrawList::add_eq_text`] lays text out (eqlib's `DrawTextFlags`);
    /// by default wrapped, from the top left.
    EqTextFlags {
        NO_WRAP = DrawText_NoWrap,
        /// Ends text that doesn't fit with "...".
        ELIDE = DrawText_Elide,
        RIGHT_JUSTIFY = DrawText_RightJustify,
        H_CENTER = DrawText_HCenter,
        V_CENTER = DrawText_VCenter,
    }
    in mq_sys
}

/// MacroQuest's color for a hovered item link (`DefaultLinkHoverColor`).
const LINK_HOVER: Color = [0.0, 0.0, 1.0, 1.0];

fn gem_index(gem: usize) -> Option<i32> {
    (gem < mq_emu_sys::NUM_SPELL_GEMS).then_some(gem as i32)
}

impl Ui {
    /// Spell gem `gem` (0-based: the game's gem 1 is 0) as the game draws
    /// it, the size it is on the game's screen, as an item (hover checks and
    /// tooltips work on it). Clicks do what `flags` allow, as clicks on the
    /// game's gem would. `id` identifies it (it shows no label). True when
    /// clicked; None, drawing nothing, if there's no such gem (no UI loaded,
    /// or `gem` is [`NUM_SPELL_GEMS`](mq_emu_sys::NUM_SPELL_GEMS) or more).
    pub fn spell_gem(&mut self, id: &str, gem: usize, flags: SpellGemFlags) -> Option<bool> {
        let gem = gem_index(gem)?;
        let (i, n) = s(id);
        let mut drawn = false;
        let clicked = unsafe { (self.mq.mqemu_imgui_spell_gem)(i, n, gem, flags.bits(), &mut drawn) };
        drawn.then_some(clicked)
    }

    /// `text` as is, styled like an item link: `color`, or `hovered_color`
    /// (None for MacroQuest's blue) under the mouse. Its ID is its text.
    /// True when clicked.
    pub fn item_link_text(&mut self, text: &str, color: Color, hovered_color: Option<Color>) -> bool {
        let (t, n) = s(text);
        let hovered = pack_color(hovered_color.unwrap_or(LINK_HOVER));
        unsafe { (self.mq.mqemu_imgui_item_link_text)(t, n, pack_color(color), hovered) }
    }
}

impl DrawList<'_> {
    /// Spell gem `gem` (as for [`Ui::spell_gem`]) with its top left at
    /// `pos`, highlighted if `hovered`. False if there's no such gem.
    pub fn add_spell_gem(&self, gem: usize, pos: Vec2, hovered: bool) -> bool {
        let (Some(gem), Some(list)) = (gem_index(gem), self.raw()) else { return false };
        unsafe { (self.mq_api().mqemu_imgui_draw_spell_gem)(list, gem, pos[0].round() as i32, pos[1].round() as i32, hovered) }
    }

    /// `text` as the game lays it out in EQ font style `font_style` (0 to 9,
    /// as in its font options), wrapped and clipped to the rectangle from
    /// `min` to `max`. Nothing if the game has no such font (or no UI).
    pub fn add_eq_text(&self, font_style: u8, text: &str, min: Vec2, max: Vec2, color: Color, flags: EqTextFlags) {
        let Some(list) = self.raw() else { return };
        let (t, n) = s(text);
        let [x0, y0, x1, y1] = [min[0], min[1], max[0], max[1]].map(|v| v.round() as i32);
        unsafe { (self.mq_api().mqemu_imgui_draw_eq_text)(list, i32::from(font_style), t, n, x0, y0, x1, y1, pack_color(color), flags.bits()) }
    }
}
