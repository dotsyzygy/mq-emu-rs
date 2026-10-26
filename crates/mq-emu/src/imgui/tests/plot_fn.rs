//! ImPlot's callback forms (items from closures, axis formatters, custom
//! scales) and its input map.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ffi::{CStr, c_void};

use super::*;

fn axis_callbacks(title: &CStr, axis: Axis) -> (bool, bool) {
    let (mut formatter, mut transform) = (false, false);
    unsafe { mqemu_implot_test_axis_callbacks(title.as_ptr(), axis as i32, &mut formatter, &mut transform) };
    (formatter, transform)
}

fn not_in_a_plot() -> bool {
    let (mut plot, mut subplots, mut style, mut colormap) = (false, false, 0, 0);
    unsafe { mqemu_implot_test_state(&mut plot, &mut subplots, &mut style, &mut colormap) };
    !plot
}

fn vertices() -> i32 {
    unsafe { mqemu_imgui_test_window_vertices() }
}

#[test]
fn items_from_closures_draw_and_ask_only_for_their_points() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let asked = RefCell::new(BTreeSet::new());
            let point = |i: usize| {
                asked.borrow_mut().insert(i);
                [i as f64, (i as f64 * 0.7).sin()]
            };
            ui.plot("from closures").size([400.0, 300.0]).show(|plot| {
                plot.setup_axis_limits(Axis::X1, 0.0..=10.0, Cond::Always);
                plot.setup_axis_limits(Axis::Y1, -1.5..=1.5, Cond::Always);
                let mut grew = Vec::new();
                macro_rules! drawn {
                    ($item:expr) => {{
                        let before = vertices();
                        $item;
                        grew.push(vertices() > before);
                    }};
                }
                drawn!(plot.line_fn("line", 10, point, LineFlags::NONE));
                drawn!(plot.scatter_fn("scatter", 10, point, ScatterFlags::NONE));
                drawn!(plot.stairs_fn("stairs", 10, point, StairsFlags::NONE));
                drawn!(plot.bars_fn("bars", 10, point, 0.5, BarsFlags::NONE));
                drawn!(plot.shaded_fn("shaded", 10, point, |i| [i as f64, 0.0], ShadedFlags::NONE));
                drawn!(plot.digital_fn("digital", 10, |i| [i as f64, (i % 2) as f64], DigitalFlags::NONE));
                grew
            })
            .map(|grew| (grew, asked.into_inner()))
        })
    });
    let (grew, asked) = result.unwrap().flatten().unwrap();
    assert_eq!(grew, [true; 6], "every item drew something");
    assert_eq!(asked, (0..10).collect(), "every point, and none past the count");
    assert_eq!(take_error(), None);
}

#[test]
fn a_panic_in_an_item_closure_resumes_after_the_item() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let calls_after = RefCell::new(0);
            ui.plot("panicking getter").show(|plot| {
                let panicked = catch_unwind(AssertUnwindSafe(|| {
                    plot.line_fn("line", 10, |i| {
                        if i == 3 {
                            panic!("in a getter");
                        }
                        if i > 3 {
                            *calls_after.borrow_mut() += 1;
                        }
                        [i as f64, 0.0]
                    }, LineFlags::NONE);
                }));
                let message = panicked.err().and_then(|p| p.downcast_ref::<&str>().map(|s| s.to_string()));
                plot.line("after", &[1.0, 2.0]);
                message
            })
            .map(|message| (message, calls_after.into_inner()))
        })
    });
    let (message, calls_after) = result.unwrap().flatten().unwrap();
    assert_eq!(message.as_deref(), Some("in a getter"));
    assert_eq!(calls_after, 0, "the closure isn't called again once it panicked");
    assert!(not_in_a_plot());
    assert_eq!(take_error(), None);
}

#[test]
fn an_axis_formatter_labels_the_axis_and_is_forgotten_when_the_plot_ends() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let asked = RefCell::new(Vec::new());
            ui.plot("formatted").show(|plot| {
                plot.setup_axis_limits(Axis::X1, 0.0..=4.0, Cond::Always);
                plot.setup_axis_format_fn(Axis::X1, |v| {
                    asked.borrow_mut().push(v);
                    format!("{v:.0} HP")
                });
                plot.line("line", &[0.0, 1.0, 2.0]);
            });
            (asked.into_inner(), axis_callbacks(c"formatted", Axis::X1))
        })
    });
    let (asked, after) = result.unwrap().unwrap();
    assert!(asked.contains(&0.0) && asked.contains(&4.0), "the ticks were labeled: {asked:?}");
    assert_eq!(after, (false, false), "ImPlot no longer holds the freed formatter");
    assert_eq!(take_error(), None);
}

/// Writes "x".
unsafe extern "C" fn static_formatter(_: f64, buffer: *mut c_char, size: i32, _: *mut c_void) -> i32 {
    if size > 1 {
        unsafe { *buffer = b'x' as c_char };
        unsafe { *buffer.add(1) = 0 };
    }
    1
}

#[test]
fn implot_keeps_a_formatter_after_its_plot_ends() {
    // The negative control for the test above: what the shim's end_plot
    // forgets, ImPlot's own EndPlot keeps (until the plot next begins).
    let _imgui = lock();
    let api = &sys_plot::API;
    let after = frame(|ui| {
        test_window(ui, |_| unsafe {
            let (t, n) = ("raw formatted".as_ptr().cast(), "raw formatted".len());
            assert!((api.mqemu_implot_begin_plot)(t, n, 0.0, 0.0, 0));
            (api.mqemu_implot_setup_axis_format_fn)(Axis::X1 as i32, static_formatter, std::ptr::null_mut());
            mqemu_implot_test_end_plot_raw();
            axis_callbacks(c"raw formatted", Axis::X1)
        })
    });
    assert_eq!(after, Ok(Some((true, false))));
    assert_eq!(take_error(), None);
}

#[test]
fn a_custom_scale_maps_values_through_its_closures() {
    let _imgui = lock();
    let e = std::f64::consts::E;
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let shown = ui.plot("log scale").size([400.0, 300.0]).show(|plot| {
                plot.setup_axis_scale_fn(Axis::X1, f64::ln, f64::exp);
                plot.setup_axis_limits(Axis::X1, 1.0..=e * e, Cond::Always);
                plot.line("line", &[1.0, 2.0, 3.0]);
                let [low, middle, high] = [1.0, e, e * e].map(|x| plot.plot_to_pixels([x, 0.0])[0]);
                let back = plot.pixels_to_plot([middle, 0.0])[0];
                (low, middle, high, back)
            });
            (shown, axis_callbacks(c"log scale", Axis::X1))
        })
    });
    let (shown, after) = result.unwrap().unwrap();
    let (low, middle, high, back) = shown.unwrap();
    assert!((middle - (low + high) / 2.0).abs() < 1.0, "e is halfway on a log scale: {low} {middle} {high}");
    assert!((back - e).abs() < 0.05, "and back: {back}");
    assert_eq!(after, (false, false), "forgotten when the plot ended");
    assert_eq!(take_error(), None);
}

#[test]
fn a_panic_in_a_formatter_or_scale_resumes_when_the_plot_ends() {
    let _imgui = lock();
    for scale in [false, true] {
        let result = frame(|ui| {
            test_window(ui, |ui| {
                catch_unwind(AssertUnwindSafe(|| {
                    ui.plot("panicking callback").show(|plot| {
                        if scale {
                            plot.setup_axis_scale_fn(Axis::Y1, |_| panic!("in a scale"), |v| v);
                        } else {
                            plot.setup_axis_format_fn(Axis::Y1, |_| -> String { panic!("in a formatter") });
                        }
                        plot.line("line", &[0.0, 1.0]);
                        "the plot's closure finished"
                    })
                }))
                .err()
                .and_then(|p| p.downcast_ref::<&str>().map(|s| s.to_string()))
            })
        });
        let expected = if scale { "in a scale" } else { "in a formatter" };
        assert_eq!(result, Ok(Some(Some(expected.to_owned()))));
        assert!(not_in_a_plot());
        assert_eq!(take_error(), None);
    }
}

#[test]
fn callbacks_are_setup_and_panic_once_something_is_plotted() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            ui.plot("late formatter").show(|plot| {
                plot.line("line", &[0.0, 1.0]);
                let format = catch_unwind(AssertUnwindSafe(|| plot.setup_axis_format_fn(Axis::X1, |v| v)));
                let scale = catch_unwind(AssertUnwindSafe(|| plot.setup_axis_scale_fn(Axis::X1, |v| v, |v| v)));
                (format.is_err(), scale.is_err())
            })
        })
    });
    assert_eq!(result, Ok(Some(Some((true, true)))));
    assert_eq!(take_error(), None);
}

#[test]
fn a_pie_chart_labels_its_slices_with_a_closure() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let mut asked = Vec::new();
            ui.plot("pie").show(|plot| {
                plot.pie_chart_fn(&["a", "b", "c"], &[1u8, 2, 3], [0.5, 0.5], 0.4, |v| {
                    asked.push(v);
                    format!("{v} kills")
                }, 90.0, PieChartFlags::NONE);
                catch_unwind(AssertUnwindSafe(|| plot.pie_chart_fn(&["a"], &[1.0f32], [0.5, 0.5], 0.4, |_| -> &str { panic!("in a label") }, 90.0, PieChartFlags::NONE)))
                    .is_err()
            })
            .map(|panicked| (asked, panicked))
        })
    });
    assert_eq!(result.unwrap().flatten(), Some((vec![1.0, 2.0, 3.0], true)));
    assert_eq!(take_error(), None);
}

#[test]
fn the_input_map_is_read_changed_for_a_scope_and_restored() {
    let _imgui = lock();
    let result = frame(|ui| {
        let (now, default, reverse) = (ui.plot_input_map(), ui.default_plot_input_map(), ui.reverse_plot_input_map());
        let inside = ui.with_plot_input_map(&reverse, |ui| {
            test_window(ui, |ui| {
                ui.plot("reversed").show(|plot| plot.line("line", &[0.0, 1.0]));
            });
            ui.plot_input_map()
        });
        (now, default, reverse, inside, ui.plot_input_map())
    });
    let (now, default, reverse, inside, after) = result.unwrap();
    assert_eq!(now, default, "nothing changed it yet");
    assert_eq!((default.pan, default.select, default.override_mod), (MouseButton::Left, MouseButton::Right, KeyMods::CTRL));
    assert_eq!((reverse.pan, reverse.select), (MouseButton::Right, MouseButton::Left));
    assert_eq!(inside, reverse);
    assert_eq!(after, default, "restored afterwards");
    assert_eq!(take_error(), None);
}

#[test]
fn a_map_imgui_cannot_use_is_refused() {
    let _imgui = lock();
    let mut ui = Ui::new(&fake::APIS);
    let mut map = ui.default_plot_input_map();
    map.select_cancel = map.select;
    assert!(catch_unwind(AssertUnwindSafe(|| ui.set_plot_input_map(&map))).is_err(), "cancelling with the selecting button");
    // ImGui indexes its mouse button arrays with these, unchecked.
    let mut raw = sys_plot::mqemu_plot_input_map::default();
    unsafe { (sys_plot::API.mqemu_implot_input_map)(&mut raw) };
    let before = raw;
    raw.pan = 9;
    unsafe { (sys_plot::API.mqemu_implot_set_input_map)(&raw) };
    assert_eq!(take_error().as_deref(), Some("not a mouse button"));
    raw.pan = before.pan;
    raw.zoom_mod = 1;
    unsafe { (sys_plot::API.mqemu_implot_set_input_map)(&raw) };
    assert_eq!(take_error().as_deref(), Some("not ImGuiMod_ modifier keys"));
    let mut after = sys_plot::mqemu_plot_input_map::default();
    unsafe { (sys_plot::API.mqemu_implot_input_map)(&mut after) };
    assert_eq!(after, before, "unchanged");
    // The extra mouse buttons are buttons ImGui knows.
    map = ui.default_plot_input_map();
    map.menu = MouseButton::Extra2;
    ui.set_plot_input_map(&map);
    assert_eq!(ui.plot_input_map().menu, MouseButton::Extra2);
    ui.set_plot_input_map(&ui.default_plot_input_map());
    assert_eq!(take_error(), None);
}

#[test]
fn the_input_map_selector_draws() {
    let _imgui = lock();
    let changed = frame(|ui| test_window(ui, |ui| ui.plot_input_map_selector("mouse")));
    assert_eq!(changed, Ok(Some(false)));
    assert_eq!(take_error(), None);
}
