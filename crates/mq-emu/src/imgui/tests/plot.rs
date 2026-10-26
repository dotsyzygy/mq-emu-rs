//! ImPlot through the safe API.

use super::*;
use crate::imgui::fake;

/// ImPlot's state: in a plot, in subplots, style and colormap stack depths.
fn implot_state() -> (bool, bool, i32, i32) {
    let (mut plot, mut subplots, mut style, mut colormap) = (false, false, 0, 0);
    unsafe { mqemu_implot_test_state(&mut plot, &mut subplots, &mut style, &mut colormap) };
    (plot, subplots, style, colormap)
}

#[test]
fn every_plot_item_draws_cleanly() {
    let _imgui = lock();
    let texture = Ui::new(&fake::APIS).load_texture("plot.png").unwrap();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let xs: Vec<f64> = (0..20).map(f64::from).collect();
            let ys: Vec<f64> = xs.iter().map(|x| (x * 0.3).sin() * 10.0).collect();
            let ys2: Vec<f64> = ys.iter().map(|y| y + 2.0).collect();
            let floats: Vec<f32> = ys.iter().map(|&y| y as f32).collect();
            let bytes: [u8; 4] = [1, 5, 2, 8];
            let ints = [3i32, -1, 4, 1, -5];
            let mut point = [1.0, 1.0];
            let (mut line_x, mut line_y, mut rect) = (2.0, 3.0, [0.0, 0.0, 4.0, 4.0]);
            let drawn = ui.plot("Everything").size([480.0, 300.0]).flags(PlotFlags::CROSSHAIRS).axis_limits(Axis::X1, 0.0..=20.0, Cond::Once).show(|plot| {
                plot.setup_axes(Some("x"), Some("y"));
                plot.setup_axis(Axis::Y2, Some("right"), AxisFlags::AUX_DEFAULT);
                plot.setup_axis(Axis::X2, None, AxisFlags::NO_DECORATIONS);
                plot.setup_axis_limits(Axis::Y1, -12.0..=12.0, Cond::Always);
                plot.setup_axis_format(Axis::Y1, "%.0f HP");
                plot.setup_axis_ticks(Axis::X1, &[0.0, 10.0, 20.0], Some(&["start", "middle", "end"]), false);
                plot.setup_axis_ticks_range::<&str>(Axis::Y2, 0.0..=1.0, 3, None, true);
                plot.setup_axis_scale(Axis::X2, Scale::Linear);
                plot.setup_axis_limits_constraints(Axis::X1, -100.0..=100.0);
                plot.setup_axis_zoom_constraints(Axis::X1, 1.0..=1000.0);
                plot.setup_legend(Location::NorthEast, LegendFlags::OUTSIDE);
                plot.setup_mouse_text(Location::SouthWest, MouseTextFlags::NONE);
                plot.setup_finish();

                plot.next_line_style(Some([1.0, 0.0, 0.0, 1.0]), Some(2.0));
                plot.line("line", (&xs, &ys));
                plot.line_with("values", &floats, LineFlags::SHADED | LineFlags::NO_LEGEND);
                plot.line("scaled", Series::values(&bytes[..]).x_scale(0.5).x_start(3.0));
                plot.next_marker_style(Marker::Diamond, Some(5.0), None, None, Some([1.0; 4]));
                plot.scatter("scatter", (&xs, &ys));
                plot.scatter_with("scatter ints", &ints, ScatterFlags::NO_CLIP);
                plot.stairs("stairs", &ints);
                plot.stairs_with("stairs xy", (&xs, &ys2), StairsFlags::PRE_STEP);
                plot.next_fill_style(Some([0.0, 1.0, 0.0, 1.0]), Some(0.3));
                plot.shaded("shaded", (&xs, &ys), 0.0);
                plot.shaded_between("between", &xs, &ys, &ys2, ShadedFlags::NONE);
                plot.bars("bars", &bytes, 0.67);
                plot.bars_with("hbars", Series::values(&ints[..]).x_start(10.0), 0.5, BarsFlags::HORIZONTAL);
                plot.bar_groups(&["a", "b"], &[1u16, 2, 3, 4, 5, 6], 0.67, 0.0, BarGroupsFlags::STACKED);
                plot.next_error_bar_style(None, Some(4.0), None);
                plot.error_bars("errors", &xs[..3], &ys[..3], &[0.5, 0.5, 0.5], None, ErrorBarsFlags::NONE);
                plot.error_bars("asymmetric", &xs[..3], &ys[..3], &[0.5, 0.5, 0.5], Some(&[1.0, 1.0, 1.0]), ErrorBarsFlags::HORIZONTAL);
                plot.stems("stems", &floats, 0.0);
                plot.stems_with("hstems", (&xs, &ys), 1.0, StemsFlags::HORIZONTAL);
                plot.inf_lines("marks", &[5i64, 15], InfLinesFlags::NONE);
                plot.pie_chart(&["one", "two", "three"], &[1.0f32, 2.0, 3.0], [15.0, 5.0], 2.0, Some("%.1f%%"), 90.0, PieChartFlags::NORMALIZE);
                plot.pie_chart(&["x"], &[1u64], [5.0, 5.0], 1.0, None, 0.0, PieChartFlags::NONE);
                plot.heatmap("heat", &[1.0f64, 2.0, 3.0, 4.0, 5.0, 6.0], 2, None, Some("%.0f"), [[0.0, 0.0], [3.0, 2.0]], HeatmapFlags::NONE);
                plot.heatmap("heat scaled", &[1i8, -2, 3, -4], 2, Some(-5.0..=5.0), None, [[4.0, 0.0], [6.0, 2.0]], HeatmapFlags::COL_MAJOR);
                let tallest = plot.histogram("histogram", &ys, Bins::Count(5), 1.0, None, HistogramFlags::NONE);
                plot.histogram("density", &floats, Bins::Sturges, 1.0, Some(-10.0..=10.0), HistogramFlags::DENSITY | HistogramFlags::CUMULATIVE);
                let most = plot.histogram_2d("2d", &xs, &ys, Bins::Sqrt, Bins::Rice, None, HistogramFlags::NONE);
                plot.digital("digital", &[0.0, 1.0, 2.0], &[0.0, 1.0, 0.0], DigitalFlags::NONE);
                plot.image("image", texture.id().unwrap(), [[0.0, 0.0], [2.0, 2.0]], [[0.0, 0.0], [1.0, 1.0]], [1.0; 4], PlotImageFlags::NONE);
                plot.text("label", [10.0, 0.0], [0.0, -10.0], false);
                plot.text("sideways", [11.0, 0.0], [0.0, 0.0], true);
                plot.dummy("dummy", DummyFlags::NONE);
                plot.hide_next_item(true, Cond::Once);
                plot.set_axes(Axis::X1, Axis::Y2);
                plot.line("on the right", &[0.1f32, 0.5, 0.9]);
                plot.set_axes(Axis::X1, Axis::Y1);

                let tools = (
                    plot.drag_point(1, &mut point, [1.0; 4], 4.0, DragToolFlags::NONE),
                    plot.drag_line_x(2, &mut line_x, [1.0; 4], 1.0, DragToolFlags::NO_FIT),
                    plot.drag_line_y(3, &mut line_y, [1.0; 4], 1.0, DragToolFlags::DELAYED),
                    plot.drag_rect(4, &mut rect, [1.0; 4], DragToolFlags::NONE),
                );
                plot.annotation([5.0, 5.0], [1.0; 4], [10.0, 10.0], true, Some("note %s %n"));
                plot.annotation([6.0, 6.0], [1.0; 4], [0.0, 0.0], false, None);
                plot.tag_x(7.0, [1.0; 4], Some("tag"));
                plot.tag_y(2.0, [1.0; 4], None);

                let pixels = plot.plot_to_pixels([10.0, 0.0]);
                let back = plot.pixels_to_plot(pixels);
                let (pos, size) = plot.plot_rect();
                let limits = plot.limits();
                let queries = (plot.is_hovered(), plot.is_axis_hovered(Axis::X1), plot.selection(), plot.is_legend_entry_hovered("line"));
                plot.cancel_selection();
                let _ = (plot.mouse_pos(), plot.last_item_color());
                plot.legend_popup("line", |ui| ui.text("never shown"));
                let drew = plot.draw(|draw| {
                    draw.add_circle_filled(pixels, 5.0, [1.0, 1.0, 0.0, 1.0]);
                    true
                });
                plot.drag_drop_target(|_| ());
                plot.axis_drag_drop_target(Axis::X1, |_| ());
                plot.legend_drag_drop_target(|_| ());
                plot.drag_drop_source(DragDropFlags::NONE, |_| ());
                plot.axis_drag_drop_source(Axis::Y1, DragDropFlags::NONE, |_| ());
                plot.item_drag_drop_source("line", DragDropFlags::NONE, |_| ());
                (tallest > 0.0, most > 0.0, tools.0.changed, (back[0] - 10.0).abs() < 0.01, size[0] > 0.0 && pos[0] > 0.0, *limits.x.end() > 0.0, queries.2, drew)
            });
            (drawn, implot_state())
        })
    });
    let (drawn, state) = result.unwrap().unwrap();
    assert_eq!(drawn, Some((true, true, false, true, true, true, None, true)));
    assert_eq!(state, (false, false, 0, 0), "the plot ended");
    assert_eq!(take_error(), None);
    drop(texture);
    free_retired_textures();
}

#[test]
fn every_value_type_plots() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            ui.plot("types").show(|plot| {
                plot.line("f32", &[1.0f32, 2.0]);
                plot.line("f64", &[1.0f64, 2.0]);
                plot.line("i8", &[1i8, 2]);
                plot.line("u8", &[1u8, 2]);
                plot.line("i16", &[1i16, 2]);
                plot.line("u16", &[1u16, 2]);
                plot.line("i32", &[1i32, 2]);
                plot.line("u32", &[1u32, 2]);
                plot.line("i64", &[1i64, 2]);
                plot.line("u64", &[1u64, 2]);
                plot.line("empty", &[0.0f32; 0]);
            })
        })
    });
    assert_eq!(result, Ok(Some(Some(()))));
    assert_eq!(take_error(), None);
}

#[test]
fn an_unknown_value_type_is_an_imgui_error() {
    // The negative control for every_value_type_plots: the shim checks the kind.
    let _imgui = lock();
    frame(|ui| {
        test_window(ui, |ui| {
            ui.plot("bad kind").show(|plot| {
                let api = plot.api();
                let (l, n) = s("bad");
                let ys = [1.0f32];
                unsafe { (api.mqemu_implot_line)(l, n, 99, std::ptr::null(), ys.as_ptr().cast(), 1, 1.0, 0.0, 0) };
            })
        })
    })
    .unwrap();
    assert_eq!(take_error().as_deref(), Some("unknown ImPlot value type"));
}

#[test]
fn plot_scopes_end_when_their_closure_panics() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let panics = [
                catch_unwind(AssertUnwindSafe(|| ui.plot("panics").show(|plot| {
                    plot.line("x", &[1.0f32]);
                    panic!("deliberate")
                })))
                .is_err(),
                catch_unwind(AssertUnwindSafe(|| {
                    ui.subplots("grid", 1, 2).show(|grid| {
                        grid.plot("a", PlotFlags::NONE, |_| ());
                        grid.plot("b", PlotFlags::NONE, |_| panic!("deliberate"))
                    })
                }))
                .is_err(),
                catch_unwind(AssertUnwindSafe(|| {
                    ui.with_plot_style(PlotStyleVar::LineWeight(3.0), |ui| {
                        ui.with_plot_style_color(PlotStyleColor::Line, [1.0; 4], |ui| ui.with_colormap(Colormap::JET, |_| panic!("deliberate")))
                    })
                }))
                .is_err(),
                catch_unwind(AssertUnwindSafe(|| ui.aligned_plots("aligned", true, |_| panic!("deliberate")))).is_err(),
            ];
            (panics, implot_state())
        })
    });
    assert_eq!(result, Ok(Some(([true; 4], (false, false, 0, 0)))));
    assert_eq!(take_error(), None);
}

#[test]
fn setup_after_plotting_panics_before_implot_sees_it() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            ui.plot("late setup").show(|plot| {
                plot.next_line_style(None, None);
                plot.hide_next_item(false, Cond::Always);
                plot.setup_axis(Axis::X1, Some("still fine"), AxisFlags::NONE);
                plot.line("x", &[1.0f32]);
                catch_unwind(AssertUnwindSafe(|| plot.setup_axis_limits(Axis::Y1, 0.0..=1.0, Cond::Always))).is_err()
            })
        })
    });
    assert_eq!(result, Ok(Some(Some(true))));
    assert_eq!(take_error(), None, "ImPlot never saw the late setup");
}

#[test]
fn implot_reports_setup_after_plotting_as_an_error() {
    // What setup_after_plotting_panics_before_implot_sees_it keeps from ImPlot.
    let _imgui = lock();
    frame(|ui| {
        test_window(ui, |ui| {
            ui.plot("raw late setup").show(|plot| {
                plot.line("x", &[1.0f32]);
                unsafe { (plot.api().mqemu_implot_setup_axis_limits)(sys_plot::ImAxis_Y1, 0.0, 1.0, 1) };
            })
        })
    })
    .unwrap();
    let error = take_error().unwrap_or_default();
    assert!(error.contains("Setup needs to be called after BeginPlot and before any setup locking functions"), "{error}");
}

#[test]
fn every_plot_style_var_has_the_right_kind() {
    let _imgui = lock();
    let vars = [
        PlotStyleVar::LineWeight(2.0),
        PlotStyleVar::Marker(Marker::Square),
        PlotStyleVar::MarkerSize(3.0),
        PlotStyleVar::MarkerWeight(1.0),
        PlotStyleVar::FillAlpha(0.5),
        PlotStyleVar::ErrorBarSize(4.0),
        PlotStyleVar::ErrorBarWeight(1.0),
        PlotStyleVar::DigitalBitHeight(5.0),
        PlotStyleVar::DigitalBitGap(2.0),
        PlotStyleVar::PlotBorderSize(1.0),
        PlotStyleVar::MinorAlpha(0.2),
        PlotStyleVar::MajorTickLen([5.0, 5.0]),
        PlotStyleVar::MinorTickLen([2.0, 2.0]),
        PlotStyleVar::MajorTickSize([1.0, 1.0]),
        PlotStyleVar::MinorTickSize([1.0, 1.0]),
        PlotStyleVar::MajorGridSize([1.0, 1.0]),
        PlotStyleVar::MinorGridSize([1.0, 1.0]),
        PlotStyleVar::PlotPadding([5.0, 5.0]),
        PlotStyleVar::LabelPadding([2.0, 2.0]),
        PlotStyleVar::LegendPadding([5.0, 5.0]),
        PlotStyleVar::LegendInnerPadding([2.0, 2.0]),
        PlotStyleVar::LegendSpacing([2.0, 2.0]),
        PlotStyleVar::MousePosPadding([5.0, 5.0]),
        PlotStyleVar::AnnotationPadding([2.0, 2.0]),
        PlotStyleVar::FitPadding([0.1, 0.1]),
        PlotStyleVar::PlotDefaultSize([300.0, 200.0]),
        PlotStyleVar::PlotMinSize([100.0, 100.0]),
    ];
    for var in vars {
        let depth = frame(|ui| ui.with_plot_style(var, |_| implot_state().2)).unwrap();
        // ImPlot ignores a value of the wrong kind, so the pop would have
        // nothing to pop.
        assert_eq!((depth, take_error()), (1, None), "{var:?}");
    }
}

#[test]
fn a_plot_style_var_of_the_wrong_kind_is_not_pushed() {
    // The negative control for every_plot_style_var_has_the_right_kind.
    let _imgui = lock();
    let depth = frame(|ui| {
        let api = Ui::plot_api(ui);
        unsafe { (api.mqemu_implot_push_style_var_float)(sys_plot::ImPlotStyleVar_Marker, 1.0) };
        let depth = implot_state().2;
        unsafe { (api.mqemu_implot_pop_style_var)(1) };
        depth
    })
    .unwrap();
    assert_eq!(depth, 0);
    assert_eq!(take_error().as_deref(), Some("You can't pop more modifiers than have been pushed!"));
}

#[test]
fn colormaps_are_found_added_and_sampled() {
    let _imgui = lock();
    let result = frame(|ui| {
        let builtin = (ui.colormap_name(Colormap::DEEP), ui.colormap_named("Viridis"), ui.colormap_named("No such map"));
        let count = ui.colormaps().count();
        let name = format!("mq-emu test {}", ui.frame_count());
        let added = ui.add_colormap(&name, &[[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]], false);
        let again = ui.add_colormap(&name, &[[1.0; 4], [0.0; 4]], true);
        let map = added.unwrap();
        let colors = (ui.colormap_size(map), ui.colormap_color(map, 1), ui.colormap_color(map, 3), ui.sample_colormap(map, 0.5));
        let mut t = 0.25;
        let mut picked = [0.0; 4];
        test_window(ui, |ui| {
            ui.colormap_scale("scale", 0.0..=100.0, [0.0, 100.0], "%.0f%%", Some(map));
            ui.colormap_slider("slider", &mut t, Some(&mut picked), None);
            ui.colormap_button("button", [50.0, 20.0], Some(Colormap::HOT));
        });
        (builtin, count, ui.colormaps().count(), ui.colormap_named(&name) == added, again, colors, picked != [0.0; 4])
    });
    let (builtin, count, count_after, found, again, colors, picked) = result.unwrap();
    assert_eq!(builtin, (Some("Deep".to_owned()), Some(Colormap::VIRIDIS), None));
    assert_eq!((count_after, found, again), (count + 1, true, None));
    let (size, second, wrapped, middle) = colors;
    assert_eq!((size, second, wrapped), (2, [0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 1.0, 1.0]));
    assert!((middle[0] - 0.5).abs() < 0.01 && (middle[2] - 0.5).abs() < 0.01, "{middle:?}");
    assert!(picked);
    assert_eq!(take_error(), None);
}

#[test]
fn subplots_have_a_plot_per_cell() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            let (mut rows, mut cols) = ([1.0, 2.0], [1.0]);
            let drawn = ui
                .subplots("grid", 2, 1)
                .size([400.0, 300.0])
                .flags(SubplotFlags::LINK_ALL_X | SubplotFlags::SHARE_ITEMS)
                .row_ratios(&mut rows)
                .col_ratios(&mut cols)
                .show(|grid| {
                    let a = grid.plot("top", PlotFlags::NONE, |plot| plot.line("x", &[1.0f32, 2.0]));
                    let b = grid.plot("", PlotFlags::NO_TITLE, |plot| plot.bars("y", &[1.0f32, 2.0], 0.5));
                    let extra = catch_unwind(AssertUnwindSafe(|| grid.plot("third", PlotFlags::NONE, |_| ()))).is_err();
                    (a.is_some(), b.is_some(), extra, grid.is_hovered())
                });
            (drawn, implot_state())
        })
    });
    assert_eq!(result, Ok(Some((Some((true, true, true, false)), (false, false, 0, 0)))));
    assert_eq!(take_error(), None);
}

#[test]
#[should_panic(expected = "as many x values as y values")]
fn a_series_needs_as_many_xs_as_ys() {
    let _ = Series::xy(&[1.0f32, 2.0], &[1.0]);
}

#[test]
fn plot_argument_mistakes_panic_before_reaching_implot() {
    let _imgui = lock();
    let result = frame(|ui| {
        test_window(ui, |ui| {
            ui.plot("mistakes").show(|plot| {
                let mut panics = Vec::new();
                let mut check = |name: &str, f: &mut dyn FnMut(&mut Plot<'_>)| {
                    panics.push((name.to_owned(), catch_unwind(AssertUnwindSafe(|| f(plot))).is_err()));
                };
                check("format", &mut |p| p.setup_axis_format(Axis::X1, "%d"));
                check("tick labels", &mut |p| p.setup_axis_ticks(Axis::X1, &[1.0, 2.0], Some(&["one"]), false));
                check("pie labels", &mut |p| p.pie_chart(&["one"], &[1.0f32, 2.0], [0.0, 0.0], 1.0, None, 0.0, PieChartFlags::NONE));
                check("pie format", &mut |p| p.pie_chart(&["one"], &[1.0f32], [0.0, 0.0], 1.0, Some("%s"), 0.0, PieChartFlags::NONE));
                check("heatmap rows", &mut |p| p.heatmap("h", &[1.0f32, 2.0, 3.0], 2, None, None, [[0.0, 0.0], [1.0, 1.0]], HeatmapFlags::NONE));
                check("bar groups", &mut |p| p.bar_groups(&["a", "b"], &[1.0f32, 2.0, 3.0], 0.5, 0.0, BarGroupsFlags::NONE));
                check("error bars", &mut |p| p.error_bars("e", &[1.0f32], &[1.0], &[1.0, 2.0], None, ErrorBarsFlags::NONE));
                check("shaded between", &mut |p| p.shaded_between("s", &[1.0f32], &[1.0], &[], ShadedFlags::NONE));
                panics.into_iter().filter(|(_, panicked)| !panicked).map(|(name, _)| name).collect::<Vec<_>>()
            })
        })
    });
    assert_eq!(result, Ok(Some(Some(vec![]))), "mistakes that didn't panic");
    assert_eq!(take_error(), None);
}

#[test]
#[should_panic(expected = "at least 2 colors")]
fn a_colormap_needs_two_colors() {
    let mut ui = Ui::new(&fake::APIS);
    ui.add_colormap("one", &[[1.0; 4]], true);
}

#[test]
fn bad_implot_indexes_are_imgui_errors() {
    // PlotStyleColor and Colormap only hold valid values; the shim checks raw
    // ones, which ImPlot would index arrays with unchecked.
    let _imgui = lock();
    let mut errors = Vec::new();
    for call in [
        (|api: &sys_plot::Api| unsafe { (api.mqemu_implot_push_style_color)(99, [1.0f32; 4].as_ptr()) }) as fn(&sys_plot::Api),
        |api| unsafe {
            (api.mqemu_implot_colormap_size)(1000);
        },
        |api| unsafe { (api.mqemu_implot_push_colormap)(-1) },
    ] {
        frame(|ui| call(ui.plot_api())).unwrap();
        errors.push(take_error());
    }
    assert_eq!(errors, [Some("not an ImPlotCol_ value".to_owned()), Some("not a colormap".to_owned()), Some("not a colormap".to_owned())]);
    assert_eq!(implot_state(), (false, false, 0, 0), "nothing was pushed");
}
