//! Hot buttons and hot bars: what a hot button holds (`${Window.HotButton}`)
//! and pressing it.

use std::fmt;

use mq_emu_sys::constants::{HOTBUTTONS_PER_PAGE, NUM_HOTBUTTON_WINDOWS, NUM_SOCIAL_PAGES, NUM_SPELL_GEMS, SOCIALS_PER_PAGE};
use mq_emu_sys::layouts::{CHotButton, CHotButtonWnd};

use super::{Window, WindowKind};
use crate::items::Item;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::{from_c_array, from_cxstr};

/// What a hot button holds (`HotButtonTypes`). [`name`](Self::name) is
/// `${Window.HotButton.TypeName}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotButtonType {
    None,
    WeaponSlot,
    CombatSkill,
    Ability,
    /// A social, or an alternate ability (socials past the last page).
    Social,
    InventorySlot,
    MenuButton,
    SpellGem,
    PetCommand,
    Skill,
    MeleeAbility,
    LeadershipAbility,
    ItemLink,
    KronoSlot,
    Command,
    CombatAbility,
    MountLink,
    IllusionLink,
    FamiliarLink,
    TeleportationLink,
    /// Another value.
    Other(i32),
}

impl HotButtonType {
    const ALL: [HotButtonType; 20] = {
        use HotButtonType::*;
        [
            None, WeaponSlot, CombatSkill, Ability, Social, InventorySlot, MenuButton, SpellGem, PetCommand, Skill, MeleeAbility,
            LeadershipAbility, ItemLink, KronoSlot, Command, CombatAbility, MountLink, IllusionLink, FamiliarLink, TeleportationLink,
        ]
    };

    pub fn from_raw(raw: i32) -> HotButtonType {
        usize::try_from(raw).ok().and_then(|i| HotButtonType::ALL.get(i).copied()).unwrap_or(HotButtonType::Other(raw))
    }

    /// The `HotButtonTypes` value (`${Window.HotButton.Type}`).
    pub fn raw(self) -> i32 {
        match self {
            HotButtonType::Other(raw) => raw,
            kind => HotButtonType::ALL.iter().position(|k| *k == kind).expect("in ALL") as i32,
        }
    }

    /// eqlib's name for it (`HotButtonTypeToString`): "SpellGem", "Social"...;
    /// "Unknown" for [`Other`](Self::Other).
    pub fn name(self) -> &'static str {
        use HotButtonType::*;
        match self {
            None => "None",
            WeaponSlot => "WeaponSlot",
            CombatSkill => "CombatSkill",
            Ability => "Ability",
            Social => "Social",
            InventorySlot => "InventorySlot",
            MenuButton => "MenuButton",
            SpellGem => "SpellGem",
            PetCommand => "PetCommand",
            Skill => "Skill",
            MeleeAbility => "MeleeAbility",
            LeadershipAbility => "LeadershipAbility",
            ItemLink => "ItemLink",
            KronoSlot => "KronoSlot",
            Command => "Command",
            CombatAbility => "CombatAbility",
            MountLink => "MountLink",
            IllusionLink => "IllusionLink",
            FamiliarLink => "FamiliarLink",
            TeleportationLink => "TeleportationLink",
            Other(_) => "Unknown",
        }
    }
}

/// Socials a hot button's slot can name; slots past these are alternate
/// abilities, by ID.
const SOCIAL_SLOTS: usize = NUM_SOCIAL_PAGES * SOCIALS_PER_PAGE;

/// One hot button (`${Window.HotButton}`), as it was last drawn: what's on
/// the page its bar shows.
#[derive(Clone, Copy)]
pub struct HotButtonWindow<'a> {
    window: Window<'a>,
    button: &'a CHotButton,
}

impl<'a> HotButtonWindow<'a> {
    pub fn window(&self) -> Window<'a> {
        self.window
    }

    pub fn raw(&self) -> &'a CHotButton {
        self.button
    }

    /// Its bar, from 0 (`${Window.HotButton.BarIndex}` is it plus one): 0 is
    /// `HotButtonWnd`, 1 is `HotButtonWnd2`...
    pub fn bar(&self) -> usize {
        usize::try_from(self.button.BarIndex()).unwrap_or(0)
    }

    /// Which button of its bar it is, from 0 (`${Window.HotButton.ButtonIndex}`
    /// is it plus one).
    pub fn index(&self) -> usize {
        usize::try_from(self.button.ButtonIndex()).unwrap_or(0)
    }

    /// What it holds (`${Window.HotButton.Type}`).
    pub fn button_type(&self) -> HotButtonType {
        HotButtonType::from_raw(self.button.LastButtonType().0)
    }

    /// Its slot (`${Window.HotButton.Slot}`): which spell gem, social,
    /// inventory slot, ability... it holds, as its type says.
    pub fn slot(&self) -> i32 {
        self.button.LastButtonSlot()
    }

    /// Its label (`${Window.HotButton.Label}`), line breaks as spaces.
    pub fn label(&self) -> String {
        let last = from_cxstr(self.button.LastLabel());
        let label = if last.is_empty() { from_cxstr(self.button.DefaultLabel()) } else { last };
        label.replace(['\r', '\n'], " ")
    }

    /// The spell gem it casts from, from 0 (the spell is [`Mq::gem`]).
    pub fn gem(&self) -> Option<usize> {
        let gem = usize::try_from(self.slot()).ok()?;
        (self.button_type() == HotButtonType::SpellGem && gem < NUM_SPELL_GEMS).then_some(gem)
    }

    /// The social it runs, from 0 (`${Window.HotButton.Social}`).
    pub fn social(&self) -> Option<usize> {
        let social = usize::try_from(self.slot()).ok()?;
        (self.button_type() == HotButtonType::Social && social < SOCIAL_SLOTS).then_some(social)
    }

    /// The ID of the alternate ability it activates (the ability is
    /// [`Mq::alt_ability`], as `${Window.HotButton.AltAbility}` finds it).
    pub fn alt_ability_id(&self) -> Option<i32> {
        let slot = usize::try_from(self.slot()).ok()?;
        let id = slot.checked_sub(SOCIAL_SLOTS)?;
        (self.button_type() == HotButtonType::Social).then(|| i32::try_from(id).ok()).flatten()
    }

    /// The item it holds (`${Window.HotButton.Item}`).
    pub fn item(&self) -> Option<Item<'a>> {
        let button = std::ptr::from_ref(self.button).cast_mut();
        // SAFETY: the item is held by the game's hot button data.
        unsafe { Item::from_raw((self.window.api.mqemu_hot_button_item)(button)) }
    }

    /// The ID of the item it was last given (`${Window.HotButton.ItemId}`).
    pub fn item_id(&self) -> i32 {
        self.button.LastItemId()
    }

    /// The GUID of the item it was last given (`${Window.HotButton.ItemGuid}`).
    pub fn item_guid(&self) -> String {
        from_c_array(&self.button.LastItemGuid().guid())
    }

    /// `${Window.HotButton.IconType}`.
    pub fn icon_type(&self) -> i32 {
        self.button.LastIconType()
    }

    /// `${Window.HotButton.IconSlot}`.
    pub fn icon_slot(&self) -> i32 {
        self.button.LastIconSlot()
    }

    /// Presses it, as its key would (`${Window.HotButton.Activate}`). False
    /// if its bar isn't found.
    pub fn activate(&self) -> bool {
        unsafe { (self.window.api.mqemu_hot_button_activate)(std::ptr::from_ref(self.button).cast_mut()) }
    }
}

impl fmt::Debug for HotButtonWindow<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HotButtonWindow")
            .field("bar", &self.bar())
            .field("index", &self.index())
            .field("type", &self.button_type())
            .field("slot", &self.slot())
            .field("label", &self.label())
            .finish()
    }
}

/// A hot bar (`HotButtonWnd`, `HotButtonWnd2`...): its buttons and the page it shows.
#[derive(Clone, Copy)]
pub struct HotBar<'a> {
    window: Window<'a>,
    bar: &'a CHotButtonWnd,
    page: usize,
}

impl<'a> HotBar<'a> {
    pub fn window(&self) -> Window<'a> {
        self.window
    }

    pub fn raw(&self) -> &'a CHotButtonWnd {
        self.bar
    }

    /// The page it shows, from 0 (`${Window.HotButton.PageIndex}` is it plus one).
    pub fn page(&self) -> usize {
        self.page
    }

    /// Its button `index`, from 0.
    pub fn button(&self, index: usize) -> Option<HotButtonWindow<'a>> {
        let raw = if index < HOTBUTTONS_PER_PAGE { self.bar.Buttons_at(index) } else { std::ptr::null_mut() };
        self.window.related(raw.cast())?.as_hot_button()
    }

    pub fn buttons(&self) -> impl Iterator<Item = HotButtonWindow<'a>> + use<'a> {
        let bar = *self;
        (0..HOTBUTTONS_PER_PAGE).filter_map(move |index| bar.button(index))
    }
}

impl fmt::Debug for HotBar<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HotBar").field("window", &self.window).field("page", &self.page).finish()
    }
}

impl<'a> Window<'a> {
    /// A hot button (`${Window.HotButton}`).
    pub fn as_hot_button(&self) -> Option<HotButtonWindow<'a>> {
        (self.kind() == WindowKind::HotButton).then(|| HotButtonWindow { window: *self, button: self.cast::<CHotButton>() })
    }
}

impl<P: Plugin> Mq<P> {
    /// Hot bar `bar`, from 0 (`HotButtonWnd`, then `HotButtonWnd2`...), shown or not.
    pub fn hot_bar(&self, bar: usize) -> Option<HotBar<'_>> {
        if bar >= NUM_HOTBUTTON_WINDOWS {
            return None;
        }
        let name = if bar == 0 { "HotButtonWnd".to_owned() } else { format!("HotButtonWnd{}", bar + 1) };
        let window = self.window(&name).filter(|window| window.kind() == WindowKind::Screen)?;
        // The page each bar shows is kept with the game's other state
        // (`EverQuestinfo::hotBank`), as `${Window.HotButton.PageIndex}` reads it.
        let info = unsafe { (self.host().everquest_info)().as_ref() }?;
        let page = usize::try_from(info.hotBank()[bar]).unwrap_or(0);
        Some(HotBar { window, bar: window.cast::<CHotButtonWnd>(), page })
    }
}

#[cfg(test)]
mod tests {
    use mq_emu_sys::constants::{UI_BUTTON, UI_HOT_BUTTON};
    use mq_emu_sys::layouts::HotButtonTypes;

    use super::*;
    use crate::items::tests::{Fake, FakeItem, set_name};
    use crate::text::tests::cstrrep;
    use crate::windows::fake;

    #[test]
    fn types_match_eqlibs_numbers_and_names() {
        use HotButtonType::*;
        let eqlib = [
            (None, HotButtonTypes::HotButtonType_None),
            (WeaponSlot, HotButtonTypes::HotButtonType_WeaponSlot),
            (CombatSkill, HotButtonTypes::HotButtonType_CombatSkill),
            (Ability, HotButtonTypes::HotButtonType_Ability),
            (Social, HotButtonTypes::HotButtonType_Social),
            (InventorySlot, HotButtonTypes::HotButtonType_InventorySlot),
            (MenuButton, HotButtonTypes::HotButtonType_MenuButton),
            (SpellGem, HotButtonTypes::HotButtonType_SpellGem),
            (PetCommand, HotButtonTypes::HotButtonType_PetCommand),
            (Skill, HotButtonTypes::HotButtonType_Skill),
            (MeleeAbility, HotButtonTypes::HotButtonType_MeleeAbility),
            (LeadershipAbility, HotButtonTypes::HotButtonType_LeadershipAbility),
            (ItemLink, HotButtonTypes::HotButtonType_ItemLink),
            (KronoSlot, HotButtonTypes::HotButtonType_KronoSlot),
            (Command, HotButtonTypes::HotButtonType_Command),
            (CombatAbility, HotButtonTypes::HotButtonType_CombatAbility),
            (MountLink, HotButtonTypes::HotButtonType_MountLink),
            (IllusionLink, HotButtonTypes::HotButtonType_IllusionLink),
            (FamiliarLink, HotButtonTypes::HotButtonType_FamiliarLink),
            (TeleportationLink, HotButtonTypes::HotButtonType_TeleportationLink),
        ];
        assert_eq!(eqlib.len(), HotButtonType::ALL.len());
        for (kind, raw) in eqlib {
            assert_eq!((kind.raw(), HotButtonType::from_raw(raw.0)), (raw.0, kind));
            // HotButtonTypeToString names each as the enum does, less the prefix.
            assert_eq!(kind.name(), format!("{kind:?}"));
        }
        assert_eq!((HotButtonType::from_raw(20), HotButtonType::from_raw(-1)), (Other(20), Other(-1)));
        assert_eq!((Other(20).name(), Other(20).raw()), ("Unknown", 20));
    }

    fn button(kind: HotButtonType, slot: i32) -> Fake<CHotButton> {
        let button = Fake::<CHotButton>::new();
        fake::add(button.ptr().cast(), UI_HOT_BUTTON, None, Some("HB_Button1"));
        button.get().set_LastButtonType(HotButtonTypes(kind.raw()));
        button.get().set_LastButtonSlot(slot);
        button
    }

    fn window(button: &Fake<CHotButton>) -> HotButtonWindow<'_> {
        unsafe { Window::from_raw(button.ptr().cast(), &fake::API) }.unwrap().as_hot_button().unwrap()
    }

    #[test]
    fn what_a_slot_means_follows_the_type() {
        fake::reset();
        let cases = [
            (HotButtonType::SpellGem, 3, (Some(3), None, None)),
            (HotButtonType::SpellGem, 12, (None, None, None)),
            (HotButtonType::Social, 5, (None, Some(5), None)),
            (HotButtonType::Social, 119, (None, Some(119), None)),
            (HotButtonType::Social, 120 + 42, (None, None, Some(42))),
            (HotButtonType::Social, -1, (None, None, None)),
            (HotButtonType::Skill, 3, (None, None, None)),
            (HotButtonType::Skill, 200, (None, None, None)),
        ];
        for (kind, slot, expected) in cases {
            let button = button(kind, slot);
            let w = window(&button);
            assert_eq!((w.gem(), w.social(), w.alt_ability_id()), expected, "{kind:?} {slot}");
            assert_eq!((w.button_type(), w.slot()), (kind, slot));
        }
    }

    #[test]
    fn a_hot_button_reads_its_place_label_and_item() {
        fake::reset();
        let button = button(HotButtonType::InventorySlot, 13);
        let item = FakeItem::new(1001, "Rusty Sword");
        fake::with(|w| w.items.insert(button.ptr() as usize, item.item.ptr() as usize));
        let raw = button.get();
        raw.set_BarIndex(1);
        raw.set_ButtonIndex(11);
        raw.set_LastItemId(1001);
        raw.LastItemGuid().set_guid(set_name("0001-0002"));
        let default = cstrrep(0, 9, b"Rusty\r\nSw");
        raw.DefaultLabel().set_m_data(default.as_ptr().cast_mut().cast());
        let w = window(&button);
        assert_eq!((w.bar(), w.index(), w.item_id(), w.item_guid()), (1, 11, 1001, "0001-0002".to_owned()));
        assert_eq!(w.label(), "Rusty  Sw", "the default label, line breaks as spaces");
        let last = cstrrep(0, 4, b"Gate");
        raw.LastLabel().set_m_data(last.as_ptr().cast_mut().cast());
        assert_eq!(w.label(), "Gate", "the last label first");
        assert_eq!(w.item().map(|i| i.name()).as_deref(), Some("Rusty Sword"));
        assert!(w.activate());
        assert_eq!(fake::log(), ["activate HB_Button1"]);
        fake::add(button.ptr().cast(), UI_BUTTON, None, None);
        assert!(w.window().as_hot_button().is_none(), "only hot buttons");
    }
}
