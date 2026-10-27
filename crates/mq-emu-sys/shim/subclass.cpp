// Subclassing the game's windows. As MacroQuest's WindowOverride does, a
// subclassed window gets a copy of its vtable with some entries replaced. Each replaced entry points at a stub of
// our own, in memory of its own, that loads its subclass into EDX (which
// __thiscall leaves unused) and jumps to a __fastcall function here, whose
// second argument EDX is: so the function knows its subclass whatever
// window it's called for, even through a copy of our table another hook
// made.
//
// Hooks can stack: a window's vtable may be another's copy when we copy it,
// and another hook (a C++ plugin's WindowOverride, another Rust plugin) may
// copy ours. We put the window's old vtable back only if ours is still the
// one it has; otherwise the stubs are rewritten to jump straight to the
// functions they replaced and left in place (with the table), so whatever
// copied them keeps working after this plugin unloads.
//
// Nothing here imports from MQ2Main.dll or eqlib.dll, so tests can link and
// exercise it with a fake window and vtable.
//
// No C++ exception may cross into Rust: every entry point catches them.

#include "mq_api.h"
#include "window_hooks.h"

#include <windows.h>

#include <algorithm>
#include <cstddef>
#include <cstring>
#include <iterator>
#include <vector>

using namespace eqlib;
using namespace mqemu;

namespace {

struct Subclass;

// The vtable entries a subclass replaces.
constexpr size_t kNotificationSlot = offsetof(CXWnd::VirtualFunctionTable, WndNotification) / sizeof(void*);
constexpr size_t kPostDrawSlot = offsetof(CXWnd::VirtualFunctionTable, PostDraw) / sizeof(void*);
constexpr size_t kLButtonDownSlot = offsetof(CXWnd::VirtualFunctionTable, HandleLButtonDown) / sizeof(void*);
constexpr size_t kLButtonUpSlot = offsetof(CXWnd::VirtualFunctionTable, HandleLButtonUp) / sizeof(void*);
constexpr size_t kRButtonDownSlot = offsetof(CXWnd::VirtualFunctionTable, HandleRButtonDown) / sizeof(void*);
constexpr size_t kRButtonUpSlot = offsetof(CXWnd::VirtualFunctionTable, HandleRButtonUp) / sizeof(void*);
constexpr size_t kMouseMoveSlot = offsetof(CXWnd::VirtualFunctionTable, HandleMouseMove) / sizeof(void*);
constexpr size_t kWheelSlot = offsetof(CXWnd::VirtualFunctionTable, HandleWheelMove) / sizeof(void*);

// Entries copied from the window's vtable: as many as MacroQuest's
// WindowOverride copies, more than any window class has.
constexpr size_t kTableEntries = 1024;
// mov edx, imm32 (5 bytes); jmp rel32 (5 bytes).
constexpr size_t kStubSize = 10;

using NotificationFn = int(__fastcall*)(CXWnd*, Subclass*, CXWnd*, uint32_t, void*);
using DrawFn = int(__fastcall*)(CXWnd*, Subclass*);
using MouseFn = int(__fastcall*)(CXWnd*, Subclass*, const CXPoint&, uint32_t);
using WheelFn = int(__fastcall*)(CXWnd*, Subclass*, const CXPoint&, int, uint32_t);

struct Subclass
{
	CXWnd* window = nullptr;
	HookTarget target;
	// The window's vtable before.
	void** original = nullptr;
	// Our copy: entries from table[0], with the RTTI pointer before them.
	void** block = nullptr;
	void** table = nullptr;
	// One stub per replaced entry, executable.
	uint8_t* stubs = nullptr;

	template <typename Fn>
	Fn Original(size_t slot) const { return reinterpret_cast<Fn>(original[slot]); }
};

int __fastcall subclass_notification(CXWnd* self, Subclass* sub, CXWnd* sender, uint32_t message, void* data)
{
	// Taken first: the plugin may remove the subclass from its handler.
	NotificationFn original = sub->Original<NotificationFn>(kNotificationSlot);
	if (self == sub->window && sub->target.Send(self, notification_event(sender, message, data)))
		return 0;
	return original(self, nullptr, sender, message, data);
}

int __fastcall subclass_post_draw(CXWnd* self, Subclass* sub)
{
	HookTarget target = sub->target;
	bool mine = self == sub->window;
	int result = sub->Original<DrawFn>(kPostDrawSlot)(self, nullptr);
	if (mine)
		target.Send(self, draw_event());
	return result;
}

template <HookKind Kind, size_t Slot>
int __fastcall subclass_mouse(CXWnd* self, Subclass* sub, const CXPoint& pos, uint32_t flags)
{
	MouseFn original = sub->Original<MouseFn>(Slot);
	if (self == sub->window && sub->target.Send(self, mouse_event(Kind, pos, flags)))
		return 0;
	return original(self, nullptr, pos, flags);
}

int __fastcall subclass_wheel(CXWnd* self, Subclass* sub, const CXPoint& pos, int scroll, uint32_t flags)
{
	WheelFn original = sub->Original<WheelFn>(kWheelSlot);
	if (self == sub->window && sub->target.Send(self, mouse_event(Hook_Wheel, pos, flags, scroll)))
		return 0;
	return original(self, nullptr, pos, scroll, flags);
}

struct ReplacedEntry
{
	HookKind kind;
	size_t slot;
	void* function;
};

const ReplacedEntry kReplaced[] = {
	{ Hook_Notification, kNotificationSlot, reinterpret_cast<void*>(&subclass_notification) },
	{ Hook_Draw, kPostDrawSlot, reinterpret_cast<void*>(&subclass_post_draw) },
	{ Hook_LButtonDown, kLButtonDownSlot, reinterpret_cast<void*>(&subclass_mouse<Hook_LButtonDown, kLButtonDownSlot>) },
	{ Hook_LButtonUp, kLButtonUpSlot, reinterpret_cast<void*>(&subclass_mouse<Hook_LButtonUp, kLButtonUpSlot>) },
	{ Hook_RButtonDown, kRButtonDownSlot, reinterpret_cast<void*>(&subclass_mouse<Hook_RButtonDown, kRButtonDownSlot>) },
	{ Hook_RButtonUp, kRButtonUpSlot, reinterpret_cast<void*>(&subclass_mouse<Hook_RButtonUp, kRButtonUpSlot>) },
	{ Hook_MouseMove, kMouseMoveSlot, reinterpret_cast<void*>(&subclass_mouse<Hook_MouseMove, kMouseMoveSlot>) },
	{ Hook_Wheel, kWheelSlot, reinterpret_cast<void*>(&subclass_wheel) },
};
constexpr size_t kReplacedCount = std::size(kReplaced);

// The windows this plugin subclassed.
std::vector<Subclass*>& subclasses()
{
	static std::vector<Subclass*> list;
	return list;
}

Subclass* find_subclass(CXWnd* window)
{
	for (Subclass* sub : subclasses())
	{
		if (sub->window == window)
			return sub;
	}
	return nullptr;
}

// Writes a jump to `to` at `at`, preceded by `mov edx, edx_value` when
// `with_edx` (else the mov is left out and the jump comes first).
void write_stub(uint8_t* at, bool with_edx, void* edx_value, void* to)
{
	uint8_t* p = at;
	if (with_edx)
	{
		*p++ = 0xBA;
		uint32_t value = reinterpret_cast<uint32_t>(edx_value);
		std::memcpy(p, &value, 4);
		p += 4;
	}
	*p++ = 0xE9;
	int32_t rel = static_cast<int32_t>(reinterpret_cast<intptr_t>(to) - reinterpret_cast<intptr_t>(p + 4));
	std::memcpy(p, &rel, 4);
}

bool set_stubs_writable(Subclass* sub, bool writable)
{
	DWORD old = 0;
	size_t size = kStubSize * kReplacedCount;
	if (!VirtualProtect(sub->stubs, size, writable ? PAGE_READWRITE : PAGE_EXECUTE_READ, &old))
		return false;
	if (!writable)
		FlushInstructionCache(GetCurrentProcess(), sub->stubs, size);
	return true;
}

// How many bytes from `at` can be read, up to `want`.
size_t readable_bytes(const void* at, size_t want)
{
	size_t total = 0;
	const uint8_t* p = static_cast<const uint8_t*>(at);
	while (total < want)
	{
		MEMORY_BASIC_INFORMATION info = {};
		if (VirtualQuery(p + total, &info, sizeof(info)) == 0 || info.State != MEM_COMMIT
			|| (info.Protect & (PAGE_NOACCESS | PAGE_GUARD)) != 0)
		{
			break;
		}
		size_t region_end = reinterpret_cast<size_t>(info.BaseAddress) + info.RegionSize;
		total = region_end - reinterpret_cast<size_t>(p);
	}
	return std::min(total, want);
}

void free_subclass(Subclass* sub)
{
	VirtualFree(sub->stubs, 0, MEM_RELEASE);
	delete[] sub->block;
	delete sub;
}

} // namespace

// The vtable entry (counted in pointers) a subclass replaces for hook kind
// `kind`; -1 for none.
extern "C" int mqemu_window_hook_slot(int kind)
{
	for (const ReplacedEntry& entry : kReplaced)
	{
		if (entry.kind == kind)
			return static_cast<int>(entry.slot);
	}
	return -1;
}

extern "C" bool mqemu_window_subclass(CXWnd* window, mqemu_window_hook_fn hook, void* user, uint32_t mask)
{
	try
	{
		if (window == nullptr || hook == nullptr || find_subclass(window) != nullptr)
			return false;
		void** original = *reinterpret_cast<void***>(window);
		if (original == nullptr)
			return false;
		// The whole table, and the RTTI pointer before it, as far as they
		// can be read.
		size_t bytes = readable_bytes(original - 1, (kTableEntries + 1) * sizeof(void*));
		size_t entries = bytes / sizeof(void*);
		if (entries < 1 + sizeof(CXWnd::VirtualFunctionTable) / sizeof(void*))
			return false;

		auto sub = new Subclass;
		sub->window = window;
		sub->target = HookTarget{ hook, user, mask };
		sub->original = original;
		sub->block = new void*[kTableEntries + 1]();
		std::memcpy(sub->block, original - 1, entries * sizeof(void*));
		sub->table = sub->block + 1;
		sub->stubs = static_cast<uint8_t*>(VirtualAlloc(nullptr, kStubSize * kReplacedCount, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE));
		if (sub->stubs == nullptr)
		{
			delete[] sub->block;
			delete sub;
			return false;
		}
		for (size_t i = 0; i < kReplacedCount; ++i)
		{
			uint8_t* stub = sub->stubs + i * kStubSize;
			write_stub(stub, true, sub, kReplaced[i].function);
			sub->table[kReplaced[i].slot] = stub;
		}
		if (!set_stubs_writable(sub, false))
		{
			free_subclass(sub);
			return false;
		}
		subclasses().push_back(sub);
		*reinterpret_cast<void***>(window) = sub->table;
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// Takes a subclass back off. True if the window has its old vtable back;
// false if it wasn't subclassed, or if another hook went on after ours (the
// subclass then only passes everything on, for good). A window that isn't
// `live` (the game deleted it) isn't touched: the subclass only stops.
extern "C" bool mqemu_window_unsubclass(CXWnd* window, bool live)
{
	try
	{
		Subclass* sub = find_subclass(window);
		if (sub == nullptr)
			return false;
		auto& list = subclasses();
		list.erase(std::find(list.begin(), list.end(), sub));
		sub->target = HookTarget{};

		void*** vtable = reinterpret_cast<void***>(window);
		if (live && *vtable == sub->table)
		{
			*vtable = sub->original;
			free_subclass(sub);
			return true;
		}

		// Something copied our table, or put the window's old one back, or
		// the window is gone: make the stubs and our entries the old
		// functions, and keep them.
		for (size_t i = 0; i < kReplacedCount; ++i)
			sub->table[kReplaced[i].slot] = sub->original[kReplaced[i].slot];
		if (set_stubs_writable(sub, true))
		{
			for (size_t i = 0; i < kReplacedCount; ++i)
				write_stub(sub->stubs + i * kStubSize, false, nullptr, sub->original[kReplaced[i].slot]);
			set_stubs_writable(sub, false);
		}
		return false;
	}
	catch (...)
	{
		return false;
	}
}
