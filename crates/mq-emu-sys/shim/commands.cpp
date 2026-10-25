// C wrappers around MacroQuest's C++ command API (mq/api/Main.h).
//
// MacroQuest reaches commands through its MainInterface, whose AddCommand
// takes a std::function that Rust can't construct. These wrappers build it
// here and expose plain C functions instead. Every function takes the
// MainInterface explicitly, so tests can pass a fake (see test_fake.cpp).
//
// No C++ exception may cross into Rust: every entry point catches them.

#include "mq_api.h"

#include <string_view>

using mq::MainInterface;
using mq::MQPluginHandle;

// A command handler with a user pointer, so Rust can route to a closure.
using mqemu_command_fn = void (*)(void* user, eqlib::PlayerClient* player, const char* line);

extern "C" bool mqemu_add_command(MainInterface* main, const MQPluginHandle* handle,
	const char* name, size_t name_len, mqemu_command_fn callback, void* user,
	bool eq, bool parse, bool in_game)
{
	try
	{
		// Two pointers: small enough for std::function's inline storage, so no
		// heap memory crosses between our C runtime and MacroQuest's.
		mq::MQCommandHandler handler = [callback, user](eqlib::PlayerClient* player, const char* line) {
			callback(user, player, line);
		};
		return main->AddCommand(std::string_view(name, name_len), std::move(handler), eq, parse, in_game, *handle);
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_remove_command(MainInterface* main, const MQPluginHandle* handle,
	const char* name, size_t name_len)
{
	try
	{
		return main->RemoveCommand(std::string_view(name, name_len), *handle);
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_is_command(MainInterface* main, const char* name, size_t name_len)
{
	try
	{
		return main->IsCommand(std::string_view(name, name_len));
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_do_command(MainInterface* main, const MQPluginHandle* handle,
	const char* line, bool delayed)
{
	try
	{
		main->DoCommand(line, delayed, *handle);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_timed_command(MainInterface* main, const MQPluginHandle* handle,
	const char* line, int ms_delay)
{
	try
	{
		main->TimedCommand(line, ms_delay, *handle);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_add_alias(MainInterface* main, const MQPluginHandle* handle,
	const char* short_command, const char* long_command, bool persist)
{
	try
	{
		return main->AddAlias(short_command, long_command, persist, *handle);
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_remove_alias(MainInterface* main, const MQPluginHandle* handle, const char* short_command)
{
	try
	{
		return main->RemoveAlias(short_command, *handle);
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_is_alias(MainInterface* main, const char* alias)
{
	try
	{
		return main->IsAlias(alias);
	}
	catch (...)
	{
		return false;
	}
}
