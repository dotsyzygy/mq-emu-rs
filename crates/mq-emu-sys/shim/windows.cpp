// EverQuest's own UI windows: finding them as MacroQuest does, what only the
// game's functions can say about them (their kind, XML names, tooltip, list
// text, the item in an item slot or behind a hot button), clicking and
// notifying them as /notify and ${Window} do, a plugin's own windows, made
// from screen pieces in UI XML it adds (AddXMLFile), subclassing the game's
// windows, drawing on them, and what /itemnotify does with an item it finds
// no slot window for.
//
// This file imports MQ2Main.dll and eqlib.dll, so only plugins link it;
// mq-emu's tests use fakes instead. Every function checks its window for null
// and the kind of window it needs, and answers "nothing" otherwise.
//
// No C++ exception may cross into Rust: every entry point catches them.

#include "mq_api.h"
#include "window_hooks.h"

#include "main/MQ2Main.h"
#include "eqlib/game/CXWnd.h"
#include "eqlib/game/Globals.h"
#include "eqlib/game/UI.h"

#include <algorithm>
#include <cstring>
#include <iterator>
#include <vector>

using namespace eqlib;

// Mirrors `mqemu_rect` in mq-emu-sys's windows module.
struct mqemu_rect
{
	int32_t left;
	int32_t top;
	int32_t right;
	int32_t bottom;
};

namespace {

// MacroQuest's click names (MQ2Windows.cpp's szClickNotification), in the
// order mqemu_window_click numbers them.
const char* const kClickNames[] = {
	"leftmouse",
	"leftmouseup",
	"leftmouseheld",
	"leftmouseheldup",
	"rightmouse",
	"rightmouseup",
	"rightmouseheld",
	"rightmouseheldup",
};

// The notifications /notify sends by name (MQ2Windows.cpp's
// szWndNotification), except `link`, whose data is text.
bool is_notify_message(uint32_t message)
{
	switch (message)
	{
	case XWM_LCLICK:
	case XWM_LMOUSEUP:
	case XWM_RCLICK:
	case XWM_HITENTER:
	case XWM_QMARKBOX:
	case XWM_CLOSE:
	case XWM_NEWVALUE:
	case XWM_MENUSELECT:
	case XWM_MOUSEOVER:
	case XWM_HISTORY:
	case XWM_LCLICKHOLD:
	case XWM_MAXIMIZEBOX:
		return true;
	default:
		return false;
	}
}

// Copies `text` into `out` (cut to fit) and returns its whole length.
int copy_out(const CXStr& text, char* out, size_t cap)
{
	const char* chars = text.c_str();
	size_t len = std::strlen(chars);
	if (cap > 0)
	{
		size_t n = std::min(len, cap - 1);
		std::memcpy(out, chars, n);
		out[n] = 0;
	}
	return static_cast<int>(len);
}

int none(char* out, size_t cap)
{
	if (cap > 0)
		out[0] = 0;
	return -1;
}

} // namespace

extern "C" CXWnd* mqemu_find_window(const char* name)
{
	try
	{
		if (name == nullptr || name[0] == 0 || std::strchr(name, '/'))
			return nullptr;
		return mq::FindMQ2Window(name);
	}
	catch (...)
	{
		return nullptr;
	}
}

extern "C" int mqemu_window_type(CXWnd* window)
{
	try
	{
		if (window == nullptr || !pSidlMgr)
			return UI_Unknown;
		return window->GetType();
	}
	catch (...)
	{
		return UI_Unknown;
	}
}

extern "C" int mqemu_window_xml_text(CXWnd* window, int which, char* out, size_t cap)
{
	try
	{
		if (window == nullptr || !pSidlMgr)
			return none(out, cap);
		CXMLData* xml = window->GetXMLData();
		if (xml == nullptr)
			return none(out, cap);
		// ${Window.Name}, .ScreenID and .Type, which read nothing for empty names.
		CXStr text;
		switch (which)
		{
		case 0: text = xml->Name; break;
		case 1: text = xml->ScreenID; break;
		case 2: text = xml->TypeName; break;
		default: return none(out, cap);
		}
		return text.empty() ? none(out, cap) : copy_out(text, out, cap);
	}
	catch (...)
	{
		return none(out, cap);
	}
}

extern "C" int mqemu_window_tooltip(CXWnd* window, char* out, size_t cap)
{
	try
	{
		if (window == nullptr)
			return none(out, cap);
		return copy_out(window->GetTooltip(), out, cap);
	}
	catch (...)
	{
		return none(out, cap);
	}
}

extern "C" bool mqemu_window_screen_rect(CXWnd* window, mqemu_rect* out)
{
	*out = {};
	try
	{
		if (window == nullptr)
			return false;
		CXRect rect = window->GetScreenRect();
		*out = { rect.left, rect.top, rect.right, rect.bottom };
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_window_click(CXWnd* window, int click)
{
	try
	{
		if (window == nullptr || click < 0 || click >= static_cast<int>(std::size(kClickNames)))
			return false;
		return mq::SendWndClick2(window, kClickNames[click]);
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_window_notify(CXWnd* window, CXWnd* sender, uint32_t message, int data)
{
	try
	{
		if (window == nullptr || !is_notify_message(message))
			return false;
		// /notify casts any control to a slider here; only a slider is one.
		if (message == XWM_NEWVALUE && sender != nullptr && sender->GetType() == UI_Slider)
			static_cast<CSliderWnd*>(sender)->SetValue(data);
#pragma warning(suppress : 4312)
		window->WndNotification(sender, message, reinterpret_cast<void*>(data));
		mq::WeDidStuff();
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_window_show(CXWnd* window, bool show)
{
	try
	{
		if (window == nullptr)
			return false;
		window->Show(show, true);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_window_move(CXWnd* window, const mqemu_rect* rect)
{
	try
	{
		if (window == nullptr)
			return false;
		window->Move(CXRect(rect->left, rect->top, rect->right, rect->bottom), true, true, true, true);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_window_refade(CXWnd* window)
{
	try
	{
		if (window == nullptr)
			return false;
		window->Refade();
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_window_set_text(CXWnd* window, const char* text)
{
	try
	{
		if (window == nullptr || window->GetType() != UI_EditBox)
			return false;
		CEditWnd* edit = static_cast<CEditWnd*>(window);
		edit->SetWindowText(text);
		edit->ParentWndNotification(edit, XWM_NEWVALUE, nullptr);
		mq::WeDidStuff();
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" int mqemu_list_item_text(CListWnd* list, int row, int column, char* out, size_t cap)
{
	try
	{
		if (list == nullptr || row < 0 || row >= list->ItemsArray.GetCount() || column < 0 || column >= list->Columns.GetCount())
			return none(out, cap);
		return copy_out(list->GetItemText(row, column), out, cap);
	}
	catch (...)
	{
		return none(out, cap);
	}
}

extern "C" bool mqemu_list_select(CXWnd* window, int row)
{
	try
	{
		if (window == nullptr || row < 0)
			return false;
		CListWnd* list = nullptr;
		switch (window->GetType())
		{
		case UI_Listbox:
		case UI_TreeView:
			list = static_cast<CListWnd*>(window);
			break;
		case UI_Combobox:
			list = static_cast<CComboWnd*>(window)->pListWnd;
			break;
		default:
			return false;
		}
		// SendListSelect2 checks the end but not the start, and reports
		// failures as macro errors; neither is wanted here.
		if (list == nullptr || row >= list->ItemsArray.GetCount())
			return false;
		return mq::SendListSelect2(window, row);
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_tab_select(CTabWnd* tab, int page)
{
	try
	{
		if (tab == nullptr || tab->GetType() != UI_TabBox || page < 0 || page >= tab->PageArray.GetCount())
			return false;
		tab->SetPage(page, true);
		mq::WeDidStuff();
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_menu_select(CContextMenu* menu, int item)
{
	try
	{
		if (menu == nullptr || !pContextMenuManager || item < 0 || item >= menu->ItemsArray.GetCount())
			return false;
#pragma warning(suppress : 4312)
		pContextMenuManager->WndNotification(menu, XWM_LMOUSEUP, reinterpret_cast<void*>(item));
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_menus_close()
{
	try
	{
		if (!pContextMenuManager)
			return false;
		pContextMenuManager->Flush();
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_window_notify_link(CXWnd* window, CXWnd* sender, const char* text)
{
	try
	{
		if (window == nullptr || text == nullptr)
			return false;
		// As /notify <window> <control> link <text> sends it: the text itself.
		window->WndNotification(sender, XWM_LINK, const_cast<char*>(text));
		mq::WeDidStuff();
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// ----- item slots -----

extern "C" ItemClient* mqemu_inv_slot_item(CInvSlotWnd* slot)
{
	try
	{
		if (slot == nullptr || slot->GetType() != UI_InvSlot)
			return nullptr;
		// ${Window.InvSlot.Item}: the item the slot is linked to (a hot
		// button's), else what's at its location. The item outlives the
		// ItemPtr: its container (or the slot) holds it.
		if (slot->LinkedItem)
			return slot->LinkedItem.get();
		if (!pLocalPC)
			return nullptr;
		return pLocalPC->GetItemByGlobalIndex(slot->ItemLocation).get();
	}
	catch (...)
	{
		return nullptr;
	}
}

extern "C" int mqemu_inv_slot_text(CInvSlotWnd* slot, char* out, size_t cap)
{
	try
	{
		if (slot == nullptr || slot->GetType() != UI_InvSlot || slot->pTextObject == nullptr)
			return none(out, cap);
		return copy_out(slot->pTextObject->GetText(), out, cap);
	}
	catch (...)
	{
		return none(out, cap);
	}
}

extern "C" int mqemu_texture_animation_name(CTextureAnimation* animation, char* out, size_t cap)
{
	try
	{
		if (animation == nullptr)
			return none(out, cap);
		return copy_out(animation->GetName(), out, cap);
	}
	catch (...)
	{
		return none(out, cap);
	}
}

// ----- hot buttons -----

extern "C" ItemClient* mqemu_hot_button_item(CHotButton* button)
{
	try
	{
		if (button == nullptr || button->GetType() != UI_HotButton)
			return nullptr;
		const HotButtonData* data = button->GetHotButtonData();
		return data ? data->Item.get() : nullptr;
	}
	catch (...)
	{
		return nullptr;
	}
}

extern "C" bool mqemu_hot_button_activate(CHotButton* button)
{
	try
	{
		if (button == nullptr || button->GetType() != UI_HotButton)
			return false;
		// ${Window.HotButton.Activate}: the bar's window presses it, as its key would.
		char name[64];
		if (button->BarIndex > 0)
			sprintf_s(name, "HotButtonWnd%d", button->BarIndex + 1);
		else
			strcpy_s(name, "HotButtonWnd");
		CHotButtonWnd* bar = static_cast<CHotButtonWnd*>(mq::FindMQ2Window(name));
		if (bar == nullptr)
			return false;
		bar->DoHotButton(button->ButtonIndex, false);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// ----- a plugin's own windows -----

extern "C" bool mqemu_add_xml_file(const char* filename)
{
	try
	{
		if (filename == nullptr || filename[0] == 0)
			return false;
		mq::AddXMLFile(filename);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_remove_xml_file(const char* filename)
{
	try
	{
		if (filename == nullptr)
			return false;
		mq::RemoveXMLFile(filename);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_screen_piece_loaded(const char* name)
{
	try
	{
		return name != nullptr && mq::IsScreenPieceLoaded(name);
	}
	catch (...)
	{
		return false;
	}
}

// ----- a plugin's own windows -----

namespace {

using namespace mqemu;

// A window made from a screen piece, as MacroQuest's CCustomWnd makes one,
// that passes what it gets to the plugin before handling it as any screen
// does, and lets the plugin draw on it after it draws. Its vtable lives in
// the plugin's DLL, so the plugin must delete it before it unloads.
class PluginWnd : public CSidlScreenWnd
{
public:
	PluginWnd(const char* screen, const HookTarget& target)
		: CSidlScreenWnd(nullptr, screen, eIniFlag_All)
		, m_target(target)
	{
		CreateChildrenFromSidl();
		Show(true);
		SetEscapable(false);
	}

	int WndNotification(CXWnd* sender, uint32_t message, void* data) override
	{
		if (m_target.Send(this, notification_event(sender, message, data)))
			return 0;
		return CSidlScreenWnd::WndNotification(sender, message, data);
	}

	int PostDraw() override
	{
		int result = CSidlScreenWnd::PostDraw();
		m_target.Send(this, draw_event());
		return result;
	}

	int HandleLButtonDown(const CXPoint& pos, uint32_t flags) override
	{
		if (m_target.Send(this, mouse_event(Hook_LButtonDown, pos, flags)))
			return 0;
		return CSidlScreenWnd::HandleLButtonDown(pos, flags);
	}

	int HandleLButtonUp(const CXPoint& pos, uint32_t flags) override
	{
		if (m_target.Send(this, mouse_event(Hook_LButtonUp, pos, flags)))
			return 0;
		return CSidlScreenWnd::HandleLButtonUp(pos, flags);
	}

	int HandleRButtonDown(const CXPoint& pos, uint32_t flags) override
	{
		if (m_target.Send(this, mouse_event(Hook_RButtonDown, pos, flags)))
			return 0;
		return CSidlScreenWnd::HandleRButtonDown(pos, flags);
	}

	int HandleRButtonUp(const CXPoint& pos, uint32_t flags) override
	{
		if (m_target.Send(this, mouse_event(Hook_RButtonUp, pos, flags)))
			return 0;
		return CSidlScreenWnd::HandleRButtonUp(pos, flags);
	}

	int HandleMouseMove(const CXPoint& pos, uint32_t flags) override
	{
		if (m_target.Send(this, mouse_event(Hook_MouseMove, pos, flags)))
			return 0;
		return CSidlScreenWnd::HandleMouseMove(pos, flags);
	}

	int HandleWheelMove(const CXPoint& pos, int scroll, uint32_t flags) override
	{
		if (m_target.Send(this, mouse_event(Hook_Wheel, pos, flags, scroll)))
			return 0;
		return CSidlScreenWnd::HandleWheelMove(pos, scroll, flags);
	}

private:
	HookTarget m_target;
};

} // namespace

extern "C" CXWnd* mqemu_plugin_window_create(const char* screen, mqemu_window_hook_fn hook, void* user, uint32_t mask)
{
	try
	{
		if (screen == nullptr || !pSidlMgr || !mq::IsScreenPieceLoaded(screen))
			return nullptr;
		return new PluginWnd(screen, HookTarget{ hook, user, mask });
	}
	catch (...)
	{
		return nullptr;
	}
}

extern "C" bool mqemu_plugin_window_destroy(CXWnd* window)
{
	try
	{
		if (window == nullptr)
			return false;
		delete window;
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// ----- drawing on a window -----

namespace {

CXRect to_cxrect(const mqemu_rect& rect)
{
	return CXRect(rect.left, rect.top, rect.right, rect.bottom);
}

CTextureFont* eq_font(int font)
{
	if (!pWndMgr || font < 0)
		return nullptr;
	return pWndMgr->GetFont(font);
}

} // namespace

extern "C" bool mqemu_window_client_clip_rect(CXWnd* window, mqemu_rect* out)
{
	try
	{
		if (window == nullptr || out == nullptr)
			return false;
		CXRect rect = window->GetClientClipRect();
		*out = mqemu_rect{ rect.left, rect.top, rect.right, rect.bottom };
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_draw_fill_rect(const mqemu_rect* rect, uint32_t color, const mqemu_rect* clip)
{
	try
	{
		if (rect == nullptr || clip == nullptr)
			return false;
		// eqlib has its address but no definition: a static member, so cdecl.
		if (!CXWnd__DrawColoredRect)
			return false;
		using DrawColoredRectFn = void(__cdecl*)(const CXRect&, COLORREF, const CXRect&);
		reinterpret_cast<DrawColoredRectFn>(CXWnd__DrawColoredRect)(to_cxrect(*rect), color, to_cxrect(*clip));
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_draw_text(int font, const char* text, const mqemu_rect* rect, const mqemu_rect* clip, uint32_t color, uint32_t flags)
{
	try
	{
		CTextureFont* textureFont = eq_font(font);
		if (textureFont == nullptr || text == nullptr || rect == nullptr || clip == nullptr)
			return false;
		textureFont->DrawWrappedText(CXStr(text), to_cxrect(*rect), to_cxrect(*clip), color, static_cast<uint16_t>(flags), 0);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" int mqemu_text_width(int font, const char* text)
{
	try
	{
		CTextureFont* textureFont = eq_font(font);
		if (textureFont == nullptr || text == nullptr)
			return -1;
		return textureFont->GetTextExtent(CXStr(text));
	}
	catch (...)
	{
		return -1;
	}
}

extern "C" int mqemu_font_height(int font)
{
	try
	{
		CTextureFont* textureFont = eq_font(font);
		return textureFont == nullptr ? -1 : textureFont->GetHeight();
	}
	catch (...)
	{
		return -1;
	}
}

extern "C" bool mqemu_draw_texture(CTextureAnimation* animation, const mqemu_rect* rect, const mqemu_rect* clip, uint32_t color, uint32_t color2)
{
	try
	{
		if (animation == nullptr || rect == nullptr || clip == nullptr)
			return false;
		animation->Draw(to_cxrect(*rect), to_cxrect(*clip), color, color2);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// ----- items without their slot windows -----

namespace {

ItemGlobalIndex item_index(int container, int slot, int in_bag)
{
	return ItemGlobalIndex(static_cast<ItemContainerInstance>(container), ItemIndex(static_cast<short>(slot), static_cast<short>(in_bag)));
}

// Puts the cursor's item on the cursor attachment (the icon the mouse
// carries), or takes the attachment down if the cursor is empty, as the
// game does after moving an item to or from the cursor.
//
// eqlib declares CCursorAttachment::AttachToCursor with 14 dwords of
// arguments (an EqItemGuid by value, an item ID, two names, a quantity and
// an icon), but this client's takes 8 and returns with `ret 20h`: overlay,
// background, type, index, the item's GUID by reference, its ID, a name and
// the quantity, as the game's own item slots call it. So it's called here
// through its address; MacroQuest's PickupItem and DropItem call it as
// declared for places inside bags, and return with the stack 24 bytes off.
void update_cursor_attachment()
{
	if (!pCursorAttachment || !pLocalPC)
		return;
	ItemClient* item = mq::FindItemByGlobalIndex(pLocalPC->CreateItemGlobalIndex(InvSlot_Cursor));
	if (item == nullptr)
	{
		pCursorAttachment->Deactivate();
		return;
	}
	if (!CCursorAttachment__AttachToCursor)
		return;
	using AttachToCursorFn = void(__fastcall*)(CCursorAttachment*, void*, CTextureAnimation* overlay, CTextureAnimation* background,
		int type, int index, const EqItemGuid* guid, int itemId, const char* name, int quantity);
	reinterpret_cast<AttachToCursorFn>(CCursorAttachment__AttachToCursor)(pCursorAttachment, nullptr, nullptr, nullptr,
		eCursorAttachment_Item, -1, &item->ItemGUID, item->ID, nullptr, -1);
}

// What PickupItem and DropItem check before moving an item to or from a
// place inside a bag: the bag is a bag (not an item holding an augment),
// and the bank's places need the bank window.
bool can_move_in_bag(const ItemGlobalIndex& index)
{
	if (!pInvSlotMgr || !pLocalPC)
		return false;
	ItemClient* bag = mq::FindItemByGlobalIndex(index.GetParent());
	if (bag == nullptr || !bag->IsContainer())
		return false;
	switch (index.GetLocation())
	{
	case eItemContainerPossessions:
		return index.GetTopSlot() != InvSlot_Cursor;
	case eItemContainerBank:
	case eItemContainerSharedBank:
		return pBankWnd && pBankWnd->IsVisible();
	default:
		return false;
	}
}

bool merchant_open()
{
	return pMerchantWnd && pMerchantWnd->IsVisible();
}

} // namespace

extern "C" bool mqemu_pick_up_item(int container, int slot, int in_bag)
{
	try
	{
		ItemGlobalIndex index = item_index(container, slot, in_bag);
		// Outside bags, and with the merchant window open (where it only
		// selects), PickupItem doesn't reach AttachToCursor.
		if (in_bag < 0 || merchant_open())
			return mq::PickupItem(index);
		if (mq::ItemOnCursor() || mq::FindItemByGlobalIndex(index) == nullptr || !can_move_in_bag(index))
			return false;
		pInvSlotMgr->MoveItem(index, pLocalPC->CreateItemGlobalIndex(InvSlot_Cursor), true, true, false, false);
		update_cursor_attachment();
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_drop_item(int container, int slot, int in_bag)
{
	try
	{
		ItemGlobalIndex index = item_index(container, slot, in_bag);
		if (in_bag < 0 || merchant_open())
			return mq::DropItem(index);
		if (!mq::ItemOnCursor() || !can_move_in_bag(index))
			return false;
		// Swaps with what's there, which then goes on the cursor.
		pInvSlotMgr->MoveItem(pLocalPC->CreateItemGlobalIndex(InvSlot_Cursor), index, true, true, false, false);
		update_cursor_attachment();
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_use_item(int slot, int in_bag)
{
	try
	{
		if (!mq::cmdUseItem || !pLocalPlayer || !mq::FindItemByGlobalIndex(item_index(eItemContainerPossessions, slot, in_bag)))
			return false;
		char line[32] = {};
		sprintf_s(line, "%d %d", slot, in_bag);
		mq::cmdUseItem(pLocalPlayer, line);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_open_container(int container, int slot, int in_bag, bool open)
{
	try
	{
		ItemClient* item = mq::FindItemByGlobalIndex(item_index(container, slot, in_bag));
		if (item == nullptr || !item->IsContainer())
			return false;
		if (!open)
			return mq::CloseContainer(item);
		if (item->Open)
			return true;
		// OpenContainer needs the bag's slot window.
		if (!pInvSlotMgr || !pInvSlotMgr->FindInvSlot(item->GetItemLocation(), false))
			return false;
		mq::OpenContainer(item, false, false);
		return item->Open != 0;
	}
	catch (...)
	{
		return false;
	}
}
