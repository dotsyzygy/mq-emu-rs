// Draw lists and images (see imgui.cpp for the conventions).
//
// Draw lists are ImDrawList pointers from the three *_draw_list functions,
// valid until the end of the frame. Points are arrays of (x, y) float pairs,
// in screen coordinates; colors are packed ImU32s (IM_COL32 layout). Shapes
// with a thickness are outlined, and filled when it's 0 or less.

#include "imgui_shim.h"
#include "imgui/imgui_internal.h"

#include <windows.h>
#include <unknwn.h>

using namespace mqemu;

// ----- images -----

// `texture` is an ImTextureID (a Direct3D 9 texture on emu). `bg` and `tint`
// are RGBA, 4 floats.
extern "C" void mqemu_imgui_image(void* texture, float w, float h, const float* uv, const float* bg, const float* tint)
{
	guarded_void([&] { ImGui::ImageWithBg(texture_ref(texture), ImVec2(w, h), point(uv, 0), point(uv, 1), color(bg), color(tint)); });
}

extern "C" bool mqemu_imgui_image_button(const char* id, size_t id_len, void* texture, float w, float h, const float* uv,
	const float* bg, const float* tint)
{
	return guarded(false, [&] {
		return ImGui::ImageButton(CString(id, id_len), texture_ref(texture), ImVec2(w, h), point(uv, 0), point(uv, 1), color(bg), color(tint));
	});
}

// ----- the lists -----

// The current window's: drawn with it, clipped to it.
extern "C" ImDrawList* mqemu_imgui_window_draw_list()
{
	return guarded<ImDrawList*>(nullptr, [] { return ImGui::GetWindowDrawList(); });
}

// Over every window.
extern "C" ImDrawList* mqemu_imgui_foreground_draw_list()
{
	return guarded<ImDrawList*>(nullptr, [] { return ImGui::GetForegroundDrawList(); });
}

// Under every window (but over the game).
extern "C" ImDrawList* mqemu_imgui_background_draw_list()
{
	return guarded<ImDrawList*>(nullptr, [] { return ImGui::GetBackgroundDrawList(); });
}

// ----- shapes -----

extern "C" void mqemu_imgui_draw_line(ImDrawList* list, const float* p, ImU32 col, float thickness)
{
	guarded_void([&] { list->AddLine(point(p, 0), point(p, 1), col, thickness); });
}

// `p` is the top-left and bottom-right corners; `flags` ImDrawFlags_RoundCorners*.
extern "C" void mqemu_imgui_draw_rect(ImDrawList* list, const float* p, ImU32 col, float rounding, int flags, float thickness)
{
	guarded_void([&] {
		if (thickness > 0.0f)
			list->AddRect(point(p, 0), point(p, 1), col, rounding, flags, thickness);
		else
			list->AddRectFilled(point(p, 0), point(p, 1), col, rounding, flags);
	});
}

// Filled, with a color per corner: top left, top right, bottom right, bottom left.
extern "C" void mqemu_imgui_draw_rect_multicolor(ImDrawList* list, const float* p, const ImU32* cols)
{
	guarded_void([&] { list->AddRectFilledMultiColor(point(p, 0), point(p, 1), cols[0], cols[1], cols[2], cols[3]); });
}

// A triangle (`count` 3) or quad (4), points clockwise.
extern "C" void mqemu_imgui_draw_triangle_or_quad(ImDrawList* list, const float* p, int count, ImU32 col, float thickness)
{
	guarded_void([&] {
		bool filled = thickness <= 0.0f;
		if (count == 3 && filled)
			list->AddTriangleFilled(point(p, 0), point(p, 1), point(p, 2), col);
		else if (count == 3)
			list->AddTriangle(point(p, 0), point(p, 1), point(p, 2), col, thickness);
		else if (count == 4 && filled)
			list->AddQuadFilled(point(p, 0), point(p, 1), point(p, 2), point(p, 3), col);
		else if (count == 4)
			list->AddQuad(point(p, 0), point(p, 1), point(p, 2), point(p, 3), col, thickness);
	});
}

// A circle (`segments` 0 picks a smooth count), or with `ngon` a regular
// polygon of `segments` sides (3 or more).
extern "C" void mqemu_imgui_draw_circle(ImDrawList* list, float x, float y, float radius, ImU32 col, int segments, bool ngon, float thickness)
{
	guarded_void([&] {
		ImVec2 center(x, y);
		bool filled = thickness <= 0.0f;
		if (ngon && segments < 3)
			throw ImGuiException("a regular polygon needs at least 3 sides");
		if (ngon && filled)
			list->AddNgonFilled(center, radius, col, segments);
		else if (ngon)
			list->AddNgon(center, radius, col, segments, thickness);
		else if (filled)
			list->AddCircleFilled(center, radius, col, segments);
		else
			list->AddCircle(center, radius, col, segments, thickness);
	});
}

// `rotation` in radians.
extern "C" void mqemu_imgui_draw_ellipse(ImDrawList* list, float x, float y, float radius_x, float radius_y, ImU32 col,
	float rotation, int segments, float thickness)
{
	guarded_void([&] {
		if (thickness > 0.0f)
			list->AddEllipse(ImVec2(x, y), ImVec2(radius_x, radius_y), col, rotation, segments, thickness);
		else
			list->AddEllipseFilled(ImVec2(x, y), ImVec2(radius_x, radius_y), col, rotation, segments);
	});
}

// A cubic (`count` 4 points) or quadratic (3) Bezier curve. Never filled.
extern "C" void mqemu_imgui_draw_bezier(ImDrawList* list, const float* p, int count, ImU32 col, float thickness, int segments)
{
	guarded_void([&] {
		if (count == 4)
			list->AddBezierCubic(point(p, 0), point(p, 1), point(p, 2), point(p, 3), col, thickness, segments);
		else if (count == 3)
			list->AddBezierQuadratic(point(p, 0), point(p, 1), point(p, 2), col, thickness, segments);
	});
}

// Lines through `count` points, back to the first with ImDrawFlags_Closed.
extern "C" void mqemu_imgui_draw_polyline(ImDrawList* list, const float* p, int count, ImU32 col, int flags, float thickness)
{
	guarded_void([&] { list->AddPolyline(points(p), count, col, flags, thickness); });
}

// A filled polygon, points clockwise, without holes or crossings. `convex`
// picks ImGui's cheaper fill, which only handles convex ones.
extern "C" void mqemu_imgui_draw_poly_filled(ImDrawList* list, const float* p, int count, ImU32 col, bool convex)
{
	guarded_void([&] {
		if (convex)
			list->AddConvexPolyFilled(points(p), count, col);
		else
			list->AddConcavePolyFilled(points(p), count, col);
	});
}

// Text as is (no formatting) with its top left at (x, y). Null `font` is the
// current font, `size` 0 the current size; `wrap_width` > 0 wraps.
extern "C" void mqemu_imgui_draw_text(ImDrawList* list, ImFont* font, float size, float x, float y, ImU32 col,
	const char* text, size_t len, float wrap_width)
{
	guarded_void([&] { list->AddText(font, size, ImVec2(x, y), col, text, text + len, wrap_width); });
}

// An image from `p[0]` to `p[1]`, texture coordinates `uv[0]` to `uv[1]`,
// tinted with `col`; rounded with `rounding` > 0.
extern "C" void mqemu_imgui_draw_image(ImDrawList* list, void* texture, const float* p, const float* uv, ImU32 col,
	float rounding, int flags)
{
	guarded_void([&] {
		if (rounding > 0.0f)
			list->AddImageRounded(texture_ref(texture), point(p, 0), point(p, 1), point(uv, 0), point(uv, 1), col, rounding, flags);
		else
			list->AddImage(texture_ref(texture), point(p, 0), point(p, 1), point(uv, 0), point(uv, 1), col);
	});
}

// An image on any 4 corners (clockwise from the top left), with a texture
// coordinate for each.
extern "C" void mqemu_imgui_draw_image_quad(ImDrawList* list, void* texture, const float* p, const float* uv, ImU32 col)
{
	guarded_void([&] {
		list->AddImageQuad(texture_ref(texture), point(p, 0), point(p, 1), point(p, 2), point(p, 3), point(uv, 0), point(uv, 1),
			point(uv, 2), point(uv, 3), col);
	});
}

// ----- the path -----
// Points added with these, then stroked or filled, which clears it.

extern "C" void mqemu_imgui_path_clear(ImDrawList* list)
{
	guarded_void([&] { list->PathClear(); });
}

extern "C" void mqemu_imgui_path_line_to(ImDrawList* list, float x, float y)
{
	guarded_void([&] { list->PathLineTo(ImVec2(x, y)); });
}

// An arc of a circle (`radius_y` < 0) or of an ellipse rotated by
// `rotation`, from angle `from` to `to` (radians, clockwise from +x).
extern "C" void mqemu_imgui_path_arc_to(ImDrawList* list, float x, float y, float radius_x, float radius_y, float rotation,
	float from, float to, int segments)
{
	guarded_void([&] {
		if (radius_y < 0.0f)
			list->PathArcTo(ImVec2(x, y), radius_x, from, to, segments);
		else
			list->PathEllipticalArcTo(ImVec2(x, y), ImVec2(radius_x, radius_y), rotation, from, to, segments);
	});
}

// A cubic (`count` 3 more points) or quadratic (2) Bezier curve from the
// path's last point.
extern "C" void mqemu_imgui_path_bezier_to(ImDrawList* list, const float* p, int count, int segments)
{
	guarded_void([&] {
		if (count == 3)
			list->PathBezierCubicCurveTo(point(p, 0), point(p, 1), point(p, 2), segments);
		else if (count == 2)
			list->PathBezierQuadraticCurveTo(point(p, 0), point(p, 1), segments);
	});
}

extern "C" void mqemu_imgui_path_rect(ImDrawList* list, const float* p, float rounding, int flags)
{
	guarded_void([&] { list->PathRect(point(p, 0), point(p, 1), rounding, flags); });
}

// Strokes the path (`thickness` > 0, `flags` ImDrawFlags_Closed to close it)
// or fills it (convex or not), then clears it.
extern "C" void mqemu_imgui_path_finish(ImDrawList* list, ImU32 col, int flags, float thickness, bool convex)
{
	guarded_void([&] {
		if (thickness > 0.0f)
			list->PathStroke(col, flags, thickness);
		else if (convex)
			list->PathFillConvex(col);
		else
			list->PathFillConcave(col);
	});
}

// ----- clipping and channels -----

// Clips what's drawn next to `p[0]`-`p[1]` (within the current clip
// rectangle, with `intersect`) until the matching pop.
extern "C" void mqemu_imgui_draw_push_clip_rect(ImDrawList* list, const float* p, bool intersect)
{
	guarded_void([&] { list->PushClipRect(point(p, 0), point(p, 1), intersect); });
}

extern "C" void mqemu_imgui_draw_pop_clip_rect(ImDrawList* list)
{
	guarded_void([&] {
		// ImGui pushes the first one itself; popping it leaves the list
		// without one, which a release imgui.dll doesn't check.
		if (list->_ClipRectStack.Size <= 1)
			throw ImGuiException("Calling PopClipRect() too many times!");
		list->PopClipRect();
	});
}

// Splits the list into `count` channels (2 or more), merged back in channel
// order: draw the background in a later call on channel 0. False (nothing
// split) if it's split already, which ImGui doesn't support.
extern "C" bool mqemu_imgui_draw_channels_split(ImDrawList* list, int count)
{
	return guarded(false, [&] {
		if (list->_Splitter._Count > 1 || count < 2)
			return false;
		list->ChannelsSplit(count);
		return true;
	});
}

// False (nothing changed) if `channel` isn't one of the split channels.
extern "C" bool mqemu_imgui_draw_channels_set_current(ImDrawList* list, int channel)
{
	return guarded(false, [&] {
		if (channel < 0 || channel >= list->_Splitter._Count)
			return false;
		list->ChannelsSetCurrent(channel);
		return true;
	});
}

extern "C" void mqemu_imgui_draw_channels_merge(ImDrawList* list)
{
	guarded_void([&] { list->ChannelsMerge(); });
}

// ----- textures from memory -----
//
// RGBA pixels (IM_COL32 layout, not premultiplied) in an ImTextureData that
// ImGui's renderer uploads before drawing, and uploads again after the
// graphics device is lost (the backend's SetStatus(Destroyed) asks for that).

// A `w` x `h` texture of `pixels`, row by row.
extern "C" ImTextureData* mqemu_imgui_user_texture_new(int w, int h, const ImU32* pixels)
{
	return guarded<ImTextureData*>(nullptr, [&] {
		ImTextureData* texture = IM_NEW(ImTextureData)();
		texture->Create(ImTextureFormat_RGBA32, w, h);
		// Without this the DX9 backend copies raw RGBA into BGRA textures.
		texture->UseColors = true;
		memcpy(texture->Pixels, pixels, size_t(w) * h * 4);
		ImGui::RegisterUserTexture(texture);
		return texture;
	});
}

// Replaces the rectangle (x, y, w, h), `pixels` its rows; uploaded before
// the next draw.
extern "C" void mqemu_imgui_user_texture_update(ImTextureData* texture, int x, int y, int w, int h, const ImU32* pixels)
{
	guarded_void([&] {
		for (int row = 0; row < h; row++)
			memcpy(texture->GetPixelsAt(x, y + row), pixels + size_t(row) * w, size_t(w) * 4);
		ImTextureRect rect = { (unsigned short)x, (unsigned short)y, (unsigned short)w, (unsigned short)h };
		switch (texture->Status)
		{
		case ImTextureStatus_OK:
			// The backend leaves the last updates listed once it has done them.
			texture->Updates.resize(0);
			texture->UpdateRect = rect;
			texture->Updates.push_back(rect);
			texture->SetStatus(ImTextureStatus_WantUpdates);
			break;
		case ImTextureStatus_WantUpdates:
		{
			ImTextureRect& all = texture->UpdateRect;
			int x0 = ImMin<int>(all.x, x), y0 = ImMin<int>(all.y, y);
			int x1 = ImMax<int>(all.x + all.w, x + w), y1 = ImMax<int>(all.y + all.h, y + h);
			all = { (unsigned short)x0, (unsigned short)y0, (unsigned short)(x1 - x0), (unsigned short)(y1 - y0) };
			texture->Updates.push_back(rect);
			break;
		}
		default:
			// Not uploaded yet: it will be whole.
			break;
		}
	});
}

// Whether the renderer has uploaded it (it can be drawn before then, and
// shows from the frame it's uploaded in).
extern "C" bool mqemu_imgui_user_texture_uploaded(const ImTextureData* texture)
{
	return guarded(false, [&] { return texture->TexID != ImTextureID_Invalid; });
}

// Frees it and its graphics texture: only once no frame drawing it is still
// to be rendered. Releases the texture as the DX9 backend would (on emu, a
// TexID is an IDirect3DTexture9*).
extern "C" void mqemu_imgui_user_texture_free(ImTextureData* texture)
{
	guarded_void([&] {
		if (texture->TexID != ImTextureID_Invalid)
			static_cast<IUnknown*>(texture->TexID)->Release();
		texture->SetTexID(ImTextureID_Invalid);
		ImGui::UnregisterUserTexture(texture);
		IM_DELETE(texture);
	});
}

// ----- meshes -----

// Triangles: `vtx_count` ImDrawVerts (position, texture coordinate, packed
// color: 20 bytes each) and `idx_count` indices into them, 3 per triangle,
// textured with `texture`. Clipped to `clip` (2 points) within the list's
// current clip rectangle. At most 65536 vertices (ImDrawIdx is 16 bits).
extern "C" void mqemu_imgui_draw_mesh(ImDrawList* list, ImTextureData* texture, const ImDrawVert* vtx, int vtx_count,
	const ImDrawIdx* idx, int idx_count, const float* clip)
{
	guarded_void([&] {
		if (vtx_count == 0 || idx_count == 0)
			return;
		ImTextureRef ref;
		ref._TexData = texture;
		list->PushClipRect(point(clip, 0), point(clip, 1), true);
		list->PushTexture(ref);
		// May start a new vertex offset, resetting _VtxCurrentIdx: read it after.
		list->PrimReserve(idx_count, vtx_count);
		ImDrawIdx base = (ImDrawIdx)list->_VtxCurrentIdx;
		memcpy(list->_VtxWritePtr, vtx, size_t(vtx_count) * sizeof(ImDrawVert));
		for (int i = 0; i < idx_count; i++)
			list->_IdxWritePtr[i] = (ImDrawIdx)(base + idx[i]);
		list->_VtxWritePtr += vtx_count;
		list->_IdxWritePtr += idx_count;
		list->_VtxCurrentIdx += vtx_count;
		list->PopTexture();
		list->PopClipRect();
	});
}
