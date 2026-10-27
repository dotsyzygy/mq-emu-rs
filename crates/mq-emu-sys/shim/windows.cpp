// EverQuest's own UI windows: finding them as MacroQuest does, what only the
// game's functions can say about them (their kind, XML names, tooltip, list
// text), and clicking and notifying them as /notify and ${Window} do.
//
// This file imports MQ2Main.dll and eqlib.dll, so only plugins link it;
// mq-emu's tests use fakes instead. Every function checks its window for null
// and the kind of window it needs, and answers "nothing" otherwise.
//
// No C++ exception may cross into Rust: every entry point catches them.

#include "mq_api.h"

#include "main/MQ2Main.h"
#include "eqlib/game/CXWnd.h"
#include "eqlib/game/Globals.h"
#include "eqlib/game/UI.h"

#include <algorithm>
#include <cstring>
#include <iterator>

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
