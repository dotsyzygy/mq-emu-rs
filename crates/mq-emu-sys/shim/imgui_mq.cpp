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
#include "eqlib/game/UITemplates.h"
#include "eqlib/game/UITextures.h"
#include "eqlib/graphics/GraphicsResources.h"
#include "imgui/imgui_internal.h"

#include <d3d9.h>
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

// ----- the game's UI texture files, and pieces of them -----

// Our copies of a CUITextureInfo or CUITexturePiece keep the texture ID the
// game's loader gives them the first time they're drawn (MacroQuest's
// GetTextureID writes it back), so they stay loaded once.

namespace {

template <typename T, typename... Args>
T* copy_new(Args&&... args)
{
	// As for CTextureAnimation above: made and freed by this plugin's runtime.
	void* memory = ::operator new(sizeof(T));
	return new (memory) T(std::forward<Args>(args)...);
}

template <typename T>
void copy_delete(T* value)
{
	value->~T();
	::operator delete(value);
}

eqlib::CXRect source_rect(int x0, int y0, int x1, int y1)
{
	// MacroQuest draws the whole texture for an "abnormal" rectangle.
	return x1 < x0 ? eqlib::CXRect(0, 0, -1, -1) : eqlib::CXRect(x0, y0, x1, y1);
}

// The size to draw at: `w` x `h`, or for 0 the source's (MacroQuest's own
// default is the whole texture's, even for a piece of it).
eqlib::CXSize draw_size(int w, int h, const eqlib::CXRect& source, const eqlib::CUITextureInfo& texture)
{
	if (w != 0 && h != 0)
		return eqlib::CXSize(w, h);
	return source.IsAbnormal() ? texture.TextureSize : source.GetSize();
}

} // namespace

// A copy of the UI texture file `name` (as the game's SIDL files name them,
// e.g. "AttackIndicator.tga": from the UI skin's directory, else the default
// one), loaded, with its size. Null if the game can't load it, or has no UI.
extern "C" eqlib::CUITextureInfo* mqemu_ui_texture_find(const char* name, size_t name_len)
{
	return guarded<eqlib::CUITextureInfo*>(nullptr, [&]() -> eqlib::CUITextureInfo* {
		eqlib::CEQSuiteTextureLoader* loader = eqlib::pEQSuiteTextureLoader;
		if (!loader || name_len == 0)
			return nullptr;
		eqlib::CUITextureInfo info(eqlib::CXStr(std::string_view(name, name_len)), eqlib::CXSize(0, 0));
		info.TextureId = loader->CreateTexture(info);
		if (static_cast<int>(info.TextureId) == -1)
			return nullptr;
		eqlib::BMI* bitmap = loader->GetTexture(info);
		auto* texture = bitmap && bitmap->pBmp ? static_cast<IDirect3DTexture9*>(bitmap->pBmp->GetTexture()) : nullptr;
		D3DSURFACE_DESC desc;
		if (!texture || FAILED(texture->GetLevelDesc(0, &desc)))
			return nullptr;
		// The texture's own size, which source rectangles are measured in.
		info.TextureSize = eqlib::CXSize(static_cast<int>(desc.Width), static_cast<int>(desc.Height));
		return copy_new<eqlib::CUITextureInfo>(info);
	});
}

extern "C" void mqemu_ui_texture_free(eqlib::CUITextureInfo* texture)
{
	guarded_void([&] { copy_delete(texture); });
}

extern "C" void mqemu_ui_texture_size(const eqlib::CUITextureInfo* texture, int* w, int* h)
{
	*w = texture->TextureSize.cx;
	*h = texture->TextureSize.cy;
}

// Its file name, NUL-terminated and cut to `cap`; returns its whole length.
extern "C" size_t mqemu_ui_texture_name(const eqlib::CUITextureInfo* texture, char* out, size_t cap)
{
	return guarded<size_t>(0, [&] {
		std::string_view name = texture->Name;
		if (cap)
		{
			size_t n = std::min(name.size(), cap - 1);
			memcpy(out, name.data(), n);
			out[n] = 0;
		}
		return name.size();
	});
}

// A piece of `texture`: its rectangle (x0, y0)-(x1, y1).
extern "C" eqlib::CUITexturePiece* mqemu_texture_piece_new(const eqlib::CUITextureInfo* texture, int x0, int y0, int x1, int y1)
{
	return guarded<eqlib::CUITexturePiece*>(nullptr, [&] {
		return copy_new<eqlib::CUITexturePiece>(*texture, eqlib::CXRect(x0, y0, x1, y1));
	});
}

// How many frames an animation has (one for a picture; a grid's cells are
// within its frame).
extern "C" int mqemu_texture_animation_frames(const eqlib::CTextureAnimation* animation)
{
	return animation->Frames.GetLength();
}

// A copy of frame `frame`'s piece; null past its frames.
extern "C" eqlib::CUITexturePiece* mqemu_texture_animation_piece(const eqlib::CTextureAnimation* animation, int frame)
{
	return guarded<eqlib::CUITexturePiece*>(nullptr, [&]() -> eqlib::CUITexturePiece* {
		if (frame < 0 || frame >= animation->Frames.GetLength())
			return nullptr;
		return copy_new<eqlib::CUITexturePiece>(animation->Frames[frame].Piece);
	});
}

extern "C" void mqemu_texture_piece_free(eqlib::CUITexturePiece* piece)
{
	guarded_void([&] { copy_delete(piece); });
}

// Its rectangle in its texture.
extern "C" void mqemu_texture_piece_rect(const eqlib::CUITexturePiece* piece, int* x0, int* y0, int* x1, int* y1)
{
	const eqlib::CXRect& rect = piece->GetRect();
	*x0 = rect.left;
	*y0 = rect.top;
	*x1 = rect.right;
	*y1 = rect.bottom;
}

// A copy of the texture it's a piece of.
extern "C" eqlib::CUITextureInfo* mqemu_texture_piece_texture(const eqlib::CUITexturePiece* piece)
{
	return guarded<eqlib::CUITextureInfo*>(nullptr, [&] { return copy_new<eqlib::CUITextureInfo>(piece->GetTextureInfo()); });
}

// Draws the rectangle (sx0, sy0)-(sx1, sy1) of `texture` (the whole of it if
// sx1 < sx0) at the cursor as an item `w` x `h` (0 for the rectangle's own
// size), tinted, framed with `border` unless that's transparent.
//
// MacroQuest's DrawUITexture for the current window draws at the screen's
// top left instead of the cursor (it passes CXPoint() as the position), so
// this lays the item out as its sibling functions do (HandleFraming) and
// draws it with the draw list version.
extern "C" bool mqemu_imgui_ui_texture(eqlib::CUITextureInfo* texture, int w, int h, int sx0, int sy0, int sx1, int sy1,
	ImU32 tint, ImU32 border)
{
	return guarded(false, [&] {
		ImGuiWindow* window = ImGui::GetCurrentWindow();
		if (window->SkipItems)
			return false;
		eqlib::CXRect source = source_rect(sx0, sy0, sx1, sy1);
		eqlib::CXSize size = draw_size(w, h, source, *texture);
		ImVec2 pos = window->DC.CursorPos;
		bool framed = (border >> IM_COL32_A_SHIFT) != 0;
		ImVec2 extent(static_cast<float>(size.cx + (framed ? 2 : 0)), static_cast<float>(size.cy + (framed ? 2 : 0)));
		ImGui::ItemSize(ImVec2(static_cast<float>(size.cx), static_cast<float>(size.cy)));
		if (!ImGui::ItemAdd(ImRect(pos, ImVec2(pos.x + extent.x, pos.y + extent.y)), 0))
			return false;
		return mq::imgui::DrawUITexture(window->DrawList, *texture, eqlib::CXPoint(static_cast<int>(pos.x), static_cast<int>(pos.y)),
			size, source, mq_color(tint), mq_color(border));
	});
}

// Draws it on `list` at (x, y), as mqemu_imgui_ui_texture. (MacroQuest's
// version draws nothing at size 0, so the size is always given.)
extern "C" bool mqemu_imgui_draw_ui_texture(ImDrawList* list, eqlib::CUITextureInfo* texture, int x, int y, int w, int h,
	int sx0, int sy0, int sx1, int sy1, ImU32 tint, ImU32 border)
{
	return guarded(false, [&] {
		eqlib::CXRect source = source_rect(sx0, sy0, sx1, sy1);
		return mq::imgui::DrawUITexture(list, *texture, eqlib::CXPoint(x, y), draw_size(w, h, source, *texture), source,
			mq_color(tint), mq_color(border));
	});
}

// Draws the piece at the cursor as an item `w` x `h` (0 for its own size).
extern "C" bool mqemu_imgui_texture_piece(eqlib::CUITexturePiece* piece, int w, int h, ImU32 tint, ImU32 border)
{
	return guarded(false, [&] {
		eqlib::CXSize size = draw_size(w, h, piece->GetRect(), piece->GetTextureInfo());
		return mq::imgui::DrawTexturePiece(*piece, size, piece->GetRect(), mq_color(tint), mq_color(border));
	});
}

extern "C" bool mqemu_imgui_draw_texture_piece(ImDrawList* list, eqlib::CUITexturePiece* piece, int x, int y, int w, int h,
	ImU32 tint, ImU32 border)
{
	return guarded(false, [&] {
		eqlib::CXSize size = draw_size(w, h, piece->GetRect(), piece->GetTextureInfo());
		return mq::imgui::DrawTexturePiece(list, *piece, eqlib::CXPoint(x, y), size, piece->GetRect(), mq_color(tint), mq_color(border));
	});
}

// ----- screen pieces -----

// The UI's screen piece template `name` (an item of the game's SIDL files,
// e.g. "Inv_Seperator1"): the game's own, valid until it unloads its UI.
// Null if there's none.
extern "C" eqlib::CScreenPieceTemplate* mqemu_screen_piece_find(const char* name, size_t name_len)
{
	return guarded<eqlib::CScreenPieceTemplate*>(nullptr, [&]() -> eqlib::CScreenPieceTemplate* {
		if (!eqlib::pSidlMgr || name_len == 0)
			return nullptr;
		return eqlib::pSidlMgr->FindScreenPieceTemplate(eqlib::CXStr(std::string_view(name, name_len)));
	});
}

// What kind it is (an EStaticScreenPieceClasses value).
extern "C" int mqemu_screen_piece_kind(const eqlib::CScreenPieceTemplate* piece)
{
	return guarded(-1, [&] { return static_cast<int>(eqlib::pSidlMgr->GetScreenPieceEnum(piece)); });
}

// Whether MacroQuest's DrawScreenPiece draws it: an animation or a tinted
// blend animation set to draw itself (AutoDraw). It draws no other kind.
extern "C" bool mqemu_screen_piece_draws(const eqlib::CScreenPieceTemplate* piece)
{
	return guarded(false, [&] {
		switch (eqlib::pSidlMgr->GetScreenPieceEnum(piece))
		{
		case eqlib::StaticScreenPieceAnimation:
		case eqlib::StaticScreenPieceTintedBlendAnimation:
			return static_cast<const eqlib::CStaticScreenPieceTemplate*>(piece)->bAutoDraw;
		default:
			return false;
		}
	});
}

// Its rectangle as its screen lays it out (relative to the window it's in,
// unless it's positioned absolutely).
extern "C" void mqemu_screen_piece_rect(const eqlib::CScreenPieceTemplate* piece, int* x0, int* y0, int* x1, int* y1)
{
	*x0 = piece->rect.left;
	*y0 = piece->rect.top;
	*x1 = piece->rect.right;
	*y1 = piece->rect.bottom;
}

// Draws it on `list` as its screen would in a window covering (x0, y0)-(x1,
// y1). False, drawing nothing, if it's a kind MacroQuest doesn't draw.
extern "C" bool mqemu_imgui_draw_screen_piece(ImDrawList* list, const eqlib::CScreenPieceTemplate* piece, int x0, int y0, int x1, int y1)
{
	return guarded(false, [&] {
		if (!mqemu_screen_piece_draws(piece))
			return false;
		mq::imgui::DrawScreenPiece(list, piece, eqlib::CXRect(x0, y0, x1, y1));
		return true;
	});
}
