// Test-only: a headless ImGui context, so the wrappers in imgui.cpp (and
// mq-emu's safe API on top) can be driven against MacroQuest's real
// imgui.dll without a game or a renderer. Only test binaries call these, so
// only they link this file.

#include "mq_api.h"

#include "imgui/imgui.h"
#include "imgui/imgui_internal.h"
#include "imgui/ImGuiUtils.h"
#include "imgui/implot/implot.h"
#include "imgui/implot/implot_internal.h"

#include <cstring>
#include <exception>

namespace {

ImGuiContext* s_context = nullptr;

} // namespace

// Creates a context (once) as MacroQuest does: with an ImPlot context,
// MacroQuest's fonts and docking on; plus a fixed-size display and no ini file.
extern "C" void mqemu_imgui_test_init()
{
	if (s_context)
		return;
	s_context = ImGui::CreateContext();
	if (!ImPlot::GetCurrentContext())
		ImPlot::CreateContext();
	ImGuiIO& io = ImGui::GetIO();
	mq::imgui::ConfigureFonts(io.Fonts);
	io.ConfigFlags |= ImGuiConfigFlags_DockingEnable;
	io.IniFilename = nullptr;
	io.LogFilename = nullptr;
	io.DisplaySize = ImVec2(1280, 800);
	io.DeltaTime = 1.0f / 60.0f;
	// Lets the atlas build itself on NewFrame, as a modern renderer would.
	io.BackendFlags |= ImGuiBackendFlags_RendererHasTextures;
}

// Starts a frame with the mouse at (x, y), the left button down or not.
// False (with the error kept for mqemu_imgui_take_error) if ImGui threw.
extern "C" bool mqemu_imgui_test_new_frame(float mouse_x, float mouse_y, bool mouse_down)
{
	mqemu_imgui_test_init();
	try
	{
		ImGuiIO& io = ImGui::GetIO();
		io.DeltaTime = 1.0f / 60.0f;
		io.AddMousePosEvent(mouse_x, mouse_y);
		io.AddMouseButtonEvent(0, mouse_down);
		ImGui::NewFrame();
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// Ends the frame as MacroQuest does (ImGui::Render), copying what it threw,
// if anything, into `error`. False if it threw: MacroQuest would then pause
// every plugin's ImGui.
extern "C" bool mqemu_imgui_test_end_frame(char* error, size_t cap)
{
	try
	{
		ImGui::Render();
		return true;
	}
	catch (const std::exception& e)
	{
		if (error && cap)
		{
			strncpy_s(error, cap, e.what(), _TRUNCATE);
		}
	}
	catch (...)
	{
		if (error && cap)
			strncpy_s(error, cap, "unknown C++ exception", _TRUNCATE);
	}
	// The frame is broken; start the next one from a fresh context (and
	// ImPlot state, which may be mid-plot).
	try
	{
		ImPlot::DestroyContext();
		ImGui::DestroyContext(s_context);
	}
	catch (...)
	{
	}
	s_context = nullptr;
	return false;
}

// The last item's rectangle, in screen coordinates.
extern "C" void mqemu_imgui_test_last_item_rect(float* min_x, float* min_y, float* max_x, float* max_y)
{
	ImGuiContext& g = *ImGui::GetCurrentContext();
	*min_x = g.LastItemData.Rect.Min.x;
	*min_y = g.LastItemData.Rect.Min.y;
	*max_x = g.LastItemData.Rect.Max.x;
	*max_y = g.LastItemData.Rect.Max.y;
}

// How deep the window stack is (1 is the implicit "Debug" window only).
extern "C" int mqemu_imgui_test_window_depth()
{
	return ImGui::GetCurrentContext()->CurrentWindowStack.Size;
}

// The ID stack depth of the current window.
extern "C" int mqemu_imgui_test_id_depth()
{
	return ImGui::GetCurrentContext()->CurrentWindow->IDStack.Size;
}

// Types UTF-8 text into whatever has keyboard focus, on the next frame.
extern "C" void mqemu_imgui_test_type(const char* utf8)
{
	ImGui::GetIO().AddInputCharactersUTF8(utf8);
}

// Vertices in the current window's draw list so far.
extern "C" int mqemu_imgui_test_window_vertices()
{
	return ImGui::GetWindowDrawList()->VtxBuffer.Size;
}

// ImGui's own SetCursorScreenPos, without what mqemu_imgui_set_cursor_screen_pos
// adds.
extern "C" void mqemu_imgui_test_set_cursor_screen_pos_raw(float x, float y)
{
	ImGui::SetCursorScreenPos(ImVec2(x, y));
}

// How many vertices `list` has so far.
extern "C" int mqemu_imgui_test_list_vertices(ImDrawList* list)
{
	return list->VtxBuffer.Size;
}

// Whether any of `list`'s draw commands so far uses `texture`.
extern "C" bool mqemu_imgui_test_list_uses_texture(ImDrawList* list, void* texture)
{
	for (const ImDrawCmd& cmd : list->CmdBuffer)
		if (cmd.TexRef._TexData == nullptr && cmd.TexRef._TexID == static_cast<ImTextureID>(texture))
			return true;
	return list->_CmdHeader.TexRef._TexData == nullptr && list->_CmdHeader.TexRef._TexID == static_cast<ImTextureID>(texture);
}

// `list`'s clip rectangle stack depth, path length and channel count.
extern "C" void mqemu_imgui_test_list_state(ImDrawList* list, int* clip_depth, int* path_points, int* channels)
{
	*clip_depth = list->_ClipRectStack.Size;
	*path_points = list->_Path.Size;
	*channels = list->_Splitter._Count;
}

// The font stack depth and current font.
extern "C" int mqemu_imgui_test_font_depth()
{
	return GImGui->FontStack.Size;
}

extern "C" int mqemu_imgui_test_clipper_depth()
{
	return GImGui->ClipperTempDataStacked;
}

// Whether ImPlot is inside a plot or subplots, and its style and colormap
// stack depths.
extern "C" void mqemu_implot_test_state(bool* in_plot, bool* in_subplots, int* style_depth, int* colormap_depth)
{
	ImPlotContext& gp = *ImPlot::GetCurrentContext();
	*in_plot = gp.CurrentPlot != nullptr;
	*in_subplots = gp.CurrentSubplot != nullptr;
	*style_depth = gp.ColorModifiers.Size + gp.StyleModifiers.Size;
	*colormap_depth = gp.ColormapModifiers.Size;
}

// A fake ImTextureID for drawing images headlessly: nothing renders it.
extern "C" void* mqemu_imgui_test_texture()
{
	static int fake_texture;
	return &fake_texture;
}

// The bytes of the payload being dragged, if any (for checking what a drag
// source sent).
extern "C" int mqemu_imgui_test_payload_size()
{
	const ImGuiPayload* payload = ImGui::GetDragDropPayload();
	return payload ? payload->DataSize : -1;
}

// Turns docking on or off, as MacroQuest's overlay setting does.
extern "C" void mqemu_imgui_test_set_docking(bool enabled)
{
	ImGuiIO& io = ImGui::GetIO();
	if (enabled)
		io.ConfigFlags |= ImGuiConfigFlags_DockingEnable;
	else
		io.ConfigFlags &= ~ImGuiConfigFlags_DockingEnable;
}

// Holds (or lets go of) ctrl and shift, from the next frame.
extern "C" void mqemu_imgui_test_modifiers(bool ctrl, bool shift)
{
	ImGuiIO& io = ImGui::GetIO();
	io.AddKeyEvent(ImGuiMod_Ctrl, ctrl);
	io.AddKeyEvent(ImGuiMod_Shift, shift);
}

// Presses (or releases) the key for letter 'A' to 'Z', from the next frame.
extern "C" void mqemu_imgui_test_letter_key(char letter, bool down)
{
	ImGui::GetIO().AddKeyEvent(static_cast<ImGuiKey>(ImGuiKey_A + (letter - 'A')), down);
}

// ImPlot's own EndPlot, without the shim forgetting the plot's callbacks.
extern "C" void mqemu_implot_test_end_plot_raw()
{
	ImPlot::EndPlot();
}

// Whether `axis` of the plot titled `title` (NUL-terminated, in the current
// window) still has a formatter or transform set.
extern "C" void mqemu_implot_test_axis_callbacks(const char* title, int axis, bool* formatter, bool* transform)
{
	ImPlotPlot* plot = ImPlot::GetPlot(title);
	*formatter = plot && plot->Axes[axis].Formatter != nullptr;
	*transform = plot && plot->Axes[axis].TransformForward != nullptr;
}
