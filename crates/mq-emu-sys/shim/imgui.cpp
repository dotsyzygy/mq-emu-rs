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
