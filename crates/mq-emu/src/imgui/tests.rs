//! The safe API against MacroQuest's real imgui.dll, in a headless context:
//! scopes always end (even when the closure panics), contents only draw when
//! ImGui says so, and inputs reach Rust values.

use std::ffi::{CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Mutex, MutexGuard};

use mq_emu_sys::imgui::testing::*;
use mq_emu_sys::implot as sys_plot;

use super::*;

/// ImGui's context is process-global; tests take turns.
static IMGUI: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    let guard = IMGUI.lock().unwrap_or_else(|e| e.into_inner());
    unsafe { mqemu_imgui_test_init() };
    // An earlier test's navigation item would draw in this one's clipped lists.
    unsafe { mqemu_imgui_test_forget_focus() };
    take_error();
    guard
}


fn take_error() -> Option<String> {
    let mut buffer = [0 as c_char; 512];
    unsafe { (sys::API.mqemu_imgui_take_error)(buffer.as_mut_ptr(), buffer.len()) }.then(|| crate::text::from_c_array(&buffer))
}

/// Draws one frame with the mouse at `mouse`; Err with what ImGui::Render threw.
fn frame_at<R>(mouse: [f32; 2], down: bool, draw: impl FnOnce(&mut Ui) -> R) -> Result<R, String> {
    assert!(unsafe { mqemu_imgui_test_new_frame(mouse[0], mouse[1], down) }, "NewFrame threw");
    let result = draw(&mut Ui::new(&fake::APIS));
    let mut error = [0 as c_char; 512];
    if unsafe { mqemu_imgui_test_end_frame(error.as_mut_ptr(), error.len()) } {
        Ok(result)
    } else {
        Err(crate::text::from_c_array(&error))
    }
}

fn frame<R>(draw: impl FnOnce(&mut Ui) -> R) -> Result<R, String> {
    frame_at([0.0, 0.0], false, draw)
}

/// A window at a fixed place, so clicks can find its widgets.
fn test_window<R>(ui: &mut Ui, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    ui.window("test").position([10.0, 10.0], Cond::Always).size([500.0, 600.0], Cond::Always).flags(WindowFlags::NO_SAVED_SETTINGS).show(f)
}

fn window_depth() -> i32 {
    unsafe { mqemu_imgui_test_window_depth() }
}

fn last_item_center() -> [f32; 2] {
    let (mut x0, mut y0, mut x1, mut y1) = (0.0, 0.0, 0.0, 0.0);
    unsafe { mqemu_imgui_test_last_item_rect(&mut x0, &mut y0, &mut x1, &mut y1) };
    [(x0 + x1) / 2.0, (y0 + y1) / 2.0]
}

#[test]
fn every_scope_ends_when_its_closure_panics() {
    let _imgui = lock();
    // One scope of each kind, nested, with a panic at the bottom. Unwinding
    // must end every one, in order, or ImGui::Render throws (as
    // mq-emu-sys's a_missing_end_makes_the_frame_fail shows it would).
    let result = frame(|ui| {
        let outside = window_depth();
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            test_window(ui, |ui| {
                ui.set_next_item_open(true, Cond::Always);
                ui.tree_node("tree", |ui| {
                    ui.with_id(7, |ui| {
                        ui.with_style_color(StyleColor::Text, [1.0, 0.0, 0.0, 1.0], |ui| {
                            ui.with_style(StyleVar::ItemSpacing([1.0, 1.0]), |ui| {
                                ui.disabled(true, |ui| {
                                    ui.indented(|ui| {
                                        ui.group(|ui| {
                                            ui.tab_bar("tabs", |tabs| {
                                                tabs.tab("tab", |ui| {
                                                    ui.child("child").size([0.0, 200.0]).show(|ui| {
                                                        ui.table("table", 2).show(|table| {
                                                            table.next_row();
                                                            table.next_column();
                                                            table.text("cell");
                                                            panic!("deliberate");
                                                        })
                                                    })
                                                })
                                            })
                                        })
                                    })
                                })
                            })
                        })
                    })
                })
            })
        }));
        assert!(panicked.is_err(), "the closure ran all the way down");
        assert_eq!(window_depth(), outside, "every window and child was ended");
    });
    assert_eq!(result, Ok(()));
    assert_eq!(take_error(), None);
}

#[test]
fn text_wrapped_and_colored_text_end_their_scopes() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            ui.text_wrapped("a long line ".repeat(40));
            ui.text_colored([0.0, 1.0, 0.0, 1.0], "green");
            ui.text_disabled("grey");
            ui.bullet_text("bullet");
            ui.label_text("label", "value");
            ui.separator_text("section");
        });
    });
    assert_eq!(result, Ok(()));
    assert_eq!(take_error(), None);
}

#[test]
fn a_closed_window_is_not_drawn() {
    let _imgui = lock();
    let mut open = false;
    let ran = frame(|ui| {
        let before = window_depth();
        let ran = ui.window("closed").open(&mut open).show(|_| ()).is_some();
        assert_eq!(window_depth(), before);
        ran
    });
    assert_eq!(ran, Ok(false));
}

#[test]
fn a_collapsed_window_skips_its_contents_but_still_ends() {
    let _imgui = lock();
    let draw = |ui: &mut Ui| ui.window("collapsed").collapsed(true, Cond::Always).show(|_| ()).is_some();
    // A new window's first frame measures its contents, collapsed or not.
    frame(draw).unwrap();
    assert_eq!(frame(draw), Ok(false));
    assert_eq!(take_error(), None);
}

#[test]
fn a_button_click_reaches_rust() {
    let _imgui = lock();
    let draw = |ui: &mut Ui| test_window(ui, |ui| (ui.button("Press"), last_item_center())).unwrap();
    let (_, at) = frame(draw).unwrap();
    let mut clicks = Vec::new();
    for down in [false, true, false] {
        clicks.push(frame_at(at, down, draw).unwrap().0);
    }
    assert_eq!(clicks, [false, false, true]);
}

#[test]
fn typing_edits_a_string_and_grows_its_buffer() {
    let _imgui = lock();
    let mut text = String::new();
    let draw = |ui: &mut Ui, text: &mut String| test_window(ui, |ui| (ui.input_text("Name", text), last_item_center())).unwrap();
    let (_, at) = frame(|ui| draw(ui, &mut text)).unwrap();
    // Click into the box, then type: longer than any buffer we started with,
    // and multi-byte, so ImGui must grow the buffer and write UTF-8.
    frame_at(at, true, |ui| draw(ui, &mut text)).unwrap();
    frame_at(at, false, |ui| draw(ui, &mut text)).unwrap();
    let typed = "Grüße, 世界! ".repeat(20);
    let typed_c = CString::new(typed.clone()).unwrap();
    unsafe { mqemu_imgui_test_type(typed_c.as_ptr()) };
    let (changed, _) = frame_at(at, false, |ui| draw(ui, &mut text)).unwrap();
    assert!(changed, "typing changed the text");
    assert_eq!(text, typed);
    // Untouched frames leave it alone.
    let (changed, _) = frame_at(at, false, |ui| draw(ui, &mut text)).unwrap();
    assert!(!changed);
    assert_eq!(text, typed);
}

#[test]
fn a_checkbox_and_radio_values_change_on_click() {
    let _imgui = lock();
    let mut checked = false;
    let draw = |ui: &mut Ui, checked: &mut bool| test_window(ui, |ui| (ui.checkbox("Check", checked), last_item_center())).unwrap();
    let (_, at) = frame(|ui| draw(ui, &mut checked)).unwrap();
    // Hover, press, release.
    for down in [false, true, false] {
        frame_at(at, down, |ui| draw(ui, &mut checked)).unwrap();
    }
    assert!(checked);

    let mut choice = 1;
    let draw = |ui: &mut Ui, choice: &mut i32| {
        test_window(ui, |ui| {
            ui.radio_value("one", choice, 1);
            ui.radio_value("two", choice, 2);
            last_item_center()
        })
        .unwrap()
    };
    let at = frame(|ui| draw(ui, &mut choice)).unwrap();
    // Hover, press, release.
    for down in [false, true, false] {
        frame_at(at, down, |ui| draw(ui, &mut choice)).unwrap();
    }
    assert_eq!(choice, 2);
}

#[test]
fn every_style_var_has_the_right_kind() {
    let _imgui = lock();
    let vars = [
        StyleVar::Alpha(0.9),
        StyleVar::DisabledAlpha(0.5),
        StyleVar::WindowPadding([1.0, 1.0]),
        StyleVar::WindowRounding(1.0),
        StyleVar::WindowBorderSize(1.0),
        StyleVar::WindowMinSize([1.0, 1.0]),
        StyleVar::WindowTitleAlign([0.5, 0.5]),
        StyleVar::ChildRounding(1.0),
        StyleVar::ChildBorderSize(1.0),
        StyleVar::PopupRounding(1.0),
        StyleVar::PopupBorderSize(1.0),
        StyleVar::FramePadding([1.0, 1.0]),
        StyleVar::FrameRounding(1.0),
        StyleVar::FrameBorderSize(1.0),
        StyleVar::ItemSpacing([1.0, 1.0]),
        StyleVar::ItemInnerSpacing([1.0, 1.0]),
        StyleVar::IndentSpacing(1.0),
        StyleVar::CellPadding([1.0, 1.0]),
        StyleVar::ScrollbarSize(1.0),
        StyleVar::ScrollbarRounding(1.0),
        StyleVar::ScrollbarPadding(1.0),
        StyleVar::GrabMinSize(1.0),
        StyleVar::GrabRounding(1.0),
        StyleVar::ImageBorderSize(1.0),
        StyleVar::TabRounding(1.0),
        StyleVar::TabBorderSize(1.0),
        StyleVar::TabBarBorderSize(1.0),
        StyleVar::TabBarOverlineSize(1.0),
        StyleVar::TreeLinesSize(1.0),
        StyleVar::TreeLinesRounding(1.0),
        StyleVar::ButtonTextAlign([0.5, 0.5]),
        StyleVar::SelectableTextAlign([0.5, 0.5]),
        StyleVar::SeparatorTextBorderSize(1.0),
        StyleVar::SeparatorTextAlign([0.5, 0.5]),
        StyleVar::SeparatorTextPadding([1.0, 1.0]),
    ];
    for var in vars {
        frame(|ui| test_window(ui, |ui| ui.with_style(var, |ui| ui.text("styled")))).unwrap();
        // ImGui rejects a value of the wrong kind with an error.
        assert_eq!(take_error(), None, "{var:?}");
    }
}

#[test]
fn a_style_var_of_the_wrong_kind_is_an_imgui_error() {
    // The negative control for every_style_var_has_the_right_kind.
    let _imgui = lock();
    frame(|_| unsafe {
        (sys::API.mqemu_imgui_push_style_var_float)(sys::ImGuiStyleVar_WindowPadding, 1.0);
    })
    .unwrap();
    assert_eq!(take_error().as_deref(), Some("Calling PushStyleVar() variant with wrong type!"));
}

#[test]
fn table_columns_must_be_set_up_first_and_within_the_count() {
    let _imgui = lock();
    let outcome = frame(|ui| {
        test_window(ui, |ui| {
            let too_many = catch_unwind(AssertUnwindSafe(|| {
                ui.table("too many", 1).show(|table| {
                    table.setup_column("a");
                    table.setup_column("b");
                })
            }));
            let too_late = catch_unwind(AssertUnwindSafe(|| {
                ui.table("too late", 2).show(|table| {
                    table.next_row();
                    table.setup_column("a");
                })
            }));
            let fine = ui.table("fine", 2).flags(TableFlags::BORDERS | TableFlags::ROW_BG).show(|table| {
                table.setup_column("a");
                table.setup_column_with("b", TableColumnFlags::WIDTH_FIXED, 50.0);
                table.setup_scroll_freeze(1, 1);
                table.headers_row();
                table.row(&["1", "2"]);
                table.next_row();
                (table.set_column(1), table.set_column(2))
            });
            (too_many.is_err(), too_late.is_err(), fine)
        })
    });
    assert_eq!(outcome, Ok(Some((true, true, Some((true, false))))));
    assert_eq!(take_error(), None, "the checks run before ImGui sees the mistake");
}

#[test]
#[should_panic(expected = "1 to 512 columns, not 0")]
fn a_table_needs_a_column() {
    let mut ui = Ui::new(&fake::APIS);
    let _ = ui.table("empty", 0);
}

#[test]
fn printf_formats_are_checked() {
    let float = |f| <f32 as Number>::format_ok(f);
    let int = |f| <i32 as Number>::format_ok(f);
    for ok in ["%.3f", "%.1f ms", "%5.2e", "%g%%", "no value", "%-+8.0f", "100%%"] {
        assert!(float(ok), "{ok}");
    }
    for bad in ["%s", "%d", "%f %f", "100%", "%.2", "%n", "%*f", "%lf"] {
        assert!(!float(bad), "{bad}");
    }
    for ok in ["%d", "%3d units", "%x", "%%d"] {
        assert!(int(ok), "{ok}");
    }
    for bad in ["%f", "%s", "%d %d", "%ld"] {
        assert!(!int(bad), "{bad}");
    }
}

#[test]
fn a_bad_format_panics_before_reaching_imgui() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let mut value = 1.0f32;
            let bad = catch_unwind(AssertUnwindSafe(|| ui.slider_format("bad", &mut value, 0.0..=2.0, "%s")));
            let good = ui.slider_format("good", &mut value, 0.0..=2.0, "%.1f ms");
            (bad.is_err(), good)
        })
    });
    assert_eq!(result, Ok(Some((true, false))));
}

#[test]
fn every_widget_draws_cleanly() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let (mut f, mut i, mut text, mut color, mut choice) = (0.5f32, 3i32, "hi".to_owned(), [1.0, 0.5, 0.0, 1.0], 1usize);
            ui.text("100% ${Me.Name} %s");
            ui.small_button("small");
            ui.same_line();
            ui.button_sized("sized", [80.0, 0.0]);
            ui.selectable("pick", true);
            ui.slider("f", &mut f, 0.0..=1.0);
            ui.slider("i", &mut i, 0..=10);
            ui.drag("drag f", &mut f, 0.1);
            ui.drag_range("drag i", &mut i, 1.0, 0..=10, Some("%d units"));
            ui.input_number("in f", &mut f);
            ui.input_number("in i", &mut i);
            ui.input_text_with("ro", &mut text, InputTextFlags::READ_ONLY);
            ui.input_text_multiline("multi", &mut text, [0.0, 40.0]);
            ui.color_edit("color", &mut color);
            ui.progress_bar(0.25);
            ui.progress_bar_with(0.5, [100.0, 0.0], Some("half"));
            ui.plot_lines("lines", &[1.0, 3.0, 2.0]);
            ui.plot_histogram_with("bars", &[1.0, 3.0, 2.0], PlotOptions { scale: Some(0.0..=5.0), size: [0.0, 40.0], overlay: Some("bars") });
            ui.plot_lines("empty", &[]);
            ui.combo_list("combo", &mut choice, &["a", "b", "c"]);
            ui.collapsing_header_with("header", TreeNodeFlags::DEFAULT_OPEN);
            ui.text_link("link");
            ui.item_tooltip_text("tip");
            ui.item_tooltip(|ui| ui.text("tip"));
            ui.popup("never opened", |ui| ui.text("x"));
            ui.menu_bar(|ui| ui.text("no menu bar flag"));
            ui.dummy([10.0, 10.0]);
            ui.spacing();
            ui.new_line();
            ui.separator();
            ui.align_text_to_frame_padding();
            ui.set_next_item_width(-f32::MIN_POSITIVE);
            ui.indented_by(30.0, |ui| ui.text("indented"));
            assert!(ui.calc_text_size("abc")[0] > 0.0);
            assert!(ui.content_region_avail()[0] > 0.0);
            assert!(ui.frame_count() > 0);
            let _ = (ui.is_item_hovered(), ui.is_item_clicked(), ui.is_item_active(), ui.is_window_appearing());
            let _ = (ui.time(), ui.framerate(), ui.delta_time());
        })
    });
    assert_eq!(result, Ok(Some(())));
    assert_eq!(take_error(), None);
}

#[test]
fn popups_and_menus_open_and_close() {
    let _imgui = lock();
    let mut opened = false;
    let draw = |ui: &mut Ui, open_now: bool, close_now: bool| {
        test_window(ui, |ui| {
            if open_now {
                ui.open_popup("menu");
            }
            ui.popup("menu", |ui| {
                ui.menu_item_with("item", Some("Ctrl+X"), true, true);
                if close_now {
                    ui.close_current_popup();
                }
            })
            .is_some()
        })
        .unwrap()
    };
    frame(|ui| draw(ui, true, false)).unwrap();
    for _ in 0..2 {
        opened = frame(|ui| draw(ui, false, false)).unwrap();
    }
    assert!(opened, "the popup stays open");
    frame(|ui| draw(ui, false, true)).unwrap();
    assert!(!frame(|ui| draw(ui, false, false)).unwrap(), "and closes");
    // Outside a popup, closing does nothing.
    frame(|ui| ui.close_current_popup()).unwrap();
    assert_eq!(take_error(), None);
}


#[test]
fn text_can_be_centered_over_an_item() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            ui.progress_bar(1.0);
            let (min, max) = ui.item_rect();
            let center = last_item_center();
            assert_eq!([(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0], center, "item_rect is the last item's");
            let size = ui.calc_text_size("176 / 238");
            let pos = [(min[0] + max[0] - size[0]) / 2.0, (min[1] + max[1] - size[1]) / 2.0];
            let draw = ui.draw_list();
            let before = unsafe { mqemu_imgui_test_window_vertices() };
            draw.add_text(pos, ui.style_color(StyleColor::Text), "176 / 238");
            let after_text = unsafe { mqemu_imgui_test_window_vertices() };
            draw.add_line(min, max, [1.0; 4], 1.0);
            draw.add_rect(min, max, [1.0; 4], 2.0, 1.0);
            draw.add_rect_filled(min, max, [1.0, 1.0, 1.0, 0.2], 0.0);
            draw.add_circle(center, 5.0, [1.0; 4], 1.0);
            draw.add_circle_filled(center, 3.0, [1.0; 4]);
            let after_shapes = unsafe { mqemu_imgui_test_window_vertices() };
            (after_text > before, after_shapes > after_text)
        })
    });
    assert_eq!(result, Ok(Some((true, true))));
    assert_eq!(take_error(), None);
}

#[test]
fn style_color_reads_what_is_pushed() {
    let _imgui = lock();
    let colors = frame(|ui| {
        let pushed = [0.25, 0.5, 0.75, 1.0];
        let inside = ui.with_style_color(StyleColor::Text, pushed, |ui| ui.style_color(StyleColor::Text));
        (pushed, inside, ui.style_color(StyleColor::Text))
    });
    let (pushed, inside, after) = colors.unwrap();
    assert_eq!(inside, pushed);
    assert_ne!(after, pushed, "popped again");
}

mod addons;
mod clip;
mod custom;
mod dock;
mod draw;
mod eq;
mod interaction;
mod plot;
mod plot_fn;
mod renderer;
mod select;
mod windows;
