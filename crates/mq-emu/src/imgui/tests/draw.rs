//! Draw lists, images, textures and fonts.

use super::*;
use crate::imgui::fake;

/// `list`'s clip rectangle depth, path length and channel count.
fn list_state(draw: &DrawList<'_>) -> (i32, i32, i32) {
    let (mut clip, mut path, mut channels) = (0, 0, 0);
    unsafe { mqemu_imgui_test_list_state(draw.raw_list(), &mut clip, &mut path, &mut channels) };
    (clip, path, channels)
}

fn vertices(draw: &DrawList<'_>) -> i32 {
    unsafe { mqemu_imgui_test_list_vertices(draw.raw_list()) }
}

#[test]
fn colors_are_packed_as_imgui_packs_them() {
    let _imgui = lock();
    let components = [-1.0, 0.0, 0.5 / 255.0, 0.001, 0.25, 0.5, 0.7, 1.0 - 0.5 / 255.0, 0.999, 1.0, 2.0, f32::INFINITY, f32::NEG_INFINITY];
    for &r in &components {
        for &a in &components {
            let color = [r, 1.0 - r, a, a];
            let theirs = unsafe { (sys::API.mqemu_imgui_color_convert)(color[0], color[1], color[2], color[3]) };
            assert_eq!(pack_color(color), theirs, "{color:?}");
        }
    }
}

#[test]
fn every_shape_draws_on_every_list() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let white = [1.0; 4];
            let lists = [ui.draw_list(), ui.foreground_draw_list(), ui.background_draw_list()];
            let mut grew = Vec::new();
            for draw in &lists {
                let shapes: [&dyn Fn(&DrawList<'_>); 21] = [
                    &|d| d.add_line([20.0, 20.0], [80.0, 80.0], white, 2.0),
                    &|d| d.add_rect([20.0, 20.0], [80.0, 80.0], white, 4.0, 1.0),
                    &|d| d.add_rect_filled([20.0, 20.0], [80.0, 80.0], white, 0.0),
                    &|d| d.add_rect_corners([20.0, 20.0], [80.0, 80.0], white, 6.0, Corners::TOP, 1.0),
                    &|d| d.add_rect_filled_corners([20.0, 20.0], [80.0, 80.0], white, 6.0, Corners::BOTTOM_LEFT),
                    &|d| d.add_rect_filled_multicolor([20.0, 20.0], [80.0, 80.0], [white, [1.0, 0.0, 0.0, 1.0], white, [0.0, 0.0, 1.0, 1.0]]),
                    &|d| d.add_triangle([[20.0, 20.0], [80.0, 20.0], [50.0, 80.0]], white, 1.0),
                    &|d| d.add_triangle_filled([[20.0, 20.0], [80.0, 20.0], [50.0, 80.0]], white),
                    &|d| d.add_quad([[20.0, 20.0], [80.0, 20.0], [80.0, 80.0], [20.0, 80.0]], white, 1.0),
                    &|d| d.add_quad_filled([[20.0, 20.0], [80.0, 20.0], [80.0, 80.0], [20.0, 80.0]], white),
                    &|d| d.add_circle([50.0, 50.0], 20.0, white, 1.0),
                    &|d| d.add_circle_filled([50.0, 50.0], 20.0, white),
                    &|d| d.add_ngon([50.0, 50.0], 20.0, white, 6, 1.0),
                    &|d| d.add_ngon_filled([50.0, 50.0], 20.0, white, 3),
                    &|d| d.add_ellipse([50.0, 50.0], [30.0, 10.0], white, 0.5, 1.0),
                    &|d| d.add_ellipse_filled([50.0, 50.0], [30.0, 10.0], white, 0.0),
                    &|d| d.add_bezier_cubic([[20.0, 20.0], [40.0, 80.0], [60.0, 0.0], [80.0, 80.0]], white, 1.0),
                    &|d| d.add_bezier_quadratic([[20.0, 20.0], [50.0, 80.0], [80.0, 20.0]], white, 1.0),
                    &|d| d.add_polyline(&[[20.0, 20.0], [50.0, 80.0], [80.0, 20.0]], white, true, 1.0),
                    &|d| d.add_convex_poly_filled(&[[20.0, 20.0], [80.0, 20.0], [50.0, 80.0]], white),
                    &|d| d.add_concave_poly_filled(&[[20.0, 20.0], [80.0, 20.0], [50.0, 50.0], [80.0, 80.0], [20.0, 80.0]], white),
                ];
                for (index, shape) in shapes.iter().enumerate() {
                    let before = vertices(draw);
                    shape(draw);
                    grew.push((index, vertices(draw) > before));
                }
                let before = vertices(draw);
                draw.add_text([20.0, 20.0], white, "text");
                draw.add_text_with(Font::Large, 0.0, [20.0, 40.0], white, "large text", 0.0);
                draw.add_text_with(Font::Console, 30.0, [20.0, 60.0], white, "wrapped ".repeat(10), 100.0);
                grew.push((99, vertices(draw) > before));
            }
            grew.into_iter().filter(|(_, grew)| !grew).map(|(index, _)| index).collect::<Vec<_>>()
        })
    });
    assert_eq!(result, Ok(Some(vec![])), "shapes that drew nothing");
    assert_eq!(take_error(), None);
}

#[test]
fn transparent_and_degenerate_shapes_draw_nothing() {
    // The negative control for every_shape_draws_on_every_list.
    let _imgui = lock();
    frame(|ui| {
        test_window(ui, |ui| {
            let draw = ui.draw_list();
            let before = vertices(&draw);
            draw.add_line([0.0, 0.0], [10.0, 10.0], [1.0, 1.0, 1.0, 0.0], 1.0);
            draw.add_polyline(&[[0.0, 0.0]], [1.0; 4], false, 1.0);
            draw.add_convex_poly_filled(&[[0.0, 0.0], [1.0, 1.0]], [1.0; 4]);
            draw.add_text([0.0, 0.0], [1.0; 4], "");
            assert_eq!(vertices(&draw), before);
        })
    })
    .unwrap();
    assert_eq!(take_error(), None);
}

#[test]
fn a_regular_polygon_needs_three_sides() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let draw = ui.draw_list();
            let message = |p: Box<dyn std::any::Any + Send>| p.downcast_ref::<String>().cloned().unwrap_or_default();
            let two = catch_unwind(AssertUnwindSafe(|| draw.add_ngon([0.0, 0.0], 1.0, [1.0; 4], 2, 1.0))).map_err(message);
            let filled = catch_unwind(AssertUnwindSafe(|| draw.add_ngon_filled([0.0, 0.0], 1.0, [1.0; 4], 0))).is_err();
            (two, filled)
        })
    });
    let (two, filled) = result.unwrap().unwrap();
    assert!(two.is_err_and(|m| m.contains("at least 3 sides, not 2")));
    assert!(filled);
    assert_eq!(take_error(), None);
}

#[test]
fn a_two_sided_polygon_is_an_imgui_error() {
    // What a_regular_polygon_needs_three_sides keeps from ImGui.
    let _imgui = lock();
    frame(|ui| {
        test_window(ui, |ui| unsafe { (sys::API.mqemu_imgui_draw_circle)(ui.draw_list().raw_list(), 0.0, 0.0, 1.0, !0, 2, true, 0.0) })
    })
    .unwrap();
    assert_eq!(take_error().as_deref(), Some("a regular polygon needs at least 3 sides"));
}

#[test]
fn paths_are_drawn_once_finished_and_dropped_otherwise() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let draw = ui.draw_list();
            let mut path = draw.path();
            path.line_to([20.0, 20.0])
                .arc_to([50.0, 50.0], 20.0, 0.0, std::f32::consts::PI)
                .elliptical_arc_to([50.0, 50.0], [30.0, 10.0], 0.3, 0.0, 1.0)
                .bezier_cubic_to([[60.0, 0.0], [70.0, 90.0], [80.0, 20.0]])
                .bezier_quadratic_to([[90.0, 50.0], [100.0, 20.0]])
                .rect([10.0, 10.0], [30.0, 30.0], 3.0, Corners::NONE);
            let points = list_state(&draw).1;
            let before = vertices(&draw);
            path.stroke([1.0; 4], true, 2.0);
            let stroked = (list_state(&draw).1, vertices(&draw) > before);

            let mut path = draw.path();
            path.line_to([20.0, 20.0]).line_to([80.0, 20.0]).line_to([50.0, 80.0]);
            path.fill_convex([1.0; 4]);
            let mut path = draw.path();
            path.line_to([20.0, 20.0]).line_to([80.0, 20.0]).line_to([50.0, 50.0]).line_to([80.0, 80.0]);
            path.fill_concave([1.0; 4]);

            let before = vertices(&draw);
            let mut abandoned = draw.path();
            abandoned.line_to([1.0, 1.0]).line_to([2.0, 2.0]);
            drop(abandoned);
            (points > 10, stroked, list_state(&draw).1, vertices(&draw) == before)
        })
    });
    assert_eq!(result, Ok(Some((true, (0, true), 0, true))));
    assert_eq!(take_error(), None);
}

#[test]
fn clip_rects_are_popped_even_when_the_closure_panics() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let draw = ui.draw_list();
            let outside = list_state(&draw).0;
            let inside = draw.with_clip_rect([0.0, 0.0], [50.0, 50.0], true, |draw| {
                draw.with_clip_rect([10.0, 10.0], [20.0, 20.0], false, |draw| list_state(draw).0)
            });
            let panicked = catch_unwind(AssertUnwindSafe(|| {
                draw.with_clip_rect([0.0, 0.0], [50.0, 50.0], true, |_| panic!("deliberate"));
            }));
            (outside, inside, panicked.is_err(), list_state(&draw).0)
        })
    });
    let (outside, inside, panicked, after) = result.unwrap().unwrap();
    assert_eq!((inside, panicked, after), (outside + 2, true, outside));
    assert_eq!(take_error(), None);
}

#[test]
fn popping_imguis_own_clip_rect_is_caught() {
    // What with_clip_rect's balancing prevents; the shim refuses it rather
    // than leave the list without one.
    let _imgui = lock();
    frame(|ui| {
        test_window(ui, |ui| {
            let draw = ui.draw_list();
            for _ in 0..list_state(&draw).0 {
                unsafe { (sys::API.mqemu_imgui_draw_pop_clip_rect)(draw.raw_list()) };
            }
        })
    })
    .unwrap();
    assert_eq!(take_error().as_deref(), Some("Calling PopClipRect() too many times!"));
}

#[test]
fn channels_merge_in_order_and_refuse_nesting() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let draw = ui.draw_list();
            let inside = draw.channels(2, |channels| {
                let split = list_state(channels).2;
                let switched = (channels.set_current(1), channels.set_current(2), channels.set_current(0));
                channels.add_rect_filled([0.0, 0.0], [10.0, 10.0], [1.0; 4], 0.0);
                let nested = channels.channels(2, |_| ()).is_some();
                (split, switched, nested)
            });
            let too_few = draw.channels(1, |_| ()).is_some();
            let panicked = catch_unwind(AssertUnwindSafe(|| draw.channels(3, |_| panic!("deliberate")))).is_err();
            (inside, too_few, panicked, list_state(&draw).2)
        })
    });
    let (inside, too_few, panicked, after) = result.unwrap().unwrap();
    assert_eq!(inside, Some((2, (true, false, true), false)));
    assert!(!too_few && panicked);
    assert!(after <= 1, "merged after the panic: {after} channels");
    assert_eq!(take_error(), None);
}

#[test]
fn images_and_image_buttons_draw_their_texture() {
    let _imgui = lock();
    let texture = Ui::new(&fake::APIS).load_texture("icons.png").expect("the fake loads it");
    let id = texture.id().expect("the fake has an ID");
    assert_eq!(texture.size(), [64.0, 32.0]);
    let result = frame(|ui| {
        test_window(ui, |ui| {
            ui.image(id, [32.0, 32.0]);
            ui.image_with(id, [32.0, 16.0], ImageOptions { uv: [[0.0, 0.0], [0.5, 0.5]], background: [0.0, 0.0, 0.0, 1.0], tint: [1.0, 0.0, 0.0, 1.0] });
            let clicked = ui.image_button("button", id, [32.0, 32.0]);
            let draw = ui.draw_list();
            draw.add_image(id, [0.0, 0.0], [10.0, 10.0]);
            draw.add_image_with(id, [0.0, 0.0], [10.0, 10.0], [[0.0, 0.0], [1.0, 1.0]], [1.0; 4], 3.0, Corners::ALL);
            draw.add_image_quad(id, [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]], [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], [1.0; 4]);
            let fg = ui.foreground_draw_list();
            let before = unsafe { mqemu_imgui_test_list_uses_texture(fg.raw_list(), id.as_raw()) };
            fg.add_image(id, [0.0, 0.0], [10.0, 10.0]);
            let after = unsafe { mqemu_imgui_test_list_uses_texture(fg.raw_list(), id.as_raw()) };
            (clicked, unsafe { mqemu_imgui_test_list_uses_texture(draw.raw_list(), id.as_raw()) }, before, after)
        })
    });
    assert_eq!(result, Ok(Some((false, true, false, true))));
    assert_eq!(take_error(), None);
    drop(texture);
    free_retired_textures();
}

#[test]
fn a_texture_is_freed_after_the_frame_that_drew_it() {
    let _imgui = lock();
    free_retired_textures();
    let destroyed = || fake::log().textures_destroyed.iter().filter(|p| *p == "retired.png").count();
    let ui = Ui::new(&fake::APIS);
    assert!(ui.load_texture(fake::MISSING).is_none());
    let texture = ui.load_texture("retired.png").unwrap();
    drop(texture);
    assert_eq!(destroyed(), 0, "the frame that drew it may not have been rendered yet");
    free_retired_textures();
    assert_eq!(destroyed(), 1);
    free_retired_textures();
    assert_eq!(destroyed(), 1, "freed once");
}

#[test]
fn texture_animations_stop_drawing_when_the_ui_unloads() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    assert!(ui.find_texture_animation("A_NoSuchThing").is_none());
    let mut icons = ui.find_texture_animation(ITEM_ICONS).unwrap();
    icons.set_item_icon(1234);
    assert_eq!(icons.cell(), 734, "item icon numbers start at 500");
    assert_eq!(icons.size(), ([256.0, 256.0], Some([40.0, 40.0])));
    let drawn = || fake::log().animations_drawn.len();
    let before = drawn();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let a = ui.texture_animation(&icons, Some([40.0, 40.0]), true);
            let b = ui.draw_list().add_texture_animation(&icons, [0.0, 0.0], None, [1.0; 4], None);
            (a, b)
        })
    });
    assert_eq!(result, Ok(Some((true, true))));
    let log = fake::log();
    assert_eq!(&log.animations_drawn[before..], [
        (734, 40, 40, 0xffff_ffff, pack_color([1.0, 1.0, 1.0, 0.5]), false),
        (734, 0, 0, 0xffff_ffff, 0, true),
    ]);

    crate::imgui::ui_unloaded();
    assert!(!icons.is_current());
    let after = drawn();
    let result = frame(|ui| test_window(ui, |ui| (ui.texture_animation(&icons, None, false), ui.draw_list().add_texture_animation(&icons, [0.0; 2], None, [1.0; 4], None))));
    assert_eq!(result, Ok(Some((false, false))));
    assert_eq!(drawn(), after, "nothing reached the game once its UI was unloaded");
    icons.set_cell(5);
    assert_eq!(icons.cell(), 734, "nor did a new cell");

    let freed = fake::log().animations_freed;
    drop(icons);
    assert_eq!(fake::log().animations_freed, freed + 1);
}

#[test]
fn fonts_are_pushed_and_popped_around_their_closure() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let depth = || unsafe { mqemu_imgui_test_font_depth() };
            let outside = (depth(), ui.font_size());
            let large = ui.with_font(Font::Large, |ui| (depth(), ui.font_size()));
            let console = ui.with_font(Font::Console, |ui| ui.font_size());
            let sized = ui.with_font_sized(Font::Default, 30.0, |ui| ui.font_size());
            let bigger = ui.with_font_size(ui.font_size_base() * 2.0, |ui| ui.font_size());
            // No EQ fonts outside the game: the current font stays.
            let eq = ui.with_font(Font::Eq(3), |ui| (depth(), ui.font_size()));
            let panicked = catch_unwind(AssertUnwindSafe(|| ui.with_font(Font::Large, |_| panic!("deliberate")))).is_err();
            (outside, large, console, sized, bigger, eq, panicked, depth())
        })
    });
    let (outside, large, console, sized, bigger, eq, panicked, after) = result.unwrap().unwrap();
    assert_eq!(outside.1, 16.0, "MacroQuest's default font is 16 px");
    assert_eq!(large, (outside.0 + 1, 22.0));
    assert_eq!(console, 13.0);
    assert_eq!(sized, 30.0);
    assert_eq!(bigger, 32.0);
    assert_eq!(eq, (outside.0 + 1, 16.0));
    assert!(panicked);
    assert_eq!(after, outside.0);
    assert_eq!(take_error(), None);
}

#[test]
fn which_fonts_are_available() {
    let _imgui = lock();
    let ui = Ui::new(&fake::APIS);
    assert!(ui.has_font(Font::Default) && ui.has_font(Font::Console) && ui.has_font(Font::Large));
    assert!(!ui.has_font(Font::Eq(0)), "EQ's fonts only exist in game");
}

#[test]
#[should_panic(expected = "a font size is 0")]
fn a_negative_font_size_panics() {
    let mut ui = Ui::new(&fake::APIS);
    ui.with_font_size(-1.0, |_| ());
}

#[test]
fn a_negative_font_size_is_an_imgui_error() {
    // What a_negative_font_size_panics keeps from ImGui.
    let _imgui = lock();
    frame(|_| unsafe { (sys::API.mqemu_imgui_push_font)(std::ptr::null_mut(), -1.0) }).unwrap();
    assert_eq!(take_error().as_deref(), Some("PushFont() needs a size of 0 or more"));
}

#[test]
fn font_files_load_once_and_draw_at_their_size() {
    let _imgui = lock();
    let windows = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_owned());
    let path = format!(r"{windows}\Fonts\arial.ttf");
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let font = ui.load_font(&path, 18.0).expect("Windows has Arial");
            let again = ui.load_font(&path, 18.0);
            let other_size = ui.load_font(&path, 24.0);
            let missing = ui.load_font(r"C:\no\such\font.ttf", 18.0);
            let size = ui.with_font(Font::Loaded(font), |ui| {
                ui.text("in Arial");
                ui.font_size()
            });
            ui.draw_list().add_text_with(Font::Loaded(font), 0.0, [0.0, 0.0], [1.0; 4], "Arial", 0.0);
            (again == Some(font), other_size.is_some_and(|f| f != font), missing, size)
        })
    });
    crate::imgui::remove_loaded_fonts();
    assert_eq!(result, Ok(Some((true, true, None, 18.0))));
    assert_eq!(take_error(), None, "a missing file isn't an ImGui error");
}
