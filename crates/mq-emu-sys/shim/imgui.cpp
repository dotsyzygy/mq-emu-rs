// C wrappers around Dear ImGui as MacroQuest builds it (imgui.dll, with
// MacroQuest's imconfig.h).
//
// ImGui's API is C++ (overloads, default arguments, references, varargs), so
// Rust can't call imgui.dll directly. These wrappers take plain C arguments:
// strings as (pointer, length), vectors as separate floats. Display text never
// goes through a printf format, so user text can't be misread as one.
//
// MacroQuest's imconfig.h makes ImGui throw ImGuiException on user errors
// (mismatched Begin/End and the like). No exception may cross into Rust:
// every entry point catches them and records the message, which Rust reads
// with mqemu_imgui_take_error. Many misuses aren't errors but crashes in a
// release imgui.dll (TableNextRow outside a table dereferences null), so the
// safe API in mq-emu must make them impossible; this layer can't.
//
// This file imports imgui.dll, so only binaries that call it link it. Draw
// lists and images are in imgui_draw.cpp, ImPlot in implot.cpp, and what
// needs MQ2Main.dll (game textures, EQ fonts) in imgui_mq.cpp.

#include "imgui_shim.h"

#include "imgui/ImGuiUtils.h"
#include "imgui/imgui_internal.h"

#include <cstdint>
#include <vector>

using namespace mqemu;

namespace {

// The last ImGui error caught, until taken. Each plugin links its own copy.
std::string s_error;
bool s_has_error = false;

} // namespace

void mqemu::note_error(const char* what)
{
	// Keep the first one: later errors are usually its consequences.
	if (!s_has_error)
	{
		s_error = what ? what : "unknown C++ exception";
		s_has_error = true;
	}
}

// ----- errors -----

// Copies the caught error (NUL-terminated, truncated to `cap`) and clears it.
// False if there was none.
extern "C" bool mqemu_imgui_take_error(char* out, size_t cap)
{
	if (!s_has_error)
		return false;
	if (out && cap)
	{
		size_t len = s_error.size() < cap - 1 ? s_error.size() : cap - 1;
		memcpy(out, s_error.data(), len);
		out[len] = 0;
	}
	s_error.clear();
	s_has_error = false;
	return true;
}

// ----- windows -----

extern "C" bool mqemu_imgui_begin(const char* name, size_t name_len, bool* open, int flags)
{
	return guarded(false, [&] { return ImGui::Begin(CString(name, name_len), open, flags); });
}

extern "C" void mqemu_imgui_end()
{
	guarded_void([] { ImGui::End(); });
}

extern "C" bool mqemu_imgui_begin_child(const char* id, size_t id_len, float w, float h, int child_flags, int window_flags)
{
	return guarded(false, [&] { return ImGui::BeginChild(CString(id, id_len), ImVec2(w, h), child_flags, window_flags); });
}

extern "C" void mqemu_imgui_end_child()
{
	guarded_void([] { ImGui::EndChild(); });
}

extern "C" void mqemu_imgui_set_next_window_pos(float x, float y, int cond, float pivot_x, float pivot_y)
{
	guarded_void([&] { ImGui::SetNextWindowPos(ImVec2(x, y), cond, ImVec2(pivot_x, pivot_y)); });
}

extern "C" void mqemu_imgui_set_next_window_size(float w, float h, int cond)
{
	guarded_void([&] { ImGui::SetNextWindowSize(ImVec2(w, h), cond); });
}

extern "C" void mqemu_imgui_set_next_window_size_constraints(float min_w, float min_h, float max_w, float max_h)
{
	guarded_void([&] { ImGui::SetNextWindowSizeConstraints(ImVec2(min_w, min_h), ImVec2(max_w, max_h)); });
}

extern "C" void mqemu_imgui_set_next_window_collapsed(bool collapsed, int cond)
{
	guarded_void([&] { ImGui::SetNextWindowCollapsed(collapsed, cond); });
}

extern "C" bool mqemu_imgui_is_window_appearing()
{
	return guarded(false, [] { return ImGui::IsWindowAppearing(); });
}

extern "C" void mqemu_imgui_content_region_avail(float* w, float* h)
{
	guarded_void([&] {
		ImVec2 avail = ImGui::GetContentRegionAvail();
		*w = avail.x;
		*h = avail.y;
	});
}

// ----- layout -----

extern "C" void mqemu_imgui_separator()
{
	guarded_void([] { ImGui::Separator(); });
}

extern "C" void mqemu_imgui_separator_text(const char* label, size_t label_len)
{
	guarded_void([&] { ImGui::SeparatorText(CString(label, label_len)); });
}

extern "C" void mqemu_imgui_same_line(float offset_from_start_x, float spacing)
{
	guarded_void([&] { ImGui::SameLine(offset_from_start_x, spacing); });
}

extern "C" void mqemu_imgui_new_line()
{
	guarded_void([] { ImGui::NewLine(); });
}

extern "C" void mqemu_imgui_spacing()
{
	guarded_void([] { ImGui::Spacing(); });
}

extern "C" void mqemu_imgui_dummy(float w, float h)
{
	guarded_void([&] { ImGui::Dummy(ImVec2(w, h)); });
}

extern "C" void mqemu_imgui_indent(float width)
{
	guarded_void([&] { ImGui::Indent(width); });
}

extern "C" void mqemu_imgui_unindent(float width)
{
	guarded_void([&] { ImGui::Unindent(width); });
}

extern "C" void mqemu_imgui_begin_group()
{
	guarded_void([] { ImGui::BeginGroup(); });
}

extern "C" void mqemu_imgui_end_group()
{
	guarded_void([] { ImGui::EndGroup(); });
}

extern "C" void mqemu_imgui_align_text_to_frame_padding()
{
	guarded_void([] { ImGui::AlignTextToFramePadding(); });
}

extern "C" void mqemu_imgui_set_next_item_width(float width)
{
	guarded_void([&] { ImGui::SetNextItemWidth(width); });
}

// ----- ID, style and state stacks -----

extern "C" void mqemu_imgui_push_id_str(const char* id, size_t id_len)
{
	guarded_void([&] { ImGui::PushID(id, id + id_len); });
}

extern "C" void mqemu_imgui_push_id_int(int id)
{
	guarded_void([&] { ImGui::PushID(id); });
}

extern "C" void mqemu_imgui_pop_id()
{
	guarded_void([] { ImGui::PopID(); });
}

extern "C" void mqemu_imgui_push_style_color(int idx, float r, float g, float b, float a)
{
	guarded_void([&] { ImGui::PushStyleColor(idx, ImVec4(r, g, b, a)); });
}

extern "C" void mqemu_imgui_pop_style_color(int count)
{
	guarded_void([&] { ImGui::PopStyleColor(count); });
}

extern "C" void mqemu_imgui_push_style_var_float(int idx, float value)
{
	guarded_void([&] { ImGui::PushStyleVar(idx, value); });
}

extern "C" void mqemu_imgui_push_style_var_vec2(int idx, float x, float y)
{
	guarded_void([&] { ImGui::PushStyleVar(idx, ImVec2(x, y)); });
}

extern "C" void mqemu_imgui_pop_style_var(int count)
{
	guarded_void([&] { ImGui::PopStyleVar(count); });
}

extern "C" void mqemu_imgui_push_text_wrap_pos(float wrap_local_pos_x)
{
	guarded_void([&] { ImGui::PushTextWrapPos(wrap_local_pos_x); });
}

extern "C" void mqemu_imgui_pop_text_wrap_pos()
{
	guarded_void([] { ImGui::PopTextWrapPos(); });
}

extern "C" void mqemu_imgui_begin_disabled(bool disabled)
{
	guarded_void([&] { ImGui::BeginDisabled(disabled); });
}

extern "C" void mqemu_imgui_end_disabled()
{
	guarded_void([] { ImGui::EndDisabled(); });
}

// ----- text -----

extern "C" void mqemu_imgui_text(const char* text, size_t len)
{
	guarded_void([&] { ImGui::TextUnformatted(text, text + len); });
}

extern "C" void mqemu_imgui_label_text(const char* label, size_t label_len, const char* text, size_t text_len)
{
	guarded_void([&] { ImGui::LabelText(CString(label, label_len), "%s", static_cast<const char*>(CString(text, text_len))); });
}

extern "C" void mqemu_imgui_bullet()
{
	guarded_void([] { ImGui::Bullet(); });
}

extern "C" bool mqemu_imgui_text_link(const char* label, size_t label_len)
{
	return guarded(false, [&] { return ImGui::TextLink(CString(label, label_len)); });
}

extern "C" void mqemu_imgui_calc_text_size(const char* text, size_t len, float* w, float* h)
{
	guarded_void([&] {
		ImVec2 size = ImGui::CalcTextSize(text, text + len);
		*w = size.x;
		*h = size.y;
	});
}

// ----- widgets -----

extern "C" bool mqemu_imgui_button(const char* label, size_t label_len, float w, float h)
{
	return guarded(false, [&] { return ImGui::Button(CString(label, label_len), ImVec2(w, h)); });
}

extern "C" bool mqemu_imgui_small_button(const char* label, size_t label_len)
{
	return guarded(false, [&] { return ImGui::SmallButton(CString(label, label_len)); });
}

extern "C" bool mqemu_imgui_checkbox(const char* label, size_t label_len, bool* value)
{
	return guarded(false, [&] { return ImGui::Checkbox(CString(label, label_len), value); });
}

extern "C" bool mqemu_imgui_radio_button(const char* label, size_t label_len, bool active)
{
	return guarded(false, [&] { return ImGui::RadioButton(CString(label, label_len), active); });
}

extern "C" bool mqemu_imgui_selectable(const char* label, size_t label_len, bool selected, int flags, float w, float h)
{
	return guarded(false, [&] { return ImGui::Selectable(CString(label, label_len), selected, flags, ImVec2(w, h)); });
}

// `format` is a printf format for the value, e.g. "%.1f ms": the caller must
// make sure it has exactly one conversion of the right type.
extern "C" bool mqemu_imgui_slider_float(const char* label, size_t label_len, float* value, float min, float max,
	const char* format, size_t format_len, int flags)
{
	return guarded(false, [&] {
		return ImGui::SliderFloat(CString(label, label_len), value, min, max, CString(format, format_len), flags);
	});
}

extern "C" bool mqemu_imgui_slider_int(const char* label, size_t label_len, int* value, int min, int max,
	const char* format, size_t format_len, int flags)
{
	return guarded(false, [&] {
		return ImGui::SliderInt(CString(label, label_len), value, min, max, CString(format, format_len), flags);
	});
}

extern "C" bool mqemu_imgui_drag_float(const char* label, size_t label_len, float* value, float speed, float min,
	float max, const char* format, size_t format_len, int flags)
{
	return guarded(false, [&] {
		return ImGui::DragFloat(CString(label, label_len), value, speed, min, max, CString(format, format_len), flags);
	});
}

extern "C" bool mqemu_imgui_drag_int(const char* label, size_t label_len, int* value, float speed, int min, int max,
	const char* format, size_t format_len, int flags)
{
	return guarded(false, [&] {
		return ImGui::DragInt(CString(label, label_len), value, speed, min, max, CString(format, format_len), flags);
	});
}

extern "C" bool mqemu_imgui_input_int(const char* label, size_t label_len, int* value, int step, int step_fast, int flags)
{
	return guarded(false, [&] { return ImGui::InputInt(CString(label, label_len), value, step, step_fast, flags); });
}

extern "C" bool mqemu_imgui_input_float(const char* label, size_t label_len, float* value, float step, float step_fast,
	const char* format, size_t format_len, int flags)
{
	return guarded(false, [&] {
		return ImGui::InputFloat(CString(label, label_len), value, step, step_fast, CString(format, format_len), flags);
	});
}

// Grows the caller's text buffer to hold at least `size` bytes (including the
// NUL) and returns it, possibly moved; its capacity goes in *capacity.
using mqemu_imgui_resize_fn = char* (*)(void* user, size_t size, size_t* capacity);

namespace {

struct InputTextResize
{
	mqemu_imgui_resize_fn resize;
	void* user;
};

int input_text_callback(ImGuiInputTextCallbackData* data)
{
	if (data->EventFlag == ImGuiInputTextFlags_CallbackResize)
	{
		auto* resize = static_cast<InputTextResize*>(data->UserData);
		size_t capacity = 0;
		char* buffer = resize->resize(resize->user, static_cast<size_t>(data->BufTextLen) + 1, &capacity);
		if (buffer)
		{
			data->Buf = buffer;
			data->BufSize = static_cast<int>(capacity);
		}
	}
	return 0;
}

} // namespace

// Edits the NUL-terminated text in `buffer` (`capacity` bytes), growing it
// through `resize` as ImGui needs. ImGui writes UTF-8.
extern "C" bool mqemu_imgui_input_text(const char* label, size_t label_len, char* buffer, size_t capacity,
	int flags, mqemu_imgui_resize_fn resize, void* user)
{
	return guarded(false, [&] {
		InputTextResize state{ resize, user };
		return ImGui::InputText(CString(label, label_len), buffer, capacity, flags | ImGuiInputTextFlags_CallbackResize,
			input_text_callback, &state);
	});
}

extern "C" bool mqemu_imgui_input_text_multiline(const char* label, size_t label_len, char* buffer, size_t capacity,
	float w, float h, int flags, mqemu_imgui_resize_fn resize, void* user)
{
	return guarded(false, [&] {
		InputTextResize state{ resize, user };
		return ImGui::InputTextMultiline(CString(label, label_len), buffer, capacity, ImVec2(w, h),
			flags | ImGuiInputTextFlags_CallbackResize, input_text_callback, &state);
	});
}

extern "C" bool mqemu_imgui_color_edit4(const char* label, size_t label_len, float* rgba, int flags)
{
	return guarded(false, [&] { return ImGui::ColorEdit4(CString(label, label_len), rgba, flags); });
}

// `overlay` is optional: pass len SIZE_MAX for none.
extern "C" void mqemu_imgui_progress_bar(float fraction, float w, float h, const char* overlay, size_t overlay_len)
{
	guarded_void([&] { ImGui::ProgressBar(fraction, ImVec2(w, h), OptionalCString(overlay, overlay_len)); });
}

// `overlay` is optional as for mqemu_imgui_progress_bar. FLT_MAX scales mean
// "fit the values".
extern "C" void mqemu_imgui_plot_lines(const char* label, size_t label_len, const float* values, int count, int offset,
	const char* overlay, size_t overlay_len, float scale_min, float scale_max, float w, float h, bool histogram)
{
	guarded_void([&] {
		if (histogram)
			ImGui::PlotHistogram(CString(label, label_len), values, count, offset, OptionalCString(overlay, overlay_len),
				scale_min, scale_max, ImVec2(w, h));
		else
			ImGui::PlotLines(CString(label, label_len), values, count, offset, OptionalCString(overlay, overlay_len),
				scale_min, scale_max, ImVec2(w, h));
	});
}

// ----- combo boxes -----

extern "C" bool mqemu_imgui_begin_combo(const char* label, size_t label_len, const char* preview, size_t preview_len, int flags)
{
	return guarded(false, [&] {
		return ImGui::BeginCombo(CString(label, label_len), CString(preview, preview_len), flags);
	});
}

extern "C" void mqemu_imgui_end_combo()
{
	guarded_void([] { ImGui::EndCombo(); });
}

extern "C" void mqemu_imgui_set_item_default_focus()
{
	guarded_void([] { ImGui::SetItemDefaultFocus(); });
}

// ----- trees -----

extern "C" bool mqemu_imgui_tree_node(const char* label, size_t label_len, int flags)
{
	return guarded(false, [&] { return ImGui::TreeNodeEx(CString(label, label_len), flags); });
}

extern "C" void mqemu_imgui_tree_pop()
{
	guarded_void([] { ImGui::TreePop(); });
}

extern "C" bool mqemu_imgui_collapsing_header(const char* label, size_t label_len, int flags)
{
	return guarded(false, [&] { return ImGui::CollapsingHeader(CString(label, label_len), flags); });
}

extern "C" void mqemu_imgui_set_next_item_open(bool open, int cond)
{
	guarded_void([&] { ImGui::SetNextItemOpen(open, cond); });
}

// ----- tabs -----

extern "C" bool mqemu_imgui_begin_tab_bar(const char* id, size_t id_len, int flags)
{
	return guarded(false, [&] { return ImGui::BeginTabBar(CString(id, id_len), flags); });
}

extern "C" void mqemu_imgui_end_tab_bar()
{
	guarded_void([] { ImGui::EndTabBar(); });
}

extern "C" bool mqemu_imgui_begin_tab_item(const char* label, size_t label_len, bool* open, int flags)
{
	return guarded(false, [&] { return ImGui::BeginTabItem(CString(label, label_len), open, flags); });
}

extern "C" void mqemu_imgui_end_tab_item()
{
	guarded_void([] { ImGui::EndTabItem(); });
}

// ----- tables -----

extern "C" bool mqemu_imgui_begin_table(const char* id, size_t id_len, int columns, int flags, float outer_w, float outer_h, float inner_width)
{
	return guarded(false, [&] {
		return ImGui::BeginTable(CString(id, id_len), columns, flags, ImVec2(outer_w, outer_h), inner_width);
	});
}

extern "C" void mqemu_imgui_end_table()
{
	guarded_void([] { ImGui::EndTable(); });
}

extern "C" void mqemu_imgui_table_setup_column(const char* label, size_t label_len, int flags, float init_width_or_weight)
{
	guarded_void([&] { ImGui::TableSetupColumn(CString(label, label_len), flags, init_width_or_weight); });
}

extern "C" void mqemu_imgui_table_setup_scroll_freeze(int columns, int rows)
{
	guarded_void([&] { ImGui::TableSetupScrollFreeze(columns, rows); });
}

extern "C" void mqemu_imgui_table_headers_row()
{
	guarded_void([] { ImGui::TableHeadersRow(); });
}

extern "C" void mqemu_imgui_table_next_row(int flags, float min_row_height)
{
	guarded_void([&] { ImGui::TableNextRow(flags, min_row_height); });
}

extern "C" bool mqemu_imgui_table_next_column()
{
	return guarded(false, [] { return ImGui::TableNextColumn(); });
}

extern "C" bool mqemu_imgui_table_set_column_index(int column)
{
	return guarded(false, [&] { return ImGui::TableSetColumnIndex(column); });
}

// ----- tooltips and popups -----

extern "C" bool mqemu_imgui_is_item_hovered(int flags)
{
	return guarded(false, [&] { return ImGui::IsItemHovered(flags); });
}

extern "C" bool mqemu_imgui_is_item_clicked(int button)
{
	return guarded(false, [&] { return ImGui::IsItemClicked(button); });
}

extern "C" bool mqemu_imgui_is_item_active()
{
	return guarded(false, [] { return ImGui::IsItemActive(); });
}

extern "C" void mqemu_imgui_set_tooltip(const char* text, size_t len)
{
	guarded_void([&] { ImGui::SetTooltip("%s", static_cast<const char*>(CString(text, len))); });
}

extern "C" bool mqemu_imgui_begin_tooltip()
{
	return guarded(false, [] { return ImGui::BeginTooltip(); });
}

extern "C" bool mqemu_imgui_begin_item_tooltip()
{
	return guarded(false, [] { return ImGui::BeginItemTooltip(); });
}

extern "C" void mqemu_imgui_end_tooltip()
{
	guarded_void([] { ImGui::EndTooltip(); });
}

extern "C" void mqemu_imgui_open_popup(const char* id, size_t id_len, int flags)
{
	guarded_void([&] { ImGui::OpenPopup(CString(id, id_len), flags); });
}

extern "C" bool mqemu_imgui_begin_popup(const char* id, size_t id_len, int flags)
{
	return guarded(false, [&] { return ImGui::BeginPopup(CString(id, id_len), flags); });
}

extern "C" void mqemu_imgui_end_popup()
{
	guarded_void([] { ImGui::EndPopup(); });
}

extern "C" void mqemu_imgui_close_current_popup()
{
	guarded_void([] { ImGui::CloseCurrentPopup(); });
}

// ----- menus -----

extern "C" bool mqemu_imgui_begin_menu_bar()
{
	return guarded(false, [] { return ImGui::BeginMenuBar(); });
}

extern "C" void mqemu_imgui_end_menu_bar()
{
	guarded_void([] { ImGui::EndMenuBar(); });
}

extern "C" bool mqemu_imgui_begin_menu(const char* label, size_t label_len, bool enabled)
{
	return guarded(false, [&] { return ImGui::BeginMenu(CString(label, label_len), enabled); });
}

extern "C" void mqemu_imgui_end_menu()
{
	guarded_void([] { ImGui::EndMenu(); });
}

// `shortcut` is optional as for mqemu_imgui_progress_bar.
extern "C" bool mqemu_imgui_menu_item(const char* label, size_t label_len, const char* shortcut, size_t shortcut_len,
	bool selected, bool enabled)
{
	return guarded(false, [&] {
		return ImGui::MenuItem(CString(label, label_len), OptionalCString(shortcut, shortcut_len), selected, enabled);
	});
}

// ----- frame info -----

extern "C" int mqemu_imgui_frame_count()
{
	return guarded(0, [] { return ImGui::GetFrameCount(); });
}

extern "C" double mqemu_imgui_time()
{
	return guarded(0.0, [] { return ImGui::GetTime(); });
}

extern "C" float mqemu_imgui_framerate()
{
	return guarded(0.0f, [] { return ImGui::GetIO().Framerate; });
}

extern "C" float mqemu_imgui_delta_time()
{
	return guarded(0.0f, [] { return ImGui::GetIO().DeltaTime; });
}

// ----- items, style and drawing -----

// The last item's rectangle, in screen coordinates.
extern "C" void mqemu_imgui_item_rect(float* min_x, float* min_y, float* max_x, float* max_y)
{
	guarded_void([&] {
		ImVec2 min = ImGui::GetItemRectMin();
		ImVec2 max = ImGui::GetItemRectMax();
		*min_x = min.x;
		*min_y = min.y;
		*max_x = max.x;
		*max_y = max.y;
	});
}

// The current style's color `idx` (an ImGuiCol_ value), into rgba[4].
extern "C" void mqemu_imgui_style_color(int idx, float* rgba)
{
	guarded_void([&] {
		const ImVec4& color = ImGui::GetStyleColorVec4(idx);
		rgba[0] = color.x;
		rgba[1] = color.y;
		rgba[2] = color.z;
		rgba[3] = color.w;
	});
}

// ImGui's own float-to-packed color conversion (no style alpha), for checking
// mq-emu's, which draw lists take.
extern "C" ImU32 mqemu_imgui_color_convert(float r, float g, float b, float a)
{
	return ImGui::ColorConvertFloat4ToU32(ImVec4(r, g, b, a));
}

// ----- cursor, mouse and window geometry -----

// The current font's line height, or a framed widget's height (a button, an
// input), each plus the spacing between items if `with_spacing`.
extern "C" float mqemu_imgui_text_line_height(bool with_spacing)
{
	return guarded(0.0f, [&] { return with_spacing ? ImGui::GetTextLineHeightWithSpacing() : ImGui::GetTextLineHeight(); });
}

extern "C" float mqemu_imgui_frame_height(bool with_spacing)
{
	return guarded(0.0f, [&] { return with_spacing ? ImGui::GetFrameHeightWithSpacing() : ImGui::GetFrameHeight(); });
}

extern "C" void mqemu_imgui_cursor_screen_pos(float* x, float* y)
{
	guarded_void([&] {
		ImVec2 pos = ImGui::GetCursorScreenPos();
		*x = pos.x;
		*y = pos.y;
	});
}

// Moving the cursor past the content grows the content to there, as ImGui did
// before 1.89. This ImGui instead throws when the window, group or table cell
// ends without an item after the move, leaving that scope half ended (and
// MacroQuest pausing ImGui for every plugin).
extern "C" void mqemu_imgui_set_cursor_screen_pos(float x, float y)
{
	guarded_void([&] {
		ImGui::SetCursorScreenPos(ImVec2(x, y));
		ImGuiWindow* window = ImGui::GetCurrentWindow();
		window->DC.CursorMaxPos = ImMax(window->DC.CursorMaxPos, window->DC.CursorPos);
		window->DC.IsSetPos = false;
	});
}

extern "C" void mqemu_imgui_window_pos(float* x, float* y)
{
	guarded_void([&] {
		ImVec2 pos = ImGui::GetWindowPos();
		*x = pos.x;
		*y = pos.y;
	});
}

extern "C" void mqemu_imgui_window_size(float* w, float* h)
{
	guarded_void([&] {
		ImVec2 size = ImGui::GetWindowSize();
		*w = size.x;
		*h = size.y;
	});
}

// The main viewport's work area (the game's window, less any menu bars).
extern "C" void mqemu_imgui_main_viewport_rect(float* x, float* y, float* w, float* h)
{
	guarded_void([&] {
		ImGuiViewport* viewport = ImGui::GetMainViewport();
		*x = viewport->WorkPos.x;
		*y = viewport->WorkPos.y;
		*w = viewport->WorkSize.x;
		*h = viewport->WorkSize.y;
	});
}

extern "C" void mqemu_imgui_mouse_pos(float* x, float* y)
{
	guarded_void([&] {
		ImVec2 pos = ImGui::GetMousePos();
		*x = pos.x;
		*y = pos.y;
	});
}

// How far the wheel turned this frame (notches; up and left positive), however
// it reached ImGui: MacroQuest adds DirectInput's to io.MouseWheel itself.
extern "C" void mqemu_imgui_mouse_wheel(float* x, float* y)
{
	guarded_void([&] {
		*x = ImGui::GetIO().MouseWheelH;
		*y = ImGui::GetIO().MouseWheel;
	});
}

// `button` 0 left, 1 right, 2 middle.
extern "C" bool mqemu_imgui_is_mouse_down(int button)
{
	return guarded(false, [&] { return ImGui::IsMouseDown(button); });
}

extern "C" bool mqemu_imgui_is_mouse_clicked(int button)
{
	return guarded(false, [&] { return ImGui::IsMouseClicked(button); });
}

extern "C" bool mqemu_imgui_is_mouse_hovering_rect(float min_x, float min_y, float max_x, float max_y)
{
	return guarded(false, [&] { return ImGui::IsMouseHoveringRect(ImVec2(min_x, min_y), ImVec2(max_x, max_y)); });
}

// ----- input events -----

// One of the input events ImGui took in this frame. `kind` is an
// ImGuiInputEventType: MousePos and MouseWheel set x and y; MouseButton sets
// code (0 left, 1 right, 2 middle, 3-4 extra) and down; Key sets code (an
// ImGuiKey) and down; Text sets code (a UTF-32 character); Focus sets down
// (whether the game's window has focus).
struct InputEvent
{
	int kind;
	int code;
	float x, y;
	bool down;
};

extern "C" int mqemu_imgui_input_event_count()
{
	return guarded(0, [] { return GImGui->InputEventsTrail.Size; });
}

// Event `index` (oldest first); false past the last.
extern "C" bool mqemu_imgui_input_event(int index, InputEvent* out)
{
	return guarded(false, [&] {
		const ImVector<ImGuiInputEvent>& events = GImGui->InputEventsTrail;
		if (index < 0 || index >= events.Size)
			return false;
		const ImGuiInputEvent& e = events[index];
		*out = { e.Type, 0, 0.0f, 0.0f, false };
		switch (e.Type)
		{
		case ImGuiInputEventType_MousePos: out->x = e.MousePos.PosX; out->y = e.MousePos.PosY; break;
		case ImGuiInputEventType_MouseWheel: out->x = e.MouseWheel.WheelX; out->y = e.MouseWheel.WheelY; break;
		case ImGuiInputEventType_MouseButton: out->code = e.MouseButton.Button; out->down = e.MouseButton.Down; break;
		case ImGuiInputEventType_Key: out->code = e.Key.Key; out->down = e.Key.Down; break;
		case ImGuiInputEventType_Text: out->code = (int)e.Text.Char; break;
		case ImGuiInputEventType_Focus: out->down = e.AppFocused.Focused; break;
		default: break;
		}
		return true;
	});
}

// `key` an ImGuiKey (a named key or an ImGuiMod_ one). Pressed this frame;
// with `repeat`, also each time it repeats while held, at ImGui's rate.
extern "C" bool mqemu_imgui_is_key_pressed(int key, bool repeat)
{
	return guarded(false, [&] { return ImGui::IsKeyPressed(static_cast<ImGuiKey>(key), repeat); });
}

extern "C" bool mqemu_imgui_is_key_released(int key)
{
	return guarded(false, [&] { return ImGui::IsKeyReleased(static_cast<ImGuiKey>(key)); });
}

extern "C" bool mqemu_imgui_is_key_down(int key)
{
	return guarded(false, [&] { return ImGui::IsKeyDown(static_cast<ImGuiKey>(key)); });
}

// The modifier keys held (ImGuiMod_* flags).
extern "C" int mqemu_imgui_key_mods()
{
	return guarded(0, [] { return (int)ImGui::GetIO().KeyMods; });
}

// `flags` ImGuiFocusedFlags.
extern "C" bool mqemu_imgui_is_window_focused(int flags)
{
	return guarded(false, [&] { return ImGui::IsWindowFocused(flags); });
}

// Whether ImGui keeps the keyboard (or mouse) from the game next frame,
// whatever its own widgets want.
extern "C" void mqemu_imgui_set_next_frame_want_capture_keyboard(bool want)
{
	guarded_void([&] { ImGui::SetNextFrameWantCaptureKeyboard(want); });
}

extern "C" void mqemu_imgui_set_next_frame_want_capture_mouse(bool want)
{
	guarded_void([&] { ImGui::SetNextFrameWantCaptureMouse(want); });
}

// The clipboard's text, NUL-terminated and cut to `cap`; returns its whole length.
extern "C" size_t mqemu_imgui_clipboard_text(char* out, size_t cap)
{
	return guarded<size_t>(0, [&] {
		const char* text = ImGui::GetClipboardText();
		std::string_view view = text ? text : "";
		if (cap)
		{
			size_t n = std::min(view.size(), cap - 1);
			memcpy(out, view.data(), n);
			out[n] = 0;
		}
		return view.size();
	});
}

extern "C" void mqemu_imgui_set_clipboard_text(const char* text, size_t len)
{
	guarded_void([&] { ImGui::SetClipboardText(CString(text, len)); });
}

// An item with an ID and size but nothing drawn: a hit area for custom
// drawing, or a drag source. Neither `w` nor `h` may be 0 (negative fills
// the space left but that much).
extern "C" bool mqemu_imgui_invisible_button(const char* id, size_t id_len, float w, float h, int flags)
{
	return guarded(false, [&] {
		if (w == 0.0f || h == 0.0f)
			throw ImGuiException("InvisibleButton() can't be 0 wide or tall");
		return ImGui::InvisibleButton(CString(id, id_len), ImVec2(w, h), flags);
	});
}

// ----- drag and drop -----

namespace {

// ImGui keeps payload types in a 33-byte buffer, and a release imgui.dll
// doesn't check the length.
bool payload_type_ok(const char* type, size_t len)
{
	return len > 0 && len <= 32 && type[0] != '_' && !memchr(type, 0, len);
}

void read_payload(const ImGuiPayload* payload, const void** data, size_t* size, bool* preview, bool* delivery)
{
	*data = payload->Data;
	*size = payload->Data ? static_cast<size_t>(payload->DataSize) : 0;
	*preview = payload->IsPreview();
	*delivery = payload->IsDelivery();
}

} // namespace

// Call after the item to drag. Call mqemu_imgui_end_drag_drop_source only if
// this returns true.
extern "C" bool mqemu_imgui_begin_drag_drop_source(int flags)
{
	return guarded(false, [&] { return ImGui::BeginDragDropSource(flags); });
}

// Copies `size` bytes of `data` as the payload, of `type`: 1 to 32 bytes, no
// NUL, not starting with '_' (ImGui's own types). False if a target hasn't
// accepted it yet. Only between a drag source's begin and end.
extern "C" bool mqemu_imgui_set_drag_drop_payload(const char* type, size_t type_len, const void* data, size_t size, int cond)
{
	return guarded(false, [&] {
		if (!payload_type_ok(type, type_len))
			throw ImGuiException("drag and drop payload types are 1 to 32 bytes and don't start with '_'");
		return ImGui::SetDragDropPayload(CString(type, type_len), size ? data : nullptr, size, cond);
	});
}

extern "C" void mqemu_imgui_end_drag_drop_source()
{
	guarded_void([] { ImGui::EndDragDropSource(); });
}

// Call after the item to drop on. Call mqemu_imgui_end_drag_drop_target only
// if this returns true.
extern "C" bool mqemu_imgui_begin_drag_drop_target()
{
	return guarded(false, [] { return ImGui::BeginDragDropTarget(); });
}

// A payload of `type` over the target: its data (owned by ImGui, valid this
// frame) and whether it's only hovering (`preview`) or dropped (`delivery`).
// Only between a drop target's begin and end.
extern "C" bool mqemu_imgui_accept_drag_drop_payload(const char* type, size_t type_len, int flags, const void** data,
	size_t* size, bool* preview, bool* delivery)
{
	return guarded(false, [&] {
		if (!payload_type_ok(type, type_len))
			return false;
		const ImGuiPayload* payload = ImGui::AcceptDragDropPayload(CString(type, type_len), flags);
		if (!payload)
			return false;
		read_payload(payload, data, size, preview, delivery);
		return true;
	});
}

extern "C" void mqemu_imgui_end_drag_drop_target()
{
	guarded_void([] { ImGui::EndDragDropTarget(); });
}

// The payload being dragged anywhere, if any, its type copied to `type`
// (NUL-terminated, truncated to `cap`).
extern "C" bool mqemu_imgui_drag_drop_payload(char* type, size_t cap, const void** data, size_t* size, bool* preview, bool* delivery)
{
	return guarded(false, [&] {
		const ImGuiPayload* payload = ImGui::GetDragDropPayload();
		if (!payload || payload->DataFrameCount == -1)
			return false;
		if (type && cap)
			strncpy_s(type, cap, payload->DataType, _TRUNCATE);
		read_payload(payload, data, size, preview, delivery);
		return true;
	});
}

// ----- fonts -----

// MacroQuest's fonts (src/imgui/ImGuiUtils.cpp): 0 its default (Roboto 16
// with icons), 1 its console font, 2 large text (Roboto 22 with icons). Null
// if absent.
extern "C" ImFont* mqemu_imgui_mq_font(int which)
{
	switch (which)
	{
	case 0: return mq::imgui::DefaultFont;
	case 1: return mq::imgui::ConsoleFont;
	case 2: return mq::imgui::LargeTextFont;
	default: return nullptr;
	}
}

// Null `font` keeps the current one; `size` 0 keeps the current size. Sizes
// are unscaled pixels, >= 0.
extern "C" void mqemu_imgui_push_font(ImFont* font, float size)
{
	guarded_void([&] {
		if (!(size >= 0.0f))
			throw ImGuiException("PushFont() needs a size of 0 or more");
		ImGui::PushFont(font, size);
	});
}

extern "C" void mqemu_imgui_pop_font()
{
	guarded_void([] { ImGui::PopFont(); });
}

extern "C" ImFont* mqemu_imgui_current_font()
{
	return guarded<ImFont*>(nullptr, [] { return ImGui::GetFont(); });
}

// The size `font` was added at, its natural size.
extern "C" float mqemu_imgui_font_own_size(const ImFont* font)
{
	return font->LegacySize;
}

// The current font size in pixels, after global scaling.
extern "C" float mqemu_imgui_font_size()
{
	return guarded(0.0f, [] { return ImGui::GetFontSize(); });
}

// The current font size before global scaling, as mqemu_imgui_push_font takes it.
extern "C" float mqemu_imgui_font_size_base()
{
	return guarded(0.0f, [] { return ImGui::GetStyle().FontSizeBase; });
}

// Adds a TrueType/OpenType font file to the font atlas at `size` pixels.
// Null if the file can't be read.
extern "C" ImFont* mqemu_imgui_add_font_file(const char* path, size_t path_len, float size)
{
	return guarded<ImFont*>(nullptr, [&] {
		ImFontConfig config;
		config.Flags |= ImFontFlags_NoLoadError;
		return ImGui::GetIO().Fonts->AddFontFromFileTTF(CString(path, path_len), size, &config);
	});
}

// Removes a font added with mqemu_imgui_add_font_file. Never while it's
// pushed, or drawing with it.
extern "C" void mqemu_imgui_remove_font(ImFont* font)
{
	guarded_void([&] {
		// When the game exits, MacroQuest may have destroyed ImGui (and its
		// fonts) before unloading plugins.
		if (ImGui::GetCurrentContext())
			ImGui::GetIO().Fonts->RemoveFont(font);
	});
}

// ----- docking -----

// Whether docking is on (MacroQuest's overlay setting).
extern "C" bool mqemu_imgui_docking_enabled()
{
	return guarded(false, [] { return (ImGui::GetIO().ConfigFlags & ImGuiConfigFlags_DockingEnable) != 0; });
}

// The ID `id` gets on the current ID stack, as a widget labeled `id` would.
extern "C" ImGuiID mqemu_imgui_get_id(const char* id, size_t id_len)
{
	return guarded<ImGuiID>(0, [&] { return ImGui::GetID(id, id + id_len); });
}

// A dock space in the current window, which windows can be docked into. `id`
// must not be 0. Returns `id`, or 0 if docking is off.
extern "C" ImGuiID mqemu_imgui_dock_space(ImGuiID id, float w, float h, int flags)
{
	return guarded<ImGuiID>(0, [&] {
		if (id == 0)
			throw ImGuiException("DockSpace() needs an ID other than 0");
		return ImGui::DockSpace(id, ImVec2(w, h), flags);
	});
}

// A dock space covering the main viewport (0 picks an ID).
extern "C" ImGuiID mqemu_imgui_dock_space_over_viewport(ImGuiID id, int flags)
{
	return guarded<ImGuiID>(0, [&] { return ImGui::DockSpaceOverViewport(id, nullptr, flags); });
}

extern "C" void mqemu_imgui_set_next_window_dock_id(ImGuiID id, int cond)
{
	guarded_void([&] { ImGui::SetNextWindowDockID(id, cond); });
}

extern "C" ImGuiID mqemu_imgui_window_dock_id()
{
	return guarded<ImGuiID>(0, [] { return ImGui::GetWindowDockID(); });
}

extern "C" bool mqemu_imgui_is_window_docked()
{
	return guarded(false, [] { return ImGui::IsWindowDocked(); });
}

// ----- docking: the dock builder -----
// ImGui's internal DockBuilder* functions (imgui_internal.h), which lay out
// dock nodes in code. To several of them node 0 means every node, which would
// undo every plugin's docking and MacroQuest's; here it's refused.

namespace {

ImGuiDockNode* dock_node(ImGuiID id)
{
	if (id == 0)
		throw ImGuiException("dock node 0 isn't a node");
	return ImGui::DockBuilderGetNode(id);
}

ImGuiDockNode* existing_dock_node(ImGuiID id)
{
	ImGuiDockNode* node = dock_node(id);
	if (!node)
		throw ImGuiException("no such dock node");
	return node;
}

// Refuses to change the tree `node` is in while a window being drawn is
// docked in it or hosts it: ImGui would free nodes that window still uses.
void check_tree_not_in_use(ImGuiDockNode* node)
{
	if (!node)
		return;
	ImGuiDockNode* root = ImGui::DockNodeGetRootNode(node);
	for (const ImGuiWindowStackData& entry : GImGui->CurrentWindowStack)
	{
		for (ImGuiDockNode* used : { entry.Window->DockNode, entry.Window->DockNodeAsHost })
		{
			if (used && ImGui::DockNodeGetRootNode(used) == root)
				throw ImGuiException("can't change dock nodes from inside a window docked in them");
		}
	}
}

// Removes `dst` ahead of copying `src` over it, as ImGui would first thing,
// and checks that didn't take `src` with it (an ancestor, or a sibling merged
// away), which ImGui would then dereference.
void remove_copy_destination(ImGuiID src, ImGuiID dst)
{
	if (src == dst)
		throw ImGuiException("can't copy a dock node onto itself");
	check_tree_not_in_use(existing_dock_node(src));
	check_tree_not_in_use(dock_node(dst));
	ImGui::DockBuilderRemoveNode(dst);
	if (!ImGui::DockBuilderGetNode(src))
		throw ImGuiException("removing the destination dock node removed the source node too");
}

} // namespace

// A new node (replacing any node `id`; 0 picks an ID), or with `dock_space`
// a dock space node (`id` must not be 0). `flags` mustn't include DockSpace or
// CentralNode, which ImGui sets. Returns its ID, or 0 for a dock space when
// docking is off.
extern "C" ImGuiID mqemu_imgui_dock_builder_add_node(ImGuiID id, int flags, bool dock_space)
{
	return guarded<ImGuiID>(0, [&]() -> ImGuiID {
		if (flags & (ImGuiDockNodeFlags_DockSpace | ImGuiDockNodeFlags_CentralNode))
			throw ImGuiException("DockSpace and CentralNode aren't flags to pass to DockBuilderAddNode() here");
		if (dock_space)
		{
			if (id == 0)
				throw ImGuiException("a dock space node needs an ID other than 0");
			// DockSpace() does nothing then, and ImGui would dereference the node it didn't make.
			if (!(ImGui::GetIO().ConfigFlags & ImGuiConfigFlags_DockingEnable))
				return 0;
		}
		if (id != 0)
			check_tree_not_in_use(ImGui::DockBuilderGetNode(id));
		return ImGui::DockBuilderAddNode(id, dock_space ? flags | ImGuiDockNodeFlags_DockSpace : flags);
	});
}

// Removes the node and the nodes in it, undocking their windows.
extern "C" void mqemu_imgui_dock_builder_remove_node(ImGuiID id)
{
	guarded_void([&] {
		check_tree_not_in_use(dock_node(id));
		ImGui::DockBuilderRemoveNode(id);
	});
}

// Removes the nodes split from root node `id`, moving their windows into it.
extern "C" void mqemu_imgui_dock_builder_remove_node_child_nodes(ImGuiID id)
{
	guarded_void([&] {
		check_tree_not_in_use(dock_node(id));
		ImGui::DockBuilderRemoveNodeChildNodes(id);
	});
}

// Undocks the windows of root node `id` and the nodes in it; with
// `clear_settings_refs`, they also forget they were docked there.
extern "C" void mqemu_imgui_dock_builder_remove_node_docked_windows(ImGuiID id, bool clear_settings_refs)
{
	guarded_void([&] {
		check_tree_not_in_use(dock_node(id));
		ImGui::DockBuilderRemoveNodeDockedWindows(id, clear_settings_refs);
	});
}

// For floating nodes; nothing if there's no such node.
extern "C" void mqemu_imgui_dock_builder_set_node_pos(ImGuiID id, float x, float y)
{
	guarded_void([&] {
		dock_node(id);
		ImGui::DockBuilderSetNodePos(id, ImVec2(x, y));
	});
}

// Both sizes must be > 0. Set it before splitting a new node.
extern "C" void mqemu_imgui_dock_builder_set_node_size(ImGuiID id, float w, float h)
{
	guarded_void([&] {
		if (!(w > 0.0f && h > 0.0f))
			throw ImGuiException("a dock node's size must be more than 0");
		dock_node(id);
		ImGui::DockBuilderSetNodeSize(id, ImVec2(w, h));
	});
}

// Splits leaf node `id` in two: `ratio` (0 to 1) of it towards `dir` (an
// ImGuiDir, not None), the rest opposite. Writes both new nodes' IDs and
// returns the one towards `dir`; 0 on an error.
extern "C" ImGuiID mqemu_imgui_dock_builder_split_node(ImGuiID id, int dir, float ratio, ImGuiID* at_dir, ImGuiID* opposite)
{
	return guarded<ImGuiID>(0, [&] {
		if (dir < ImGuiDir_Left || dir > ImGuiDir_Down)
			throw ImGuiException("a dock node splits Left, Right, Up or Down");
		ImGuiDockNode* node = existing_dock_node(id);
		if (node->IsSplitNode())
			throw ImGuiException("the dock node is already split; split one of its children");
		check_tree_not_in_use(node);
		return ImGui::DockBuilderSplitNode(id, static_cast<ImGuiDir>(dir), ratio, at_dir, opposite);
	});
}

// Docks the window called `name` (drawn yet or not) into node `id`, from its
// next Begin; 0 undocks it.
extern "C" void mqemu_imgui_dock_builder_dock_window(const char* name, size_t name_len, ImGuiID id)
{
	guarded_void([&] {
		CString window_name(name, name_len);
		// Moving a window out of its node can free the node.
		if (ImGuiWindow* window = ImGui::FindWindowByName(window_name))
			check_tree_not_in_use(window->DockNode);
		ImGui::DockBuilderDockWindow(window_name, id);
	});
}

// Gives the window `dst` the position, size and collapsed state of `src`.
extern "C" void mqemu_imgui_dock_builder_copy_window_settings(const char* src, size_t src_len, const char* dst, size_t dst_len)
{
	guarded_void([&] { ImGui::DockBuilderCopyWindowSettings(CString(src, src_len), CString(dst, dst_len)); });
}

// Replaces node `dst` with a copy of node `src` and the nodes in it. Writes up
// to `cap` (source, copy) ID pairs to `pairs` (2 IDs each) and returns how
// many there are; -1 on an error.
extern "C" int mqemu_imgui_dock_builder_copy_node(ImGuiID src, ImGuiID dst, ImGuiID* pairs, int cap)
{
	return guarded(-1, [&] {
		remove_copy_destination(src, dst);
		ImVector<ImGuiID> remap;
		ImGui::DockBuilderCopyNode(src, dst, &remap);
		int count = remap.Size / 2;
		for (int i = 0; i < count && i < cap; ++i)
		{
			pairs[i * 2] = remap[i * 2];
			pairs[i * 2 + 1] = remap[i * 2 + 1];
		}
		return count;
	});
}

// Copies dock space `src`'s layout to dock space `dst`. `windows` names
// `pair_count` (source window, destination window) pairs, as 2 strings each:
// the destination windows go where the source ones are.
extern "C" void mqemu_imgui_dock_builder_copy_dock_space(ImGuiID src, ImGuiID dst, const char* const* windows,
	const size_t* window_lens, int pair_count)
{
	guarded_void([&] {
		std::vector<std::string> names;
		names.reserve(pair_count * 2);
		for (int i = 0; i < pair_count * 2; ++i)
			names.emplace_back(windows[i], window_lens[i]);
		ImVector<const char*> remap;
		for (const std::string& name : names)
			remap.push_back(name.c_str());
		remove_copy_destination(src, dst);
		ImGui::DockBuilderCopyDockSpace(src, dst, &remap);
	});
}

// Applies the layout built under root node `id` to the windows docked there.
extern "C" void mqemu_imgui_dock_builder_finish(ImGuiID id)
{
	guarded_void([&] {
		dock_node(id);
		ImGui::DockBuilderFinish(id);
	});
}

// What a dock node is (mqemu_imgui_dock_node).
struct mqemu_dock_node_info
{
	ImGuiID parent;
	ImGuiID children[2];
	// The central node of its tree (the part a dock space keeps for itself).
	ImGuiID central;
	float x, y, w, h;
	// Its own and inherited flags.
	int flags;
	int windows;
	bool dock_space;
	bool central_node;
	bool floating;
	bool visible;
};

// Describes node `id` into `out`; false if there's no such node.
extern "C" bool mqemu_imgui_dock_node(ImGuiID id, mqemu_dock_node_info* out)
{
	return guarded(false, [&] {
		ImGuiDockNode* node = dock_node(id);
		if (!node)
			return false;
		ImGuiDockNode* central = ImGui::DockNodeGetRootNode(node)->CentralNode;
		out->parent = node->ParentNode ? node->ParentNode->ID : 0;
		for (int i = 0; i < 2; ++i)
			out->children[i] = node->ChildNodes[i] ? node->ChildNodes[i]->ID : 0;
		out->central = central ? central->ID : 0;
		out->x = node->Pos.x;
		out->y = node->Pos.y;
		out->w = node->Size.x;
		out->h = node->Size.y;
		out->flags = node->MergedFlags;
		out->windows = node->Windows.Size;
		out->dock_space = node->IsDockSpace();
		out->central_node = node->IsCentralNode();
		out->floating = node->IsFloatingNode();
		out->visible = node->IsVisible;
		return true;
	});
}

// The title of window `index` docked in node `id`, copied to `out`
// (NUL-terminated, truncated to `cap`); false if there's none.
extern "C" bool mqemu_imgui_dock_node_window(ImGuiID id, int index, char* out, size_t cap)
{
	return guarded(false, [&] {
		ImGuiDockNode* node = dock_node(id);
		if (!node || index < 0 || index >= node->Windows.Size)
			return false;
		if (out && cap)
			strncpy_s(out, cap, node->Windows[index]->Name, _TRUNCATE);
		return true;
	});
}

// ----- multi-select -----
// Items in a BeginMultiSelect()/EndMultiSelect() scope (selectables, tree
// nodes) each get an index with SetNextItemSelectionUserData, and ImGui hands
// back requests to apply to the caller's selection.

// Begins a scope; always end it. Returns the requests to apply now (read with
// mqemu_imgui_multi_select_io, before anything else begins one), or null on
// an error (then don't end it). -1 for an unknown size or count.
extern "C" ImGuiMultiSelectIO* mqemu_imgui_begin_multi_select(int flags, int selection_size, int items_count)
{
	return guarded<ImGuiMultiSelectIO*>(nullptr, [&] { return ImGui::BeginMultiSelect(flags, selection_size, items_count); });
}

// Ends the scope, returning the requests to apply afterwards (null on an
// error), valid until a scope next begins.
extern "C" ImGuiMultiSelectIO* mqemu_imgui_end_multi_select()
{
	return guarded<ImGuiMultiSelectIO*>(nullptr, [] { return ImGui::EndMultiSelect(); });
}

// The IO's fields; returns its number of requests. Items are -1 for none.
extern "C" int mqemu_imgui_multi_select_io(const ImGuiMultiSelectIO* io, int64_t* range_src_item, int64_t* nav_id_item,
	bool* nav_id_selected, int* items_count)
{
	*range_src_item = io->RangeSrcItem;
	*nav_id_item = io->NavIdItem;
	*nav_id_selected = io->NavIdSelected;
	*items_count = io->ItemsCount;
	return io->Requests.Size;
}

// Request `index` of the IO: an ImGuiSelectionRequestType, and for SetRange
// its first and last items (inclusive) and direction (+1 or -1).
extern "C" void mqemu_imgui_multi_select_request(const ImGuiMultiSelectIO* io, int index, int* type, bool* selected,
	int* direction, int64_t* first, int64_t* last)
{
	const ImGuiSelectionRequest& request = io->Requests[index];
	*type = request.Type;
	*selected = request.Selected;
	*direction = request.RangeDirection;
	*first = request.RangeFirstItem;
	*last = request.RangeLastItem;
}

// In the current scope: forget the item shift-clicks extend from (when it was
// deleted, say).
extern "C" void mqemu_imgui_multi_select_reset_range_src()
{
	guarded_void([] {
		if (!GImGui->CurrentMultiSelect)
			throw ImGuiException("not in a multi-select scope");
		GImGui->CurrentMultiSelect->IO.RangeSrcReset = true;
	});
}

extern "C" void mqemu_imgui_set_next_item_selection_user_data(int64_t data)
{
	guarded_void([&] { ImGui::SetNextItemSelectionUserData(data); });
}

// Whether the last item's selection was toggled this frame. Only in a scope.
extern "C" bool mqemu_imgui_is_item_toggled_selection()
{
	return guarded(false, [] {
		if (!GImGui->CurrentMultiSelect)
			throw ImGuiException("IsItemToggledSelection() is only for inside a multi-select scope");
		return ImGui::IsItemToggledSelection();
	});
}

// ----- list clipper -----
// ImGuiListClipper, on the heap so Rust can hold it. Each step gives a range
// of items to draw; the rest of a long list is skipped by moving the cursor.
// A release imgui.dll doesn't check its asserts, so the misuse they catch
// (stepping an ended clipper, including items after stepping) is refused here.

// Begins a clipper over `items_count` items, `items_height` apart (0 or less
// to measure the first item). Always end it, unless this returns null (an
// error). Clippers must end in the reverse order they began, in the window
// they began in.
extern "C" ImGuiListClipper* mqemu_imgui_list_clipper_begin(int items_count, float items_height, int flags)
{
	return guarded<ImGuiListClipper*>(nullptr, [&] {
		if (!GImGui->CurrentWindow)
			throw ImGuiException("a list clipper needs a current window");
		if (items_count < 0 || items_count == INT_MAX)
			throw ImGuiException("a list clipper's item count must be 0 or more, below INT_MAX");
		ImGuiListClipper* clipper = new ImGuiListClipper();
		clipper->Flags = flags;
		clipper->Begin(items_count, items_height > 0.0f ? items_height : -1.0f);
		return clipper;
	});
}

// Draws items [item_begin, item_end) whether they're visible or not. Only
// before the first step.
extern "C" void mqemu_imgui_list_clipper_include(ImGuiListClipper* clipper, int item_begin, int item_end)
{
	guarded_void([&] {
		if (!clipper->TempData || clipper->DisplayStart >= 0)
			throw ImGuiException("items can only be included before a list clipper's first step");
		if (item_begin < 0 || item_begin > item_end)
			throw ImGuiException("a list clipper's included items must be a range of indices");
		clipper->IncludeItemsByIndex(item_begin, item_end);
	});
}

// The next range of items to draw, [*display_start, *display_end); false once
// there are no more (the clipper has then moved the cursor past the list).
// False from then on, rather than ImGui's crash.
extern "C" bool mqemu_imgui_list_clipper_step(ImGuiListClipper* clipper, int* display_start, int* display_end)
{
	*display_start = 0;
	*display_end = 0;
	return guarded(false, [&] {
		const ImGuiListClipperData* data = static_cast<const ImGuiListClipperData*>(clipper->TempData);
		if (!data)
			return false;
		// This step measures the first item, which ImGui only asserts moved the cursor.
		bool measuring = data->StepNo == 1 && clipper->ItemsHeight <= 0.0f;
		bool more = clipper->Step();
		if (measuring && !(clipper->ItemsHeight > 0.0f))
		{
			clipper->End();
			throw ImGuiException("a list clipper's first item didn't move the cursor down: draw something for every item");
		}
		*display_start = clipper->DisplayStart;
		*display_end = clipper->DisplayEnd;
		return more;
	});
}

// Ends the clipper (moving the cursor past the list) and frees it.
extern "C" void mqemu_imgui_list_clipper_end(ImGuiListClipper* clipper)
{
	guarded_void([&] { delete clipper; });
}

// ----- custom widgets (imgui_internal.h) -----
// What ImGui's own widgets are made of: an item laid out and added with an
// ID, its button behavior, splitters, and the helpers that draw frames, text
// and arrows in the style's colors. Rectangles are in screen coordinates.

static ImRect rect(float x0, float y0, float x1, float y1)
{
	return ImRect(x0, y0, x1, y1);
}

static void check_dir(int dir)
{
	if (dir < ImGuiDir_Left || dir > ImGuiDir_Down)
		throw ImGuiException("not a direction (ImGuiDir_Left to ImGuiDir_Down)");
}

// Lays out an item of `w` x `h` at the cursor and adds it with `id` (0 for
// one nothing interacts with). Writes its rectangle; false if it's clipped
// (it still takes its space).
extern "C" bool mqemu_imgui_item_add(float w, float h, ImGuiID id, float* x0, float* y0, float* x1, float* y1)
{
	return guarded(false, [&] {
		ImGuiWindow* window = ImGui::GetCurrentWindow();
		ImRect bb(window->DC.CursorPos, window->DC.CursorPos + ImVec2(w, h));
		*x0 = bb.Min.x;
		*y0 = bb.Min.y;
		*x1 = bb.Max.x;
		*y1 = bb.Max.y;
		ImGui::ItemSize(bb.GetSize());
		return ImGui::ItemAdd(bb, id);
	});
}

// ImGui::ButtonBehavior: whether the item at the rectangle was pressed (as
// `flags`, ImGuiButtonFlags_ and the private ones, say), and whether it's
// hovered and held. `id` must not be 0.
extern "C" bool mqemu_imgui_button_behavior(float x0, float y0, float x1, float y1, ImGuiID id, int flags, bool* hovered, bool* held)
{
	*hovered = false;
	*held = false;
	return guarded(false, [&] {
		if (id == 0)
			throw ImGuiException("ButtonBehavior() needs an item ID other than 0");
		return ImGui::ButtonBehavior(rect(x0, y0, x1, y1), id, hovered, held, flags);
	});
}

// The item being interacted with, under the mouse and with keyboard focus (0 for none).
extern "C" void mqemu_imgui_item_ids(ImGuiID* active, ImGuiID* hovered, ImGuiID* focus)
{
	*active = GImGui->ActiveId;
	*hovered = GImGui->HoveredId;
	*focus = GImGui->NavId;
}

// Lets go of the active item (ends a drag, say).
extern "C" void mqemu_imgui_clear_active_id()
{
	guarded_void([] { ImGui::ClearActiveID(); });
}

// Keeps `id` active or hovered on a frame its item isn't drawn.
extern "C" void mqemu_imgui_keep_alive_id(ImGuiID id)
{
	guarded_void([&] { ImGui::KeepAliveID(id); });
}

// Marks the item's value as edited, for IsItemEdited and IsItemDeactivatedAfterEdit.
extern "C" void mqemu_imgui_mark_item_edited(ImGuiID id)
{
	guarded_void([&] { ImGui::MarkItemEdited(id); });
}

// ImGui::SplitterBehavior: a bar at the rectangle that, dragged along `axis`
// (ImGuiAxis_X or ImGuiAxis_Y), moves size from one side to the other, never
// below the minimums. True while it's dragged.
extern "C" bool mqemu_imgui_splitter(float x0, float y0, float x1, float y1, ImGuiID id, int axis, float* size1, float* size2,
	float min_size1, float min_size2, float hover_extend, float hover_visibility_delay)
{
	return guarded(false, [&] {
		if (id == 0)
			throw ImGuiException("a splitter needs an item ID other than 0");
		if (axis != ImGuiAxis_X && axis != ImGuiAxis_Y)
			throw ImGuiException("a splitter's axis is ImGuiAxis_X or ImGuiAxis_Y");
		return ImGui::SplitterBehavior(rect(x0, y0, x1, y1), id, static_cast<ImGuiAxis>(axis), size1, size2, min_size1, min_size2,
			hover_extend, hover_visibility_delay);
	});
}

// A frame in the current window, as widgets draw theirs: `fill`, the style's
// border if `borders` and it has one.
extern "C" void mqemu_imgui_render_frame(float x0, float y0, float x1, float y1, ImU32 fill, bool borders, float rounding)
{
	guarded_void([&] { ImGui::RenderFrame(ImVec2(x0, y0), ImVec2(x1, y1), fill, borders, rounding); });
}

extern "C" void mqemu_imgui_render_frame_border(float x0, float y0, float x1, float y1, float rounding)
{
	guarded_void([&] { ImGui::RenderFrameBorder(ImVec2(x0, y0), ImVec2(x1, y1), rounding); });
}

// The keyboard navigation cursor around item `id`, when it has focus.
extern "C" void mqemu_imgui_render_nav_cursor(float x0, float y0, float x1, float y1, ImGuiID id)
{
	guarded_void([&] { ImGui::RenderNavCursor(rect(x0, y0, x1, y1), id); });
}

// Text in the style's text color, clipped to the rectangle and aligned in it
// (0 to 1 on each axis). Stops at a ## as labels do.
extern "C" void mqemu_imgui_render_text_clipped(float x0, float y0, float x1, float y1, const char* text, size_t len, float align_x, float align_y)
{
	guarded_void([&] { ImGui::RenderTextClipped(ImVec2(x0, y0), ImVec2(x1, y1), text, text + len, nullptr, ImVec2(align_x, align_y)); });
}

// A triangle pointing `dir` (ImGuiDir), a font size wide times `scale`, its
// top left at (x, y).
extern "C" void mqemu_imgui_render_arrow(ImDrawList* list, float x, float y, ImU32 col, int dir, float scale)
{
	guarded_void([&] {
		check_dir(dir);
		ImGui::RenderArrow(list, ImVec2(x, y), col, static_cast<ImGuiDir>(dir), scale);
	});
}

// A bullet centered at (x, y).
extern "C" void mqemu_imgui_render_bullet(ImDrawList* list, float x, float y, ImU32 col)
{
	guarded_void([&] { ImGui::RenderBullet(list, ImVec2(x, y), col); });
}

// A check mark `size` wide, its top left at (x, y).
extern "C" void mqemu_imgui_render_check_mark(ImDrawList* list, float x, float y, ImU32 col, float size)
{
	guarded_void([&] { ImGui::RenderCheckMark(list, ImVec2(x, y), col, size); });
}

// A button with ImGuiButtonFlags_ (and the private ones).
extern "C" bool mqemu_imgui_button_ex(const char* label, size_t label_len, float w, float h, int flags)
{
	return guarded(false, [&] { return ImGui::ButtonEx(CString(label, label_len), ImVec2(w, h), flags); });
}

// A square button with an arrow pointing `dir` (ImGuiDir), `w` x `h`.
extern "C" bool mqemu_imgui_arrow_button_ex(const char* id, size_t id_len, int dir, float w, float h, int flags)
{
	return guarded(false, [&] {
		check_dir(dir);
		return ImGui::ArrowButtonEx(CString(id, id_len), static_cast<ImGuiDir>(dir), ImVec2(w, h), flags);
	});
}

// A separator across (or with `vertical`, down) the line, `thickness` pixels.
extern "C" void mqemu_imgui_separator_ex(bool vertical, float thickness)
{
	guarded_void([&] {
		if (!(thickness > 0.0f))
			throw ImGuiException("a separator's thickness must be more than 0");
		ImGui::SeparatorEx(vertical ? ImGuiSeparatorFlags_Vertical : ImGuiSeparatorFlags_Horizontal, thickness);
	});
}

// ----- windows and scrolling (imgui_internal.h) -----
// Any window ImGui has, this plugin's or not, found by name or ID: what it
// is, its focus and display order, and moving, sizing, focusing and
// scrolling it. Windows are passed by ID and found again on each call, so no
// ImGuiWindow pointer leaves the shim.

static ImGuiWindow* existing_window(ImGuiID id)
{
	ImGuiWindow* window = id ? ImGui::FindWindowByID(id) : nullptr;
	if (!window)
		throw ImGuiException("no such window");
	return window;
}

static void check_cond(int cond)
{
	if (cond != 0 && !ImIsPowerOfTwo(cond))
		throw ImGuiException("one ImGuiCond_ at a time");
}

static void check_ratio(float ratio)
{
	if (!(ratio >= 0.0f && ratio <= 1.0f))
		throw ImGuiException("a scroll ratio is 0 to 1");
}

static void check_scroll_flags(int flags)
{
	int x = flags & ImGuiScrollFlags_MaskX_;
	int y = flags & ImGuiScrollFlags_MaskY_;
	if ((x && !ImIsPowerOfTwo(x)) || (y && !ImIsPowerOfTwo(y)))
		throw ImGuiException("one ImGuiScrollFlags_ behavior per axis");
}

static void check_axis(int axis)
{
	if (axis != ImGuiAxis_X && axis != ImGuiAxis_Y)
		throw ImGuiException("an axis is ImGuiAxis_X or ImGuiAxis_Y");
}

// A window's ID: the hash of its name, as ImGui finds windows by.
extern "C" ImGuiID mqemu_imgui_window_id(const char* name, size_t len)
{
	return guarded<ImGuiID>(0, [&] { return ImHashStr(name, len); });
}

// The window named `name` (its ID), or 0 if ImGui has none.
extern "C" ImGuiID mqemu_imgui_find_window(const char* name, size_t len)
{
	return guarded<ImGuiID>(0, [&] {
		ImGuiWindow* window = ImGui::FindWindowByName(CString(name, len));
		return window ? window->ID : 0;
	});
}

// The current window, the focused one and the one under the mouse (0 for none).
extern "C" void mqemu_imgui_window_ids(ImGuiID* current, ImGuiID* focused, ImGuiID* hovered)
{
	*current = GImGui->CurrentWindow ? GImGui->CurrentWindow->ID : 0;
	*focused = GImGui->NavWindow ? GImGui->NavWindow->ID : 0;
	*hovered = GImGui->HoveredWindow ? GImGui->HoveredWindow->ID : 0;
}

// How many windows ImGui has, and the one at `index` in display order (0 at
// the back; 0 past the end).
extern "C" int mqemu_imgui_window_count()
{
	return GImGui->Windows.Size;
}

extern "C" ImGuiID mqemu_imgui_window_at(int index)
{
	return index >= 0 && index < GImGui->Windows.Size ? GImGui->Windows[index]->ID : 0;
}

// What a window is (mqemu_imgui_window).
struct mqemu_window_info
{
	ImGuiID parent;
	// The window drawn in front with it: itself, or for a child or docked
	// window the top-level window it's in.
	ImGuiID root;
	ImGuiID dock_id;
	float x, y, w, h;
	float content_w, content_h;
	float scroll_x, scroll_y;
	float scroll_max_x, scroll_max_y;
	int flags;
	int display_index;
	// -1 for windows not in the focus order (child windows).
	int focus_order;
	int last_frame_drawn;
	bool collapsed;
	bool hidden;
};

// Describes window `id` into `out` and copies its name to `name`
// (NUL-terminated, truncated to `cap`); false if there's no such window.
extern "C" bool mqemu_imgui_window(ImGuiID id, mqemu_window_info* out, char* name, size_t cap)
{
	return guarded(false, [&] {
		ImGuiWindow* window = id ? ImGui::FindWindowByID(id) : nullptr;
		if (!window)
			return false;
		out->parent = window->ParentWindow ? window->ParentWindow->ID : 0;
		out->root = window->RootWindowDockTree ? window->RootWindowDockTree->ID : window->ID;
		out->dock_id = window->DockId;
		out->x = window->Pos.x;
		out->y = window->Pos.y;
		out->w = window->Size.x;
		out->h = window->Size.y;
		out->content_w = window->ContentSize.x;
		out->content_h = window->ContentSize.y;
		out->scroll_x = window->Scroll.x;
		out->scroll_y = window->Scroll.y;
		out->scroll_max_x = window->ScrollMax.x;
		out->scroll_max_y = window->ScrollMax.y;
		out->flags = window->Flags;
		out->display_index = ImGui::FindWindowDisplayIndex(window);
		// Child windows are never in the list, but keep FocusOrder's default of 0.
		int order = window->FocusOrder;
		ImVector<ImGuiWindow*>& focus = GImGui->WindowsFocusOrder;
		out->focus_order = order >= 0 && order < focus.Size && focus[order] == window ? order : -1;
		out->last_frame_drawn = window->LastFrameActive;
		out->collapsed = window->Collapsed;
		out->hidden = window->Hidden;
		if (name && cap)
			strncpy_s(name, cap, window->Name, _TRUNCATE);
		return true;
	});
}

extern "C" void mqemu_imgui_set_window_pos_by_id(ImGuiID id, float x, float y, int cond)
{
	guarded_void([&] {
		check_cond(cond);
		ImGui::SetWindowPos(existing_window(id), ImVec2(x, y), cond);
	});
}

extern "C" void mqemu_imgui_set_window_size_by_id(ImGuiID id, float w, float h, int cond)
{
	guarded_void([&] {
		check_cond(cond);
		ImGui::SetWindowSize(existing_window(id), ImVec2(w, h), cond);
	});
}

extern "C" void mqemu_imgui_set_window_collapsed_by_id(ImGuiID id, bool collapsed, int cond)
{
	guarded_void([&] {
		check_cond(cond);
		ImGui::SetWindowCollapsed(existing_window(id), collapsed, cond);
	});
}

// Focuses window `id` (ImGuiFocusRequestFlags_), bringing it to the front
// unless it has NoBringToFrontOnFocus; 0 takes focus from every window.
extern "C" void mqemu_imgui_focus_window(ImGuiID id, int flags)
{
	guarded_void([&] { ImGui::FocusWindow(id ? existing_window(id) : nullptr, flags); });
}

// Moves the window `id` is drawn with (its root) to the front or back of the
// display order, or just behind the one `behind` is drawn with.
extern "C" void mqemu_imgui_bring_window_to_front(ImGuiID id)
{
	guarded_void([&] { ImGui::BringWindowToDisplayFront(existing_window(id)->RootWindowDockTree); });
}

extern "C" void mqemu_imgui_bring_window_to_back(ImGuiID id)
{
	guarded_void([&] { ImGui::BringWindowToDisplayBack(existing_window(id)->RootWindowDockTree); });
}

extern "C" void mqemu_imgui_bring_window_behind(ImGuiID id, ImGuiID behind)
{
	guarded_void([&] {
		ImGuiWindow* window = existing_window(id)->RootWindowDockTree;
		ImGuiWindow* above = existing_window(behind)->RootWindowDockTree;
		if (window != above)
			ImGui::BringWindowToDisplayBehind(window, above);
	});
}

// Whether window `above` is drawn over window `below`.
extern "C" bool mqemu_imgui_is_window_above(ImGuiID above, ImGuiID below)
{
	return guarded(false, [&] { return ImGui::IsWindowAbove(existing_window(above), existing_window(below)); });
}

// Scrolls window `id` to `scroll` on `axis` (ImGuiAxis), from next frame.
extern "C" void mqemu_imgui_set_window_scroll(ImGuiID id, int axis, float scroll)
{
	guarded_void([&] {
		check_axis(axis);
		ImGuiWindow* window = existing_window(id);
		if (axis == ImGuiAxis_X)
			ImGui::SetScrollX(window, scroll);
		else
			ImGui::SetScrollY(window, scroll);
	});
}

// Scrolls window `id` so `local` (from its top left, as the cursor's
// position in it is) ends up `ratio` (0 to 1) of the way across its view.
extern "C" void mqemu_imgui_set_window_scroll_from_pos(ImGuiID id, int axis, float local, float ratio)
{
	guarded_void([&] {
		check_axis(axis);
		check_ratio(ratio);
		ImGuiWindow* window = existing_window(id);
		if (axis == ImGuiAxis_X)
			ImGui::SetScrollFromPosX(window, local, ratio);
		else
			ImGui::SetScrollFromPosY(window, local, ratio);
	});
}

// Scrolls the current window so the cursor ends up `ratio` (0 to 1) of the
// way across its view.
extern "C" void mqemu_imgui_set_scroll_here(int axis, float ratio)
{
	guarded_void([&] {
		check_axis(axis);
		check_ratio(ratio);
		if (axis == ImGuiAxis_X)
			ImGui::SetScrollHereX(ratio);
		else
			ImGui::SetScrollHereY(ratio);
	});
}

// Scrolls the current window (and its parents, unless NoScrollParent) to
// bring the last item into view, as `flags` (ImGuiScrollFlags_) say.
extern "C" void mqemu_imgui_scroll_to_item(int flags)
{
	guarded_void([&] {
		check_scroll_flags(flags);
		ImGui::ScrollToItem(flags);
	});
}

// Scrolls window `id` to bring the rectangle (screen coordinates) into view;
// writes how far it will scroll.
extern "C" void mqemu_imgui_scroll_to_rect(ImGuiID id, float x0, float y0, float x1, float y1, int flags, float* dx, float* dy)
{
	*dx = 0.0f;
	*dy = 0.0f;
	guarded_void([&] {
		check_scroll_flags(flags);
		ImVec2 delta = ImGui::ScrollToRectEx(existing_window(id), ImRect(x0, y0, x1, y1), flags);
		*dx = delta.x;
		*dy = delta.y;
	});
}
