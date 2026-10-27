// What a plugin hooks in a window: the events a plugin's own window
// (windows.cpp) or a subclassed game window (subclass.cpp) passes on, and
// to whom.

#pragma once

#include "eqlib/game/CXWnd.h"

#include <cstdint>

// Mirrors `mqemu_window_event` in mq-emu-sys's windows module.
struct mqemu_window_event
{
	int32_t kind;
	// For a notification: the control that sent it (or null), the message
	// and its data.
	eqlib::CXWnd* sender;
	uint32_t message;
	void* data;
	// For the mouse: where it is on screen, the keyboard flags, and for the
	// wheel how far it turned.
	int32_t x;
	int32_t y;
	uint32_t flags;
	int32_t scroll;
};

// Mirrors `mqemu_window_hook_fn` in mq-emu-sys's windows module.
using mqemu_window_hook_fn = bool (*)(void* user, eqlib::CXWnd* window, const mqemu_window_event* event);

namespace mqemu {

// Mirror the HOOK_* constants in mq-emu-sys's windows module.
enum HookKind : int32_t
{
	Hook_Notification = 0,
	Hook_Draw,
	Hook_LButtonDown,
	Hook_LButtonUp,
	Hook_RButtonDown,
	Hook_RButtonUp,
	Hook_MouseMove,
	Hook_Wheel,
	Hook_Count,
};

// Who gets a window's events: the plugin's function, its data, and a bit
// per kind it wants.
struct HookTarget
{
	mqemu_window_hook_fn hook = nullptr;
	void* user = nullptr;
	uint32_t mask = 0;

	bool Wants(HookKind kind) const { return hook != nullptr && (mask & (1u << kind)) != 0; }

	// True if the plugin handled it, so the window shouldn't.
	bool Send(eqlib::CXWnd* window, const mqemu_window_event& event) const
	{
		return Wants(static_cast<HookKind>(event.kind)) && hook(user, window, &event);
	}
};

inline mqemu_window_event notification_event(eqlib::CXWnd* sender, uint32_t message, void* data)
{
	mqemu_window_event event = {};
	event.kind = Hook_Notification;
	event.sender = sender;
	event.message = message;
	event.data = data;
	return event;
}

inline mqemu_window_event mouse_event(HookKind kind, const eqlib::CXPoint& pos, uint32_t flags, int scroll = 0)
{
	mqemu_window_event event = {};
	event.kind = kind;
	event.x = pos.x;
	event.y = pos.y;
	event.flags = flags;
	event.scroll = scroll;
	return event;
}

inline mqemu_window_event draw_event()
{
	mqemu_window_event event = {};
	event.kind = Hook_Draw;
	return event;
}

} // namespace mqemu
