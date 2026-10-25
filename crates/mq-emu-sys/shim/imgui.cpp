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
// This file imports imgui.dll, so only binaries that call it link it.

#include "mq_api.h"

#include "imgui/imgui.h"

#include <cstring>
#include <exception>
#include <string>

namespace {

// The last ImGui error caught, until taken. Each plugin links its own copy.
std::string s_error;
bool s_has_error = false;

void note_error(const char* what)
{
	// Keep the first one: later errors are usually its consequences.
	if (!s_has_error)
	{
		s_error = what ? what : "unknown C++ exception";
		s_has_error = true;
	}
}

template <typename R, typename F>
R guarded(R fallback, F&& body)
{
	try
	{
		return body();
	}
	catch (const std::exception& e)
	{
		note_error(e.what());
	}
	catch (...)
	{
		note_error(nullptr);
	}
	return fallback;
}

template <typename F>
void guarded_void(F&& body)
{
	guarded(0, [&] { body(); return 0; });
}

// A (pointer, length) string as the NUL-terminated one ImGui wants. Short
// strings (nearly every label) stay on the stack.
class CString
{
public:
	CString(const char* text, size_t len)
	{
		if (len < sizeof(m_small))
		{
			if (len)
				memcpy(m_small, text, len);
			m_small[len] = 0;
			m_ptr = m_small;
		}
		else
		{
			m_big.assign(text, len);
			m_ptr = m_big.c_str();
		}
	}

	CString(const CString&) = delete;
	CString& operator=(const CString&) = delete;

	operator const char*() const { return m_ptr; }

private:
	char m_small[128];
	std::string m_big;
	const char* m_ptr;
};

// Null for an absent optional string (len == SIZE_MAX).
class OptionalCString
{
public:
	OptionalCString(const char* text, size_t len)
	{
		if (text && len != SIZE_MAX)
		{
			m_text.assign(text, len);
			m_present = true;
		}
	}

	operator const char*() const { return m_present ? m_text.c_str() : nullptr; }

private:
	std::string m_text;
	bool m_present = false;
};

} // namespace

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

// Drawing on the current window's draw list, in screen coordinates. These
// don't move the cursor or affect layout.

extern "C" void mqemu_imgui_draw_line(float x1, float y1, float x2, float y2, float r, float g, float b, float a, float thickness)
{
	guarded_void([&] {
		ImGui::GetWindowDrawList()->AddLine(ImVec2(x1, y1), ImVec2(x2, y2), ImGui::ColorConvertFloat4ToU32(ImVec4(r, g, b, a)), thickness);
	});
}

extern "C" void mqemu_imgui_draw_rect(float x1, float y1, float x2, float y2, float r, float g, float b, float a,
	float rounding, float thickness, bool filled)
{
	guarded_void([&] {
		ImU32 color = ImGui::ColorConvertFloat4ToU32(ImVec4(r, g, b, a));
		ImDrawList* list = ImGui::GetWindowDrawList();
		if (filled)
			list->AddRectFilled(ImVec2(x1, y1), ImVec2(x2, y2), color, rounding);
		else
			list->AddRect(ImVec2(x1, y1), ImVec2(x2, y2), color, rounding, 0, thickness);
	});
}

extern "C" void mqemu_imgui_draw_circle(float x, float y, float radius, float r, float g, float b, float a, float thickness, bool filled)
{
	guarded_void([&] {
		ImU32 color = ImGui::ColorConvertFloat4ToU32(ImVec4(r, g, b, a));
		ImDrawList* list = ImGui::GetWindowDrawList();
		if (filled)
			list->AddCircleFilled(ImVec2(x, y), radius, color);
		else
			list->AddCircle(ImVec2(x, y), radius, color, 0, thickness);
	});
}

extern "C" void mqemu_imgui_draw_text(float x, float y, float r, float g, float b, float a, const char* text, size_t len)
{
	guarded_void([&] {
		ImGui::GetWindowDrawList()->AddText(ImVec2(x, y), ImGui::ColorConvertFloat4ToU32(ImVec4(r, g, b, a)), text, text + len);
	});
}
