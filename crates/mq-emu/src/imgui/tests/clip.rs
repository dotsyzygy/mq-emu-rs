//! The list clipper: only the items in view draw, the cursor still moves
//! past the whole list, and the clipper ends even when its closure panics.

use std::ops::Range;

use super::*;

const ITEMS: usize = 10_000;

/// A clipped list of `ITEMS` lines in the test window, `lines_above` lines of
/// it above the window's top: the items drawn, the ranges they came in, and
/// how far the cursor moved over the list.
fn draw_lines(ui: &mut Ui, lines_above: f32, configure: impl FnOnce(ListClipper<'_, Ui>) -> ListClipper<'_, Ui>) -> (Vec<usize>, Vec<Range<usize>>, f32) {
    test_window(ui, |ui| {
        let [x, y] = ui.cursor_screen_pos();
        let top = y - lines_above * ui.text_line_height_with_spacing();
        ui.set_cursor_screen_pos([x, top]);
        let (mut drawn, mut ranges) = (Vec::new(), Vec::new());
        configure(ui.list_clipper(ITEMS)).show(|ui, visible| {
            ranges.push(visible.clone());
            for i in visible {
                ui.text(format!("line {i}"));
                drawn.push(i);
            }
        });
        (drawn, ranges, ui.cursor_screen_pos()[1] - top)
    })
    .unwrap()
}

fn line_height() -> f32 {
    frame(|ui| ui.text_line_height_with_spacing()).unwrap()
}

#[test]
fn only_the_items_in_view_draw_and_the_cursor_moves_past_the_list() {
    let _imgui = lock();
    let (drawn, ranges, height) = frame(|ui| draw_lines(ui, 0.0, |c| c)).unwrap();
    assert_eq!(ranges.first(), Some(&(0..1)), "the first item is drawn alone, to measure it");
    assert!(drawn.len() < 50, "{} items drawn in a 600 px window", drawn.len());
    assert_eq!(drawn, (0..drawn.len()).collect::<Vec<_>>(), "from the top, in order, each once");
    let expected = ITEMS as f32 * line_height();
    assert!((height - expected).abs() < 1.0, "the cursor moved {height} px past {ITEMS} lines, not {expected}");
    assert_eq!(take_error(), None);
}

#[test]
fn the_items_drawn_follow_the_view() {
    let _imgui = lock();
    let (drawn, _, _) = frame(|ui| draw_lines(ui, 5000.0, |c| c)).unwrap();
    // Item 0, to measure, then those in view.
    assert_eq!(drawn[0], 0);
    let rest = &drawn[1..];
    assert!(rest.iter().all(|i| (4990..5050).contains(i)), "lines 5000 on are in view: {rest:?}");
    assert!(rest.contains(&5000) && rest.contains(&5020), "{rest:?}");
    assert_eq!(take_error(), None);
}

#[test]
fn a_given_height_skips_measuring_and_included_items_draw_out_of_view() {
    let _imgui = lock();
    let height = line_height();
    let (drawn, ranges, moved) = frame(|ui| draw_lines(ui, 0.0, |c| c.items_height(height).include(7000..7002).include(3..3))).unwrap();
    assert_eq!(ranges.len(), 2, "the view and the included items: {ranges:?}");
    assert_eq!(ranges[0].start, 0);
    assert_eq!(ranges[1], 7000..7002);
    assert!(drawn.contains(&7001));
    assert!((moved - ITEMS as f32 * height).abs() < 1.0);
    assert_eq!(take_error(), None);
}

#[test]
fn an_empty_list_draws_nothing() {
    let _imgui = lock();
    let calls = frame(|ui| {
        test_window(ui, |ui| {
            let mut calls = 0;
            ui.list_clipper(0).show(|_, _| calls += 1);
            calls
        })
    });
    assert_eq!(calls, Ok(Some(0)));
    assert_eq!(take_error(), None);
}

#[test]
fn a_clipper_ends_when_its_closure_panics_and_clippers_nest() {
    let _imgui = lock();
    let draw = |ui: &mut Ui| {
        test_window(ui, |ui| {
            let panicked = catch_unwind(AssertUnwindSafe(|| {
                // Short, as it still moves the cursor past all of its items.
                ui.list_clipper(5).show(|ui, visible| {
                    for i in visible {
                        ui.text(format!("{i}"));
                        if i == 3 {
                            panic!("deliberate");
                        }
                    }
                })
            }))
            .is_err();
            assert_eq!(unsafe { mqemu_imgui_test_clipper_depth() }, 0, "the panic ended the clipper");
            let mut inner_drawn = 0;
            ui.list_clipper(100).show(|ui, visible| {
                for i in visible {
                    ui.child(&format!("child {i}")).size([0.0, 60.0]).show(|ui| {
                        ui.list_clipper(1000).show(|ui, visible| {
                            for j in visible {
                                ui.text(format!("{i}.{j}"));
                                inner_drawn += 1;
                                assert_eq!(unsafe { mqemu_imgui_test_clipper_depth() }, 2);
                            }
                        });
                    });
                }
            });
            (panicked, inner_drawn > 0)
        })
    };
    assert_eq!(frame(draw), Ok(Some((true, true))));
    assert_eq!(take_error(), None);
}

#[test]
fn a_table_clips_its_rows() {
    let _imgui = lock();
    let drawn = frame(|ui| {
        test_window(ui, |ui| {
            let mut drawn = Vec::new();
            ui.table("clipped", 2).flags(TableFlags::SCROLL_Y | TableFlags::ROW_BG).outer_size([0.0, 300.0]).show(|table| {
                table.setup_column("index");
                table.setup_column("name");
                table.setup_scroll_freeze(0, 1);
                table.headers_row();
                table.list_clipper(ITEMS).show(|table, rows| {
                    for i in rows {
                        table.row(&[i.to_string(), format!("row {i}")]);
                        drawn.push(i);
                    }
                });
            });
            drawn
        })
    })
    .unwrap()
    .unwrap();
    assert!(drawn.len() < 30 && drawn.first() == Some(&0), "{drawn:?}");
    assert_eq!(take_error(), None);
}

#[test]
fn a_multi_select_list_always_draws_the_item_shift_click_selects_from() {
    let _imgui = lock();
    let mut selection = Selection::new();
    // Item 1 at the top of the window, or 1000 lines further down the list.
    let draw = |ui: &mut Ui, selection: &mut Selection, lines_above: f32| {
        test_window(ui, |ui| {
            let [x, y] = ui.cursor_screen_pos();
            ui.set_cursor_screen_pos([x, y - lines_above * ui.text_line_height_with_spacing()]);
            let mut drawn = Vec::new();
            ui.multi_select(MultiSelectFlags::NONE, selection, ITEMS, |list, selection| {
                list.list_clipper(ITEMS).show(|list, visible| {
                    for i in visible {
                        list.set_next_item_index(i);
                        list.selectable(&format!("item {i}"), selection.contains(i));
                        drawn.push((i, last_item_center()));
                    }
                });
            });
            drawn
        })
        .unwrap()
    };
    let item_1 = frame(|ui| draw(ui, &mut selection, 0.0)).unwrap()[1].1;
    for down in [false, true, false] {
        frame_at(item_1, down, |ui| draw(ui, &mut selection, 0.0)).unwrap();
    }
    assert_eq!(selection.iter().collect::<Vec<_>>(), [1]);
    let drawn: Vec<usize> = frame(|ui| draw(ui, &mut selection, 1000.0)).unwrap().into_iter().map(|(i, _)| i).collect();
    assert!(drawn.contains(&1), "item 1, where shift+click selects from, is drawn: {drawn:?}");
    assert!(drawn.contains(&1010), "{drawn:?}");
    assert_eq!(take_error(), None);
}

/// Why `lock` forgets focus: a clicked item becomes its window's navigation
/// item, and ImGui's clipper draws the rows where that item was last drawn
/// in every later list in the window. A test drawing its clicked item far out
/// of view (as a multi-select list draws where shift+click selects from) made
/// a later test's list draw rows it shouldn't.
#[test]
fn a_clicked_item_draws_rows_in_later_lists_until_focus_is_forgotten() {
    let _imgui = lock();
    let draw = |ui: &mut Ui, lines_above: f32, include: Range<usize>| {
        test_window(ui, |ui| {
            let [x, y] = ui.cursor_screen_pos();
            ui.set_cursor_screen_pos([x, y - lines_above * ui.text_line_height_with_spacing()]);
            let mut drawn = Vec::new();
            ui.list_clipper(ITEMS).include(include).show(|ui, visible| {
                for i in visible {
                    ui.selectable(&format!("item {i}"), false);
                    drawn.push((i, last_item_center()));
                }
            });
            drawn
        })
        .unwrap()
    };
    let item_1 = frame(|ui| draw(ui, 0.0, 0..0)).unwrap()[1].1;
    for down in [false, true, false] {
        frame_at(item_1, down, |ui| draw(ui, 0.0, 0..0)).unwrap();
    }
    assert_ne!(unsafe { mqemu_imgui_test_nav_id() }, 0);
    // Item 1 drawn 999 lines above the window's top.
    frame(|ui| draw(ui, 1000.0, 1..2)).unwrap();

    let leaked = |drawn: Vec<(usize, [f32; 2])>| drawn.into_iter().map(|(i, _)| i).filter(|i| (3990..4010).contains(i)).collect::<Vec<_>>();
    // 999 lines above the top of a list scrolled 5000 lines is row 4001.
    assert!(!leaked(frame(|ui| draw(ui, 5000.0, 0..0)).unwrap()).is_empty());
    unsafe { mqemu_imgui_test_forget_focus() };
    assert_eq!(unsafe { mqemu_imgui_test_nav_id() }, 0);
    assert_eq!(leaked(frame(|ui| draw(ui, 5000.0, 0..0)).unwrap()), []);
    assert_eq!(take_error(), None);
}

#[test]
fn a_first_item_that_draws_nothing_is_an_error() {
    let _imgui = lock();
    let ranges = frame(|ui| {
        test_window(ui, |ui| {
            let mut ranges = Vec::new();
            ui.list_clipper(ITEMS).show(|_, visible| ranges.push(visible));
            ranges
        })
    });
    let first = 0..1;
    assert_eq!(ranges, Ok(Some(vec![first])), "it stops after the first item");
    assert_eq!(take_error().as_deref(), Some("a list clipper's first item didn't move the cursor down: draw something for every item"));
}

#[test]
fn misuse_the_safe_api_prevents_is_refused_by_the_shim() {
    let _imgui = lock();
    // ImGui would step a clipper that had ended (dereferencing its freed
    // data) and take included items after stepping, without a word.
    let errors = frame(|ui| {
        test_window(ui, |_| unsafe {
            let api = &sys::API;
            let (mut start, mut end) = (0, 0);
            let negative = (api.mqemu_imgui_list_clipper_begin)(-1, 0.0, 0).is_null();
            let negative_error = take_error();

            let clipper = (api.mqemu_imgui_list_clipper_begin)(3, 10.0, 0);
            (api.mqemu_imgui_list_clipper_include)(clipper, 2, 1);
            let backwards_error = take_error();
            let mut steps = 0;
            while (api.mqemu_imgui_list_clipper_step)(clipper, &mut start, &mut end) {
                (api.mqemu_imgui_dummy)(10.0, 10.0);
                steps += 1;
            }
            (api.mqemu_imgui_list_clipper_include)(clipper, 0, 1);
            let late_error = take_error();
            let again = (api.mqemu_imgui_list_clipper_step)(clipper, &mut start, &mut end);
            (api.mqemu_imgui_list_clipper_end)(clipper);
            (negative, negative_error, backwards_error, steps, late_error, again, take_error())
        })
    })
    .unwrap()
    .unwrap();
    let (negative, negative_error, backwards_error, steps, late_error, again, last_error) = errors;
    assert!(negative);
    assert_eq!(negative_error.as_deref(), Some("a list clipper's item count must be 0 or more, below INT_MAX"));
    assert_eq!(backwards_error.as_deref(), Some("a list clipper's included items must be a range of indices"));
    assert_eq!(steps, 1);
    assert_eq!(late_error.as_deref(), Some("items can only be included before a list clipper's first step"));
    assert!(!again, "a clipper that ended stays ended");
    assert_eq!(last_error, None);
}
