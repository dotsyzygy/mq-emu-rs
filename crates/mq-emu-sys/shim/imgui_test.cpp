// Test-only: a headless ImGui context, so the wrappers in imgui.cpp (and
// mq-emu's safe API on top) can be driven against MacroQuest's real
// imgui.dll without a game or a renderer. Only test binaries call these, so
// only they link this file.

#include "mq_api.h"

#include "imgui/imgui.h"
#include "imgui/imgui_internal.h"

#include <cstring>
#include <exception>

namespace {

ImGuiContext* s_context = nullptr;

} // namespace

// Creates a context (once) with a fixed-size display, no ini file and a
// built font atlas.
extern "C" void mqemu_imgui_test_init()
{
	if (s_context)
		return;
	s_context = ImGui::CreateContext();
	ImGuiIO& io = ImGui::GetIO();
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
	// The frame is broken; start the next one from a fresh context.
	try
	{
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
