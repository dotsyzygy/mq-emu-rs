// C wrappers around ImPlot 0.17, which MacroQuest builds into imgui.dll and
// gives one context, current during OnUpdateImGui. Conventions as in
// imgui.cpp; ImPlot reports user errors through the same ImGuiException.
//
// ImPlot's plotting functions are templates over the value type. Here each
// takes a `kind` (MQEMU_PLOT_*) saying what its arrays hold, and the arrays
// are contiguous. Optional colors are null for ImPlot's automatic choice.

#include "imgui_shim.h"

#include "imgui/implot/implot.h"

#include <cstdint>
#include <vector>

using namespace mqemu;

namespace {

enum Kind
{
	MQEMU_PLOT_F32,
	MQEMU_PLOT_F64,
	MQEMU_PLOT_I8,
	MQEMU_PLOT_U8,
	MQEMU_PLOT_I16,
	MQEMU_PLOT_U16,
	MQEMU_PLOT_I32,
	MQEMU_PLOT_U32,
	MQEMU_PLOT_I64,
	MQEMU_PLOT_U64,
};

template <typename T>
struct Type { using type = T; };

// Calls `f(Type<T>{})` for the type `kind` names; ImGuiException for others.
template <typename F>
void dispatch(int kind, F&& f)
{
	switch (kind)
	{
	case MQEMU_PLOT_F32: return f(Type<float>{});
	case MQEMU_PLOT_F64: return f(Type<double>{});
	case MQEMU_PLOT_I8: return f(Type<ImS8>{});
	case MQEMU_PLOT_U8: return f(Type<ImU8>{});
	case MQEMU_PLOT_I16: return f(Type<ImS16>{});
	case MQEMU_PLOT_U16: return f(Type<ImU16>{});
	case MQEMU_PLOT_I32: return f(Type<ImS32>{});
	case MQEMU_PLOT_U32: return f(Type<ImU32>{});
	case MQEMU_PLOT_I64: return f(Type<ImS64>{});
	case MQEMU_PLOT_U64: return f(Type<ImU64>{});
	default: throw ImGuiException("unknown ImPlot value type");
	}
}

template <typename T>
const T* typed(const void* values)
{
	return static_cast<const T*>(values);
}

ImVec4 auto_color(const float* rgba)
{
	return rgba ? color(rgba) : IMPLOT_AUTO_COL;
}

void write_color(const ImVec4& c, float* out)
{
	out[0] = c.x;
	out[1] = c.y;
	out[2] = c.z;
	out[3] = c.w;
}

// Labels given as parallel arrays of pointers and lengths, as the
// NUL-terminated array ImPlot wants.
class Labels
{
public:
	Labels(const char* const* texts, const size_t* lens, int count)
	{
		if (!texts)
			return;
		m_strings.reserve(count);
		for (int i = 0; i < count; ++i)
			m_strings.emplace_back(texts[i], lens[i]);
		for (const std::string& s : m_strings)
			m_pointers.push_back(s.c_str());
	}

	const char* const* get() const { return m_pointers.empty() ? nullptr : m_pointers.data(); }

private:
	std::vector<std::string> m_strings;
	std::vector<const char*> m_pointers;
};

} // namespace

// ----- plots -----

// Call mqemu_implot_end_plot only if this returns true.
extern "C" bool mqemu_implot_begin_plot(const char* title, size_t title_len, float w, float h, int flags)
{
	return guarded(false, [&] { return ImPlot::BeginPlot(CString(title, title_len), ImVec2(w, h), flags); });
}

extern "C" void mqemu_implot_end_plot()
{
	guarded_void([] { ImPlot::EndPlot(); });
}

// A grid of `rows` x `cols` plots, each begun with mqemu_implot_begin_plot.
// The optional ratio arrays (`rows` and `cols` floats) are updated as the
// user resizes. Call mqemu_implot_end_subplots only if this returns true.
extern "C" bool mqemu_implot_begin_subplots(const char* title, size_t title_len, int rows, int cols, float w, float h,
	int flags, float* row_ratios, float* col_ratios)
{
	return guarded(false, [&] {
		return ImPlot::BeginSubplots(CString(title, title_len), rows, cols, ImVec2(w, h), flags, row_ratios, col_ratios);
	});
}

extern "C" void mqemu_implot_end_subplots()
{
	guarded_void([] { ImPlot::EndSubplots(); });
}

// Lines up the axes of the plots begun until mqemu_implot_end_aligned_plots.
extern "C" bool mqemu_implot_begin_aligned_plots(const char* group, size_t group_len, bool vertical)
{
	return guarded(false, [&] { return ImPlot::BeginAlignedPlots(CString(group, group_len), vertical); });
}

extern "C" void mqemu_implot_end_aligned_plots()
{
	guarded_void([] { ImPlot::EndAlignedPlots(); });
}

// ----- setup: after begin_plot, before anything is plotted or queried -----

// `label_len` NO_STRING for no label.
extern "C" void mqemu_implot_setup_axis(int axis, const char* label, size_t label_len, int flags)
{
	guarded_void([&] { ImPlot::SetupAxis(axis, OptionalCString(label, label_len), flags); });
}

extern "C" void mqemu_implot_setup_axis_limits(int axis, double min, double max, int cond)
{
	guarded_void([&] { ImPlot::SetupAxisLimits(axis, min, max, cond); });
}

// `format` is a printf format for one double, e.g. "%.0f HP".
extern "C" void mqemu_implot_setup_axis_format(int axis, const char* format, size_t format_len)
{
	guarded_void([&] { ImPlot::SetupAxisFormat(axis, CString(format, format_len)); });
}

// Ticks at `count` `values` or, with `values` null, evenly from `min` to
// `max`; labeled with `labels` (`count` strings) if given.
extern "C" void mqemu_implot_setup_axis_ticks(int axis, const double* values, double min, double max, int count,
	const char* const* labels, const size_t* label_lens, bool keep_default)
{
	guarded_void([&] {
		Labels names(labels, label_lens, count);
		if (values)
			ImPlot::SetupAxisTicks(axis, values, count, names.get(), keep_default);
		else
			ImPlot::SetupAxisTicks(axis, min, max, count, names.get(), keep_default);
	});
}

extern "C" void mqemu_implot_setup_axis_scale(int axis, int scale)
{
	guarded_void([&] { ImPlot::SetupAxisScale(axis, scale); });
}

extern "C" void mqemu_implot_setup_axis_limits_constraints(int axis, double min, double max)
{
	guarded_void([&] { ImPlot::SetupAxisLimitsConstraints(axis, min, max); });
}

extern "C" void mqemu_implot_setup_axis_zoom_constraints(int axis, double min, double max)
{
	guarded_void([&] { ImPlot::SetupAxisZoomConstraints(axis, min, max); });
}

extern "C" void mqemu_implot_setup_legend(int location, int flags)
{
	guarded_void([&] { ImPlot::SetupLegend(location, flags); });
}

extern "C" void mqemu_implot_setup_mouse_text(int location, int flags)
{
	guarded_void([&] { ImPlot::SetupMouseText(location, flags); });
}

// Ends setup explicitly (plotting or querying does it implicitly).
extern "C" void mqemu_implot_setup_finish()
{
	guarded_void([] { ImPlot::SetupFinish(); });
}

// Before begin_plot: the next plot's axis limits, or fit them to the data.
extern "C" void mqemu_implot_set_next_axis_limits(int axis, double min, double max, int cond)
{
	guarded_void([&] { ImPlot::SetNextAxisLimits(axis, min, max, cond); });
}

extern "C" void mqemu_implot_set_next_axis_to_fit(int axis)
{
	guarded_void([&] { ImPlot::SetNextAxisToFit(axis); });
}

// ----- items -----
// Each needs a plot. A null `xs` plots `ys` against x = xstart + i * xscale.

extern "C" void mqemu_implot_line(const char* label, size_t label_len, int kind, const void* xs, const void* ys, int count,
	double xscale, double xstart, int flags)
{
	guarded_void([&] {
		CString id(label, label_len);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			if (xs)
				ImPlot::PlotLine(id, typed<T>(xs), typed<T>(ys), count, flags);
			else
				ImPlot::PlotLine(id, typed<T>(ys), count, xscale, xstart, flags);
		});
	});
}

extern "C" void mqemu_implot_scatter(const char* label, size_t label_len, int kind, const void* xs, const void* ys, int count,
	double xscale, double xstart, int flags)
{
	guarded_void([&] {
		CString id(label, label_len);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			if (xs)
				ImPlot::PlotScatter(id, typed<T>(xs), typed<T>(ys), count, flags);
			else
				ImPlot::PlotScatter(id, typed<T>(ys), count, xscale, xstart, flags);
		});
	});
}

extern "C" void mqemu_implot_stairs(const char* label, size_t label_len, int kind, const void* xs, const void* ys, int count,
	double xscale, double xstart, int flags)
{
	guarded_void([&] {
		CString id(label, label_len);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			if (xs)
				ImPlot::PlotStairs(id, typed<T>(xs), typed<T>(ys), count, flags);
			else
				ImPlot::PlotStairs(id, typed<T>(ys), count, xscale, xstart, flags);
		});
	});
}

// The area between `ys` and `ys2`, or with `ys2` null between `ys` and y = yref.
extern "C" void mqemu_implot_shaded(const char* label, size_t label_len, int kind, const void* xs, const void* ys,
	const void* ys2, int count, double yref, double xscale, double xstart, int flags)
{
	guarded_void([&] {
		CString id(label, label_len);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			if (xs && ys2)
				ImPlot::PlotShaded(id, typed<T>(xs), typed<T>(ys), typed<T>(ys2), count, flags);
			else if (xs)
				ImPlot::PlotShaded(id, typed<T>(xs), typed<T>(ys), count, yref, flags);
			else
				ImPlot::PlotShaded(id, typed<T>(ys), count, yref, xscale, xstart, flags);
		});
	});
}

// `bar_size` in plot units. A null `xs` puts bar i at x = shift + i.
extern "C" void mqemu_implot_bars(const char* label, size_t label_len, int kind, const void* xs, const void* ys, int count,
	double bar_size, double shift, int flags)
{
	guarded_void([&] {
		CString id(label, label_len);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			if (xs)
				ImPlot::PlotBars(id, typed<T>(xs), typed<T>(ys), count, bar_size, flags);
			else
				ImPlot::PlotBars(id, typed<T>(ys), count, bar_size, shift, flags);
		});
	});
}

// `values` is `item_count` rows of `group_count` values; `labels` names the
// items.
extern "C" void mqemu_implot_bar_groups(const char* const* labels, const size_t* label_lens, int kind, const void* values,
	int item_count, int group_count, double group_size, double shift, int flags)
{
	guarded_void([&] {
		Labels names(labels, label_lens, item_count);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			ImPlot::PlotBarGroups(names.get(), typed<T>(values), item_count, group_count, group_size, shift, flags);
		});
	});
}

// Error bars of `neg` below and `pos` above (or `neg` both ways when `pos` is null).
extern "C" void mqemu_implot_error_bars(const char* label, size_t label_len, int kind, const void* xs, const void* ys,
	const void* neg, const void* pos, int count, int flags)
{
	guarded_void([&] {
		CString id(label, label_len);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			if (pos)
				ImPlot::PlotErrorBars(id, typed<T>(xs), typed<T>(ys), typed<T>(neg), typed<T>(pos), count, flags);
			else
				ImPlot::PlotErrorBars(id, typed<T>(xs), typed<T>(ys), typed<T>(neg), count, flags);
		});
	});
}

// Stems from y = `ref`. A null `xs` puts stem i at x = start + i * scale.
extern "C" void mqemu_implot_stems(const char* label, size_t label_len, int kind, const void* xs, const void* ys, int count,
	double ref, double scale, double start, int flags)
{
	guarded_void([&] {
		CString id(label, label_len);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			if (xs)
				ImPlot::PlotStems(id, typed<T>(xs), typed<T>(ys), count, ref, flags);
			else
				ImPlot::PlotStems(id, typed<T>(ys), count, ref, scale, start, flags);
		});
	});
}

// Infinite vertical lines at `values` (horizontal with the flag).
extern "C" void mqemu_implot_inf_lines(const char* label, size_t label_len, int kind, const void* values, int count, int flags)
{
	guarded_void([&] {
		CString id(label, label_len);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			ImPlot::PlotInfLines(id, typed<T>(values), count, flags);
		});
	});
}

// A pie of `count` slices named by `labels`. `format` is a printf format for
// one double, labeling each slice; NO_STRING for none.
extern "C" void mqemu_implot_pie_chart(const char* const* labels, const size_t* label_lens, int kind, const void* values,
	int count, double x, double y, double radius, const char* format, size_t format_len, double angle0, int flags)
{
	guarded_void([&] {
		Labels names(labels, label_lens, count);
		OptionalCString fmt(format, format_len);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			// The format overload would hand a null format to snprintf; no
			// formatter is how ImPlot leaves labels out.
			if (fmt)
				ImPlot::PlotPieChart(names.get(), typed<T>(values), count, x, y, radius, fmt, angle0, flags);
			else
				ImPlot::PlotPieChart(names.get(), typed<T>(values), count, x, y, radius, ImPlotFormatter(nullptr), nullptr, angle0, flags);
		});
	});
}

// `rows` x `cols` values colored by the colormap from `scale_min` to
// `scale_max` (both 0 to fit), filling `bounds` (min x, min y, max x, max y).
// `format` labels each cell as for the pie chart.
extern "C" void mqemu_implot_heatmap(const char* label, size_t label_len, int kind, const void* values, int rows, int cols,
	double scale_min, double scale_max, const char* format, size_t format_len, const double* bounds, int flags)
{
	guarded_void([&] {
		CString id(label, label_len);
		OptionalCString fmt(format, format_len);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			ImPlot::PlotHeatmap(id, typed<T>(values), rows, cols, scale_min, scale_max, fmt, ImPlotPoint(bounds[0], bounds[1]),
				ImPlotPoint(bounds[2], bounds[3]), flags);
		});
	});
}

// Bins `values` (`bins` > 0, or an ImPlotBin_ method) between `range_min` and
// `range_max` (equal: the data's range). Returns the largest bin's height.
extern "C" double mqemu_implot_histogram(const char* label, size_t label_len, int kind, const void* values, int count, int bins,
	double bar_scale, double range_min, double range_max, int flags)
{
	return guarded(0.0, [&] {
		CString id(label, label_len);
		double result = 0.0;
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			result = ImPlot::PlotHistogram(id, typed<T>(values), count, bins, bar_scale, ImPlotRange(range_min, range_max), flags);
		});
		return result;
	});
}

// `range` is min x, max x, min y, max y (all 0: the data's range).
extern "C" double mqemu_implot_histogram_2d(const char* label, size_t label_len, int kind, const void* xs, const void* ys,
	int count, int x_bins, int y_bins, const double* range, int flags)
{
	return guarded(0.0, [&] {
		CString id(label, label_len);
		double result = 0.0;
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			result = ImPlot::PlotHistogram2D(id, typed<T>(xs), typed<T>(ys), count, x_bins, y_bins,
				ImPlotRect(range[0], range[1], range[2], range[3]), flags);
		});
		return result;
	});
}

// Digital signals (0 or 1 at each x), drawn as bands below the plot.
extern "C" void mqemu_implot_digital(const char* label, size_t label_len, int kind, const void* xs, const void* ys, int count, int flags)
{
	guarded_void([&] {
		CString id(label, label_len);
		dispatch(kind, [&](auto t) {
			using T = typename decltype(t)::type;
			ImPlot::PlotDigital(id, typed<T>(xs), typed<T>(ys), count, flags);
		});
	});
}

// A texture filling `bounds` (min x, min y, max x, max y, plot units).
extern "C" void mqemu_implot_image(const char* label, size_t label_len, void* texture, const double* bounds, const float* uv,
	const float* tint, int flags)
{
	guarded_void([&] {
		ImPlot::PlotImage(CString(label, label_len), texture_ref(texture), ImPlotPoint(bounds[0], bounds[1]),
			ImPlotPoint(bounds[2], bounds[3]), point(uv, 0), point(uv, 1), color(tint), flags);
	});
}

// Text centered at (x, y) in plot units, moved by (dx, dy) pixels.
extern "C" void mqemu_implot_text(const char* text, size_t len, double x, double y, float dx, float dy, int flags)
{
	guarded_void([&] { ImPlot::PlotText(CString(text, len), x, y, ImVec2(dx, dy), flags); });
}

// A legend entry without data.
extern "C" void mqemu_implot_dummy(const char* label, size_t label_len, int flags)
{
	guarded_void([&] { ImPlot::PlotDummy(CString(label, label_len), flags); });
}

// ----- tools -----
// True while dragged; `state` gets clicked, hovered and held.

extern "C" bool mqemu_implot_drag_point(int id, double* x, double* y, const float* col, float size, int flags, bool* state)
{
	return guarded(false, [&] { return ImPlot::DragPoint(id, x, y, color(col), size, flags, &state[0], &state[1], &state[2]); });
}

// A vertical line at *x, or with `horizontal` a horizontal one at *x.
extern "C" bool mqemu_implot_drag_line(int id, bool horizontal, double* value, const float* col, float thickness, int flags, bool* state)
{
	return guarded(false, [&] {
		if (horizontal)
			return ImPlot::DragLineY(id, value, color(col), thickness, flags, &state[0], &state[1], &state[2]);
		return ImPlot::DragLineX(id, value, color(col), thickness, flags, &state[0], &state[1], &state[2]);
	});
}

// `rect` is x1, y1, x2, y2.
extern "C" bool mqemu_implot_drag_rect(int id, double* rect, const float* col, int flags, bool* state)
{
	return guarded(false, [&] {
		return ImPlot::DragRect(id, &rect[0], &rect[1], &rect[2], &rect[3], color(col), flags, &state[0], &state[1], &state[2]);
	});
}

// A label at (x, y), moved by (dx, dy) pixels, kept inside the plot with
// `clamp`. With `text_len` NO_STRING, shows the y value.
extern "C" void mqemu_implot_annotation(double x, double y, const float* col, float dx, float dy, bool clamp, const char* text,
	size_t text_len)
{
	guarded_void([&] {
		if (text_len == SIZE_MAX)
			ImPlot::Annotation(x, y, color(col), ImVec2(dx, dy), clamp);
		else
			ImPlot::Annotation(x, y, color(col), ImVec2(dx, dy), clamp, "%s", static_cast<const char*>(CString(text, text_len)));
	});
}

// A tag on the x axis at `value` (the y axis with `y_axis`), showing `text`
// or (NO_STRING) the value.
extern "C" void mqemu_implot_tag(bool y_axis, double value, const float* col, const char* text, size_t text_len)
{
	guarded_void([&] {
		if (text_len == SIZE_MAX && y_axis)
			ImPlot::TagY(value, color(col));
		else if (text_len == SIZE_MAX)
			ImPlot::TagX(value, color(col));
		else if (y_axis)
			ImPlot::TagY(value, color(col), "%s", static_cast<const char*>(CString(text, text_len)));
		else
			ImPlot::TagX(value, color(col), "%s", static_cast<const char*>(CString(text, text_len)));
	});
}

// ----- queries (inside a plot) -----

// The axes the next items plot against (y_axis -1 for only x).
extern "C" void mqemu_implot_set_axes(int x_axis, int y_axis)
{
	guarded_void([&] {
		if (y_axis < 0)
			ImPlot::SetAxis(x_axis);
		else
			ImPlot::SetAxes(x_axis, y_axis);
	});
}

// Axes are IMPLOT_AUTO (-1) for the current ones.
extern "C" void mqemu_implot_pixels_to_plot(float px, float py, int x_axis, int y_axis, double* x, double* y)
{
	guarded_void([&] {
		ImPlotPoint p = ImPlot::PixelsToPlot(px, py, x_axis, y_axis);
		*x = p.x;
		*y = p.y;
	});
}

extern "C" void mqemu_implot_plot_to_pixels(double x, double y, int x_axis, int y_axis, float* px, float* py)
{
	guarded_void([&] {
		ImVec2 p = ImPlot::PlotToPixels(x, y, x_axis, y_axis);
		*px = p.x;
		*py = p.y;
	});
}

// The plot area's top left and size, in pixels.
extern "C" void mqemu_implot_plot_rect(float* x, float* y, float* w, float* h)
{
	guarded_void([&] {
		ImVec2 pos = ImPlot::GetPlotPos();
		ImVec2 size = ImPlot::GetPlotSize();
		*x = pos.x;
		*y = pos.y;
		*w = size.x;
		*h = size.y;
	});
}

extern "C" void mqemu_implot_mouse_pos(int x_axis, int y_axis, double* x, double* y)
{
	guarded_void([&] {
		ImPlotPoint p = ImPlot::GetPlotMousePos(x_axis, y_axis);
		*x = p.x;
		*y = p.y;
	});
}

// `out` gets min x, max x, min y, max y.
extern "C" void mqemu_implot_limits(int x_axis, int y_axis, double* out)
{
	guarded_void([&] {
		ImPlotRect r = ImPlot::GetPlotLimits(x_axis, y_axis);
		out[0] = r.X.Min;
		out[1] = r.X.Max;
		out[2] = r.Y.Min;
		out[3] = r.Y.Max;
	});
}

extern "C" bool mqemu_implot_is_plot_hovered()
{
	return guarded(false, [] { return ImPlot::IsPlotHovered(); });
}

extern "C" bool mqemu_implot_is_axis_hovered(int axis)
{
	return guarded(false, [&] { return ImPlot::IsAxisHovered(axis); });
}

extern "C" bool mqemu_implot_is_subplots_hovered()
{
	return guarded(false, [] { return ImPlot::IsSubplotsHovered(); });
}

// The box the user selected (right-drag), if any; `out` as for mqemu_implot_limits.
extern "C" bool mqemu_implot_selection(int x_axis, int y_axis, double* out)
{
	return guarded(false, [&] {
		if (!ImPlot::IsPlotSelected())
			return false;
		ImPlotRect r = ImPlot::GetPlotSelection(x_axis, y_axis);
		out[0] = r.X.Min;
		out[1] = r.X.Max;
		out[2] = r.Y.Min;
		out[3] = r.Y.Max;
		return true;
	});
}

extern "C" void mqemu_implot_cancel_selection()
{
	guarded_void([] { ImPlot::CancelPlotSelection(); });
}

extern "C" void mqemu_implot_hide_next_item(bool hidden, int cond)
{
	guarded_void([&] { ImPlot::HideNextItem(hidden, cond); });
}

extern "C" bool mqemu_implot_is_legend_entry_hovered(const char* label, size_t label_len)
{
	return guarded(false, [&] { return ImPlot::IsLegendEntryHovered(CString(label, label_len)); });
}

// A popup opened by clicking `label`'s legend entry with `button`. Call
// mqemu_implot_end_legend_popup only if this returns true.
extern "C" bool mqemu_implot_begin_legend_popup(const char* label, size_t label_len, int button)
{
	return guarded(false, [&] { return ImPlot::BeginLegendPopup(CString(label, label_len), button); });
}

extern "C" void mqemu_implot_end_legend_popup()
{
	guarded_void([] { ImPlot::EndLegendPopup(); });
}

// ----- drawing in a plot -----

// The plot's draw list; draw between push and pop of the plot's clip
// rectangle (grown by `expand` pixels).
extern "C" ImDrawList* mqemu_implot_draw_list()
{
	return guarded<ImDrawList*>(nullptr, [] { return ImPlot::GetPlotDrawList(); });
}

extern "C" void mqemu_implot_push_clip_rect(float expand)
{
	guarded_void([&] { ImPlot::PushPlotClipRect(expand); });
}

extern "C" void mqemu_implot_pop_clip_rect()
{
	guarded_void([] { ImPlot::PopPlotClipRect(); });
}

// ----- drag and drop -----
// Plots, axes and legends as ImGui drag and drop targets and sources, used
// with mqemu_imgui_accept_drag_drop_payload / set_drag_drop_payload.
// `which` is 0 for the plot area, 1 an axis (`axis`), 2 the legend (targets)
// or a legend item (`label`, sources). End only if begin returns true.

extern "C" bool mqemu_implot_begin_drag_drop_target(int which, int axis)
{
	return guarded(false, [&] {
		switch (which)
		{
		case 0: return ImPlot::BeginDragDropTargetPlot();
		case 1: return ImPlot::BeginDragDropTargetAxis(axis);
		case 2: return ImPlot::BeginDragDropTargetLegend();
		default: return false;
		}
	});
}

extern "C" void mqemu_implot_end_drag_drop_target()
{
	guarded_void([] { ImPlot::EndDragDropTarget(); });
}

extern "C" bool mqemu_implot_begin_drag_drop_source(int which, int axis, const char* label, size_t label_len, int flags)
{
	return guarded(false, [&] {
		switch (which)
		{
		case 0: return ImPlot::BeginDragDropSourcePlot(flags);
		case 1: return ImPlot::BeginDragDropSourceAxis(axis, flags);
		case 2: return ImPlot::BeginDragDropSourceItem(CString(label, label_len), flags);
		default: return false;
		}
	});
}

extern "C" void mqemu_implot_end_drag_drop_source()
{
	guarded_void([] { ImPlot::EndDragDropSource(); });
}

// ----- style -----

// `idx` an ImPlotCol_ value; `rgba` 4 floats.
extern "C" void mqemu_implot_push_style_color(int idx, const float* rgba)
{
	guarded_void([&] {
		if (idx < 0 || idx >= ImPlotCol_COUNT)
			throw ImGuiException("not an ImPlotCol_ value");
		ImPlot::PushStyleColor(idx, color(rgba));
	});
}

extern "C" void mqemu_implot_pop_style_color(int count)
{
	guarded_void([&] { ImPlot::PopStyleColor(count); });
}

// `idx` an ImPlotStyleVar_ value of the right kind.
extern "C" void mqemu_implot_push_style_var_float(int idx, float value)
{
	guarded_void([&] { ImPlot::PushStyleVar(idx, value); });
}

extern "C" void mqemu_implot_push_style_var_int(int idx, int value)
{
	guarded_void([&] { ImPlot::PushStyleVar(idx, value); });
}

extern "C" void mqemu_implot_push_style_var_vec2(int idx, float x, float y)
{
	guarded_void([&] { ImPlot::PushStyleVar(idx, ImVec2(x, y)); });
}

extern "C" void mqemu_implot_pop_style_var(int count)
{
	guarded_void([&] { ImPlot::PopStyleVar(count); });
}

// The next item's style. Null colors and negative numbers (IMPLOT_AUTO) leave
// ImPlot's choice.
extern "C" void mqemu_implot_set_next_line_style(const float* col, float weight)
{
	guarded_void([&] { ImPlot::SetNextLineStyle(auto_color(col), weight); });
}

extern "C" void mqemu_implot_set_next_fill_style(const float* col, float alpha)
{
	guarded_void([&] { ImPlot::SetNextFillStyle(auto_color(col), alpha); });
}

// `marker` an ImPlotMarker_ value. ImPlotMarker_None and IMPLOT_AUTO are both
// -1: the style's marker, which is none unless pushed.
extern "C" void mqemu_implot_set_next_marker_style(int marker, float size, const float* fill, float weight, const float* outline)
{
	guarded_void([&] { ImPlot::SetNextMarkerStyle(marker, size, auto_color(fill), weight, auto_color(outline)); });
}

extern "C" void mqemu_implot_set_next_error_bar_style(const float* col, float size, float weight)
{
	guarded_void([&] { ImPlot::SetNextErrorBarStyle(auto_color(col), size, weight); });
}

extern "C" void mqemu_implot_last_item_color(float* out)
{
	guarded_void([&] { write_color(ImPlot::GetLastItemColor(), out); });
}

// ----- colormaps -----
// `cmap` an ImPlotColormap_ value, one from mqemu_implot_add_colormap, or -1
// for the current one.

namespace {

void check_colormap(int cmap)
{
	if (cmap < -1 || cmap >= ImPlot::GetColormapCount())
		throw ImGuiException("not a colormap");
}

} // namespace

extern "C" int mqemu_implot_colormap_count()
{
	return guarded(0, [] { return ImPlot::GetColormapCount(); });
}

// Adds a colormap of `count` RGBA colors (4 floats each), smoothly blended
// unless `qualitative`. -1 if `name` is taken.
extern "C" int mqemu_implot_add_colormap(const char* name, size_t name_len, const float* colors, int count, bool qualitative)
{
	return guarded(-1, [&] {
		CString id(name, name_len);
		if (ImPlot::GetColormapIndex(id) != -1)
			return -1;
		if (count < 2)
			throw ImGuiException("a colormap needs at least 2 colors");
		return ImPlot::AddColormap(id, reinterpret_cast<const ImVec4*>(colors), count, qualitative);
	});
}

// The colormap's name (static), or null.
extern "C" const char* mqemu_implot_colormap_name(int cmap)
{
	return guarded<const char*>(nullptr, [&] {
		check_colormap(cmap);
		return cmap < 0 ? nullptr : ImPlot::GetColormapName(cmap);
	});
}

extern "C" int mqemu_implot_colormap_index(const char* name, size_t name_len)
{
	return guarded(-1, [&] { return ImPlot::GetColormapIndex(CString(name, name_len)); });
}

extern "C" int mqemu_implot_colormap_size(int cmap)
{
	return guarded(0, [&] {
		check_colormap(cmap);
		return ImPlot::GetColormapSize(cmap);
	});
}

// Color `index` (wrapping) of the colormap, or with `sample` its color at
// `t` (0 to 1), into `out`.
extern "C" void mqemu_implot_colormap_color(int cmap, int index, bool sample, float t, float* out)
{
	guarded_void([&] {
		check_colormap(cmap);
		write_color(sample ? ImPlot::SampleColormap(t, cmap) : ImPlot::GetColormapColor(index, cmap), out);
	});
}

extern "C" void mqemu_implot_next_colormap_color(float* out)
{
	guarded_void([&] { write_color(ImPlot::NextColormapColor(), out); });
}

extern "C" void mqemu_implot_push_colormap(int cmap)
{
	guarded_void([&] {
		check_colormap(cmap);
		if (cmap < 0)
			throw ImGuiException("not a colormap");
		ImPlot::PushColormap(cmap);
	});
}

extern "C" void mqemu_implot_pop_colormap(int count)
{
	guarded_void([&] { ImPlot::PopColormap(count); });
}

// A vertical bar showing the colormap from `min` to `max`, labeled with
// `format` (a printf format for one double).
extern "C" void mqemu_implot_colormap_scale(const char* label, size_t label_len, double min, double max, float w, float h,
	const char* format, size_t format_len, int flags, int cmap)
{
	guarded_void([&] {
		check_colormap(cmap);
		ImPlot::ColormapScale(CString(label, label_len), min, max, ImVec2(w, h), CString(format, format_len), flags, cmap);
	});
}

// A slider over the colormap: `*t` 0 to 1, its color into `out` (4 floats,
// may be null). `format` as for a float slider.
extern "C" bool mqemu_implot_colormap_slider(const char* label, size_t label_len, float* t, float* out, const char* format,
	size_t format_len, int cmap)
{
	return guarded(false, [&] {
		check_colormap(cmap);
		ImVec4 picked;
		bool changed = ImPlot::ColormapSlider(CString(label, label_len), t, &picked, CString(format, format_len), cmap);
		if (out)
			write_color(picked, out);
		return changed;
	});
}

extern "C" bool mqemu_implot_colormap_button(const char* label, size_t label_len, float w, float h, int cmap)
{
	return guarded(false, [&] {
		check_colormap(cmap);
		return ImPlot::ColormapButton(CString(label, label_len), ImVec2(w, h), cmap);
	});
}
