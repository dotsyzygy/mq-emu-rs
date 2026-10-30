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
#include <string>

#include <windows.h>
#include <unknwn.h>

namespace {

ImGuiContext* s_context = nullptr;

// The clipboard, kept here rather than on the machine's.
std::string s_clipboard;

// A stand-in for the renderer's IDirect3DTexture9, counting its releases.
int s_released = 0;

struct FakeTexture final : IUnknown
{
	HRESULT STDMETHODCALLTYPE QueryInterface(REFIID, void** out) override { *out = nullptr; return E_NOINTERFACE; }
	ULONG STDMETHODCALLTYPE AddRef() override { return 2; }
	ULONG STDMETHODCALLTYPE Release() override { s_released++; return 1; }
};

FakeTexture s_fake_texture;

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
	ImGuiPlatformIO& platform = ImGui::GetPlatformIO();
	platform.Platform_GetClipboardTextFn = [](ImGuiContext*) { return s_clipboard.c_str(); };
	platform.Platform_SetClipboardTextFn = [](ImGuiContext*, const char* text) { s_clipboard = text; };
}

// Forgets which window and item have focus, keyboard navigation and the
// active item, so a test doesn't start with an earlier one's: list clippers
// also draw the window's navigation item, wherever it is. Also drops input
// an earlier test left queued (ImGui trickles events over frames) or held.
extern "C" void mqemu_imgui_test_forget_focus()
{
	mqemu_imgui_test_init();
	ImGui::FocusWindow(nullptr);
	ImGui::ClearActiveID();
	ImGuiIO& io = ImGui::GetIO();
	io.ClearEventsQueue();
	io.ClearInputKeys();
	io.ClearInputMouse();
}

// The keyboard navigation item's ID (0 for none).
extern "C" ImGuiID mqemu_imgui_test_nav_id()
{
	return ImGui::GetCurrentContext()->NavId;
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

// Starts a frame with the mouse fed as MacroQuest feeds it in game: the
// position through the event queue (its Win32 backend polls the cursor), but
// the buttons and wheel written straight into io from DirectInput (its
// ProcessMouseEvents detour and ImGuiOverlay_HandleMouseEvent), so no input
// event reports them.
extern "C" bool mqemu_imgui_test_new_frame_as_mq(float mouse_x, float mouse_y, bool mouse_down, float wheel)
{
	mqemu_imgui_test_init();
	try
	{
		ImGuiIO& io = ImGui::GetIO();
		io.DeltaTime = 1.0f / 60.0f;
		io.AddMousePosEvent(mouse_x, mouse_y);
		io.MouseDown[0] = mouse_down;
		io.MouseWheel += wheel;
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

// ----- user textures and meshes -----

// Does what the renderer does with `texture` before drawing: creates or
// updates it (with a fake texture).
extern "C" void mqemu_imgui_test_render_texture(ImTextureData* texture)
{
	if (texture->Status == ImTextureStatus_WantCreate)
		texture->SetTexID(static_cast<ImTextureID>(static_cast<IUnknown*>(&s_fake_texture)));
	if (texture->Status == ImTextureStatus_WantCreate || texture->Status == ImTextureStatus_WantUpdates)
		texture->SetStatus(ImTextureStatus_OK);
}

// Does what the renderer does when the graphics device is lost: frees the
// texture and marks it destroyed.
extern "C" void mqemu_imgui_test_lose_device(ImTextureData* texture)
{
	if (texture->TexID != ImTextureID_Invalid)
		static_cast<IUnknown*>(texture->TexID)->Release();
	texture->SetTexID(ImTextureID_Invalid);
	texture->SetStatus(ImTextureStatus_Destroyed);
}

// How many fake textures have been released.
extern "C" int mqemu_imgui_test_released_textures()
{
	return s_released;
}

// How many user textures are registered.
extern "C" int mqemu_imgui_test_user_textures()
{
	return GImGui->UserTextures.Size;
}

// `texture`'s status (ImTextureStatus), its queued updates, their bounding
// box (x, y, w, h) and the pixel at (x, y).
extern "C" void mqemu_imgui_test_texture_state(ImTextureData* texture, int x, int y, int* status, int* updates, int* rect, ImU32* pixel)
{
	*status = texture->Status;
	*updates = texture->Updates.Size;
	rect[0] = texture->UpdateRect.x;
	rect[1] = texture->UpdateRect.y;
	rect[2] = texture->UpdateRect.w;
	rect[3] = texture->UpdateRect.h;
	*pixel = *static_cast<ImU32*>(texture->GetPixelsAt(x, y));
}

// The last draw command of `list` drawing `texture`: its clip rectangle
// (x0, y0, x1, y1) and how many indices it has. False if none does.
extern "C" bool mqemu_imgui_test_list_texture_cmd(ImDrawList* list, ImTextureData* texture, float* clip, int* indices)
{
	for (int i = list->CmdBuffer.Size - 1; i >= 0; i--)
	{
		const ImDrawCmd& cmd = list->CmdBuffer[i];
		if (cmd.TexRef._TexData != texture || cmd.ElemCount == 0)
			continue;
		clip[0] = cmd.ClipRect.x;
		clip[1] = cmd.ClipRect.y;
		clip[2] = cmd.ClipRect.z;
		clip[3] = cmd.ClipRect.w;
		*indices = (int)cmd.ElemCount;
		return true;
	}
	return false;
}

// `list`'s index count, and index `i` of it.
extern "C" int mqemu_imgui_test_list_indices(ImDrawList* list)
{
	return list->IdxBuffer.Size;
}

extern "C" int mqemu_imgui_test_list_index(ImDrawList* list, int i)
{
	return list->IdxBuffer[i];
}

// Vertex `i` of `list`.
extern "C" void mqemu_imgui_test_list_vertex(ImDrawList* list, int i, ImDrawVert* out)
{
	*out = list->VtxBuffer[i];
}

// Whether ImGui keeps the keyboard and the mouse from the game this frame.
extern "C" void mqemu_imgui_test_want_capture(bool* keyboard, bool* mouse)
{
	*keyboard = ImGui::GetIO().WantCaptureKeyboard;
	*mouse = ImGui::GetIO().WantCaptureMouse;
}

// Turns the mouse wheel, from the next frame.
extern "C" void mqemu_imgui_test_wheel(float x, float y)
{
	ImGui::GetIO().AddMouseWheelEvent(x, y);
}
