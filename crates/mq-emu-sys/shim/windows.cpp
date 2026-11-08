// EverQuest's own UI windows, what the Rust side (windows.rs) doesn't do:
// a plugin's own windows, made from screen pieces in UI XML it adds
// (AddXMLFile), which use MacroQuest's XML machinery. Subclassing is in
// subclass.cpp.
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

using namespace eqlib;

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
