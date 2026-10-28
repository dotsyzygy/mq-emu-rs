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

#[test]
fn ui_textures_and_their_pieces_reach_macroquest_with_their_sizes() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    assert!(ui.find_ui_texture(fake::MISSING).is_none());
    let texture = ui.find_ui_texture("AttackIndicator.tga").unwrap();
    assert_eq!(texture.name(), "AttackIndicator.tga");
    assert_eq!(texture.size(), fake::FAKE_TEXTURE_SIZE.map(|v| v as f32));
    let piece = texture.piece([10.2, 20.0], [30.0, 52.6]).unwrap();
    assert_eq!(piece.rect(), [[10.0, 20.0], [30.0, 53.0]]);
    assert_eq!(piece.size(), [20.0, 33.0]);
    assert_eq!(piece.texture().unwrap().name(), "AttackIndicator.tga");

    let icons = ui.find_texture_animation(ITEM_ICONS).unwrap();
    assert_eq!(icons.frame_count(), 1);
    assert!(icons.frame(1).is_none(), "past its frames");
    let frame0 = icons.frame(0).unwrap();
    assert_eq!(frame0.rect(), [[0.0, 0.0], [40.0, 40.0]]);
    assert_eq!(frame0.texture().unwrap().name(), fake::ANIMATION_TEXTURE);

    let before = fake::log().ui_drawn.len();
    let red = [1.0, 0.0, 0.0, 1.0];
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let whole = ui.ui_texture(&texture, None, UiImageOptions::default());
            let part = ui.ui_texture(&texture, Some([[0.0, 0.0], [16.0, 8.0]]), UiImageOptions { size: Some([32.0, 16.0]), tint: red, border: Some(red) });
            let item = ui.texture_piece(&piece, UiImageOptions::default());
            let list = ui.draw_list();
            let on_list = list.add_ui_texture(&texture, [5.0, 6.0], None, UiImageOptions::default());
            let piece_on_list = list.add_texture_piece(&frame0, [7.4, 8.6], UiImageOptions { size: Some([20.0, 20.0]), ..Default::default() });
            (whole, part, item, on_list, piece_on_list)
        })
    });
    assert_eq!(result, Ok(Some((true, true, true, true, true))));
    let drawn = |texture: &str, piece, at, size, source: [i32; 4], tint, border| fake::UiDrawn { texture: texture.to_owned(), piece, at, size, source, tint, border };
    let white = 0xffff_ffff;
    assert_eq!(&fake::log().ui_drawn[before..], [
        drawn("AttackIndicator.tga", false, None, [0, 0], [0, 0, -1, -1], white, 0),
        drawn("AttackIndicator.tga", false, None, [32, 16], [0, 0, 16, 8], pack_color(red), pack_color(red)),
        drawn("AttackIndicator.tga", true, None, [0, 0], [10, 20, 30, 53], white, 0),
        drawn("AttackIndicator.tga", false, Some([5, 6]), [0, 0], [0, 0, -1, -1], white, 0),
        drawn(fake::ANIMATION_TEXTURE, true, Some([7, 9]), [20, 20], [0, 0, 40, 40], white, 0),
    ]);
    assert_eq!(take_error(), None);
}

#[test]
fn ui_textures_pieces_and_screen_pieces_stop_when_the_ui_unloads() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    let texture = ui.find_ui_texture("AttackIndicator.tga").unwrap();
    let piece = texture.piece([0.0; 2], [8.0; 2]).unwrap();
    let screen = ui.find_screen_piece(fake::DRAWN_PIECE).unwrap();
    assert!(texture.is_current() && piece.is_current() && screen.is_current());

    crate::imgui::ui_unloaded();
    assert!(!texture.is_current() && !piece.is_current() && !screen.is_current());
    assert!(texture.piece([0.0; 2], [8.0; 2]).is_none(), "no new pieces of a texture that's gone");
    assert!(piece.texture().is_none());
    assert_eq!(screen.kind(), ScreenPieceKind::Unknown);
    assert!(!screen.draws());
    assert_eq!(screen.rect(), [[0.0; 2]; 2]);
    let (drawn, pieces) = (fake::log().ui_drawn.len(), fake::log().screen_pieces.len());
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let items = [ui.ui_texture(&texture, None, UiImageOptions::default()), ui.texture_piece(&piece, UiImageOptions::default())];
            let list = ui.draw_list();
            [
                items[0],
                items[1],
                list.add_ui_texture(&texture, [0.0; 2], None, UiImageOptions::default()),
                list.add_texture_piece(&piece, [0.0; 2], UiImageOptions::default()),
                list.add_screen_piece(&screen, [0.0; 2], [100.0; 2]),
            ]
        })
    });
    assert_eq!(result, Ok(Some([false; 5])));
    assert_eq!((fake::log().ui_drawn.len(), fake::log().screen_pieces.len()), (drawn, pieces), "nothing reached the game");

    let log = fake::log();
    drop((texture, piece, screen));
    let after = fake::log();
    assert_eq!((after.ui_textures_freed, after.pieces_freed), (log.ui_textures_freed + 1, log.pieces_freed + 1), "copies are freed; a screen piece is the game's");
}

#[test]
fn screen_pieces_draw_only_the_kinds_macroquest_draws() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    assert!(ui.find_screen_piece("NoSuchPiece").is_none());
    let drawn = ui.find_screen_piece(fake::DRAWN_PIECE).unwrap();
    let text = ui.find_screen_piece(fake::TEXT_PIECE).unwrap();
    assert_eq!((drawn.kind(), drawn.draws()), (ScreenPieceKind::Animation, true));
    assert_eq!((text.kind(), text.draws()), (ScreenPieceKind::Text, false));
    assert_eq!(drawn.rect(), [[1.0, 117.0], [101.0, 119.0]]);
    let before = fake::log().screen_pieces.len();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let list = ui.draw_list();
            (list.add_screen_piece(&drawn, [10.0, 20.4], [210.0, 420.6]), list.add_screen_piece(&text, [0.0; 2], [100.0; 2]))
        })
    });
    assert_eq!(result, Ok(Some((true, false))));
    assert_eq!(&fake::log().screen_pieces[before..], [(fake::DRAWN_PIECE.to_owned(), [10, 20, 210, 421])]);
}
