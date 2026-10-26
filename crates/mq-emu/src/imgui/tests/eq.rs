//! MacroQuest's EQ-styled widgets, against the fake MacroQuest: what reaches
//! it, and gems the game doesn't have.

use super::*;
use crate::imgui::fake;

#[test]
fn spell_gems_draw_only_gems_the_game_has() {
    let _imgui = lock();
    let before = fake::log().spell_gems.len();
    let drawn = frame(|ui| {
        test_window(ui, |ui| {
            let flags = SpellGemFlags::ALLOW_SPELL_CAST | SpellGemFlags::ALLOW_UNMEMORIZE;
            let gems: Vec<Option<bool>> = (0..mq_emu_sys::NUM_SPELL_GEMS + 2).map(|gem| ui.with_id(gem, |ui| ui.spell_gem("gem", gem, flags))).collect();
            let draw = ui.draw_list();
            (gems, draw.add_spell_gem(2, [10.4, 20.6], true), draw.add_spell_gem(mq_emu_sys::NUM_SPELL_GEMS, [0.0, 0.0], false))
        })
    })
    .unwrap()
    .unwrap();
    let fake_gems = fake::FAKE_GEMS as usize;
    let (gems, on_list, past_the_end) = drawn;
    assert!(gems[..fake_gems].iter().all(|g| *g == Some(false)), "drawn, not clicked: {gems:?}");
    assert!(gems[fake_gems..].iter().all(Option::is_none), "the fake has no gem {fake_gems} or later: {gems:?}");
    assert!(on_list && !past_the_end);
    let seen = &fake::log().spell_gems[before..];
    let flags = (SpellGemFlags::ALLOW_SPELL_CAST | SpellGemFlags::ALLOW_UNMEMORIZE).bits();
    let mut expected: Vec<(i32, i32, Option<[i32; 2]>)> = (0..fake::FAKE_GEMS).map(|gem| (gem, flags, None)).collect();
    expected.push((2, -1, Some([10, 21])));
    assert_eq!(seen, expected, "gems past the game's last aren't even asked for");
    assert_eq!(take_error(), None);
}

#[test]
fn item_links_and_eq_text_reach_macroquest() {
    let _imgui = lock();
    let (links_before, texts_before) = (fake::log().item_links.len(), fake::log().eq_texts.len());
    frame(|ui| {
        test_window(ui, |ui| {
            ui.item_link_text("Fine Steel Sword", [1.0, 1.0, 0.0, 1.0], None);
            ui.item_link_text("Cloth Cap", [1.0; 4], Some([0.0, 1.0, 0.0, 1.0]));
            ui.draw_list().add_eq_text(3, "Hail, adventurer", [10.2, 20.0], [200.0, 40.7], [1.0; 4], EqTextFlags::H_CENTER | EqTextFlags::NO_WRAP);
        })
    })
    .unwrap();
    let log = fake::log();
    assert_eq!(log.item_links[links_before..], [
        ("Fine Steel Sword".to_owned(), pack_color([1.0, 1.0, 0.0, 1.0]), pack_color([0.0, 0.0, 1.0, 1.0])),
        ("Cloth Cap".to_owned(), pack_color([1.0; 4]), pack_color([0.0, 1.0, 0.0, 1.0])),
    ]);
    let flags = mq_emu_sys::imgui_mq::DrawText_HCenter | mq_emu_sys::imgui_mq::DrawText_NoWrap;
    assert_eq!(log.eq_texts[texts_before..], [(3, "Hail, adventurer".to_owned(), [10, 20, 200, 41], pack_color([1.0; 4]), flags)]);
    assert_eq!(take_error(), None);
}
