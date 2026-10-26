// ImGui drawing that needs MacroQuest itself (MQ2Main.dll and eqlib.dll):
// textures loaded through the game's renderer, the game's own UI textures
// (CTextureAnimation: item and spell icons, window art), EQ's fonts, and
// MacroQuest's EQ-styled widgets (spell gems, item link text). Conventions as
// in imgui.cpp. Only plugins can link this file's functions.

#include "imgui_shim.h"

#include "mq/api/Textures.h"
#include "mq/imgui/Widgets.h"
#include "eqlib/game/Globals.h"
#include "eqlib/game/UI.h"
#include "eqlib/game/UITextures.h"

#include <new>

using namespace mqemu;

namespace {

mq::MQColor mq_color(ImU32 col)
{
	return mq::MQColor(static_cast<uint8_t>(col >> IM_COL32_R_SHIFT), static_cast<uint8_t>(col >> IM_COL32_G_SHIFT),
		static_cast<uint8_t>(col >> IM_COL32_B_SHIFT), static_cast<uint8_t>(col >> IM_COL32_A_SHIFT));
}

} // namespace

// ----- textures from files -----

// Loads an image file (any format the game reads: bmp, dds, png, tga, ...)
// as a texture. Null if it can't be loaded, or the game has no renderer yet.
extern "C" mq::MQTexture* mqemu_texture_create(const char* path, size_t path_len)
{
	return guarded<mq::MQTexture*>(nullptr, [&] { return mq::CreateTexture(std::string_view(path, path_len)); });
}

// Frees the texture at once: nothing may draw with it after this, including
// draw data of the current frame not rendered yet.
extern "C" void mqemu_texture_destroy(mq::MQTexture* texture)
{
	guarded_void([&] { mq::DestroyTexture(texture); });
}

// Its ImTextureID now; null while the device is lost. Ask every frame:
// MacroQuest recreates textures after a device reset.
extern "C" void* mqemu_texture_id(const mq::MQTexture* texture)
{
	return guarded<void*>(nullptr, [&] { return static_cast<void*>(texture->GetTextureID()); });
}

extern "C" void mqemu_texture_size(const mq::MQTexture* texture, int* w, int* h)
{
	guarded_void([&] {
		eqlib::CXSize size = texture->GetTextureSize();
		*w = size.cx;
		*h = size.cy;
	});
}

// ----- the game's UI textures -----

// A copy of the UI's texture animation `name` (from the game's SIDL files,
// e.g. "A_DragItem" for item icons, "A_SpellIcons" for spell icons), to
// free with mqemu_texture_animation_free. Null if there's no such
// animation, or no UI yet.
extern "C" eqlib::CTextureAnimation* mqemu_texture_animation_find(const char* name, size_t name_len)
{
	return guarded<eqlib::CTextureAnimation*>(nullptr, [&]() -> eqlib::CTextureAnimation* {
		if (!eqlib::pSidlMgr)
			return nullptr;
		eqlib::CTextureAnimation* found = eqlib::pSidlMgr->FindAnimation(eqlib::CXStr(std::string_view(name, name_len)));
		if (!found)
			return nullptr;
		// Allocated and freed here, by this plugin's C runtime: `delete` would
		// go through the class's virtual destructor and eqlib.dll's own.
		void* memory = ::operator new(sizeof(eqlib::CTextureAnimation));
		return new (memory) eqlib::CTextureAnimation(*found);
	});
}

extern "C" void mqemu_texture_animation_free(eqlib::CTextureAnimation* animation)
{
	guarded_void([&] {
		animation->eqlib::CTextureAnimation::~CTextureAnimation();
		::operator delete(animation);
	});
}

// For grid animations (icon sheets): which cell to draw, e.g. an item's icon
// number minus 500, or a spell's icon.
extern "C" void mqemu_texture_animation_set_cell(eqlib::CTextureAnimation* animation, int cell)
{
	guarded_void([&] { animation->SetCurCell(cell); });
}

extern "C" int mqemu_texture_animation_cell(const eqlib::CTextureAnimation* animation)
{
	return animation->GetCurCell();
}

// Its size, and a grid's cell size (0 if it isn't a grid).
extern "C" void mqemu_texture_animation_size(const eqlib::CTextureAnimation* animation, int* w, int* h, int* cell_w, int* cell_h)
{
	*w = animation->Size.cx;
	*h = animation->Size.cy;
	*cell_w = animation->IsGrid() ? animation->CellWidth : 0;
	*cell_h = animation->IsGrid() ? animation->CellHeight : 0;
}

// Draws the animation at the cursor as an item of `w` x `h` (0 for its own
// size), tinted, with a border unless `border` is transparent.
extern "C" bool mqemu_imgui_texture_animation(const eqlib::CTextureAnimation* animation, int w, int h, ImU32 tint, ImU32 border)
{
	return guarded(false, [&] {
		return mq::imgui::DrawTextureAnimation(animation, eqlib::CXSize(w, h), mq_color(tint), mq_color(border));
	});
}

// Draws the animation on a draw list at (x, y), as mqemu_imgui_texture_animation.
extern "C" bool mqemu_imgui_draw_texture_animation(ImDrawList* list, const eqlib::CTextureAnimation* animation, int x, int y,
	int w, int h, ImU32 tint, ImU32 border)
{
	return guarded(false, [&] {
		return mq::imgui::DrawTextureAnimation(list, animation, eqlib::CXPoint(x, y), eqlib::CXSize(w, h), mq_color(tint), mq_color(border));
	});
}

// ----- EQ fonts -----

// The ImGui copy of EQ font style `style` (0 to 9, as in the game's font
// options). Null if there's none.
extern "C" ImFont* mqemu_imgui_eq_font(int style)
{
	return guarded<ImFont*>(nullptr, [&] { return mq::imgui::GetEQImFont(style); });
}

// Draws `text` as the game would in EQ font style `style`, wrapped and
// clipped to the rectangle (x0, y0)-(x1, y1); `flags` are eqlib's
// DrawTextFlags. Nothing if there's no such font.
extern "C" void mqemu_imgui_draw_eq_text(ImDrawList* list, int style, const char* text, size_t len, int x0, int y0, int x1,
	int y1, ImU32 col, int flags)
{
	guarded_void([&] {
		mq::imgui::DrawEQText(list, style, CString(text, len), eqlib::CXRect(x0, y0, x1, y1), mq_color(col),
			static_cast<uint16_t>(flags));
	});
}

// ----- EQ-styled widgets -----

namespace {

// The game's window for spell gem `gem` (0-based), or null without one.
eqlib::CSpellGemWnd* spell_gem(int gem)
{
	eqlib::CCastSpellWnd* window = eqlib::pCastSpellWnd;
	if (!window || gem < 0 || gem >= eqlib::NUM_SPELL_GEMS)
		return nullptr;
	return window->SpellSlots[gem];
}

} // namespace

// Draws spell gem `gem` (0-based) as the game does, as an item of its size
// on the game's screen. Clicks cast, unmemorize or pick up the spell as
// `flags` (ImGuiSpellGemFlags_) allow, as clicking the game's gem would.
// True when clicked; *drawn false (and nothing drawn) if there's no such gem.
extern "C" bool mqemu_imgui_spell_gem(const char* id, size_t id_len, int gem, int flags, bool* drawn)
{
	*drawn = false;
	return guarded(false, [&] {
		eqlib::CSpellGemWnd* window = spell_gem(gem);
		if (!window)
			return false;
		*drawn = true;
		return mq::imgui::SpellGem(CString(id, id_len), window, flags);
	});
}

// Draws spell gem `gem` on a draw list at (x, y), highlighted if `hovered`.
// False if there's no such gem.
extern "C" bool mqemu_imgui_draw_spell_gem(ImDrawList* list, int gem, int x, int y, bool hovered)
{
	return guarded(false, [&] {
		eqlib::CSpellGemWnd* window = spell_gem(gem);
		if (!window)
			return false;
		mq::imgui::DrawSpellGem(list, window, eqlib::CXPoint(x, y), hovered);
		return true;
	});
}

// Text as is, styled like an item link: `col`, or `hovered` under the mouse.
// Its ID is the text's. True when clicked.
extern "C" bool mqemu_imgui_item_link_text(const char* text, size_t len, ImU32 col, ImU32 hovered)
{
	return guarded(false, [&] { return mq::imgui::ItemLinkText(std::string_view(text, len), mq_color(col), mq_color(hovered)); });
}
