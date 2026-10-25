// A fake MainInterface that records command API calls, so the shim can be
// tested without MacroQuest. Only tests reference it, so it never ends up in
// a plugin.

#include "mq_api.h"

#include <cstring>
#include <map>
#include <string>

using mq::MQPluginHandle;

// Mirrors `FakeLog` in mq-emu-sys's shim module.
struct mqemu_fake_log
{
	int add_count;
	int remove_count;
	int do_count;
	int timed_count;
	bool last_eq;
	bool last_parse;
	bool last_in_game;
	bool last_delayed;
	int last_ms;
	uint64_t last_handle;
	char last_text[256];
};

namespace {

class FakeMainInterface final : public mq::MainInterface
{
public:
	mqemu_fake_log log{};
	std::map<std::string, mq::MQCommandHandler, std::less<>> commands;
	std::map<std::string, std::string, std::less<>> aliases;

	void Record(std::string_view text, const MQPluginHandle& handle)
	{
		size_t n = std::min(text.size(), sizeof(log.last_text) - 1);
		std::memcpy(log.last_text, text.data(), n);
		log.last_text[n] = 0;
		log.last_handle = handle.pluginID;
	}

	// Commands
	bool AddCommand(std::string_view command, mq::MQCommandHandler handler, bool eq, bool parse, bool inGame,
		const MQPluginHandle& pluginHandle) override
	{
		log.add_count++;
		log.last_eq = eq;
		log.last_parse = parse;
		log.last_in_game = inGame;
		Record(command, pluginHandle);
		return commands.emplace(std::string(command), std::move(handler)).second;
	}

	bool RemoveCommand(std::string_view command, const MQPluginHandle& pluginHandle) override
	{
		log.remove_count++;
		Record(command, pluginHandle);
		auto it = commands.find(command);
		if (it == commands.end())
			return false;
		commands.erase(it);
		return true;
	}

	bool IsCommand(std::string_view command) const override { return commands.find(command) != commands.end(); }

	void DoCommand(const char* command, bool delayed, const MQPluginHandle& pluginHandle) override
	{
		log.do_count++;
		log.last_delayed = delayed;
		Record(command, pluginHandle);
	}

	void TimedCommand(const char* command, int msDelay, const MQPluginHandle& pluginHandle) override
	{
		log.timed_count++;
		log.last_ms = msDelay;
		Record(command, pluginHandle);
	}

	bool AddAlias(const std::string& shortCommand, const std::string& longCommand, bool, const MQPluginHandle& pluginHandle) override
	{
		Record(longCommand, pluginHandle);
		return aliases.emplace(shortCommand, longCommand).second;
	}

	bool RemoveAlias(const std::string& shortCommand, const MQPluginHandle& pluginHandle) override
	{
		Record(shortCommand, pluginHandle);
		return aliases.erase(shortCommand) != 0;
	}

	bool IsAlias(const std::string& alias) const override { return aliases.count(alias) != 0; }

	// Everything else is unused by the shim.
	void SendToActor(mq::postoffice::Dropbox*, const mq::postoffice::Address&, const std::string&,
		const mq::postoffice::ResponseCallbackAPI&, const MQPluginHandle&) override {}
	void ReplyToActor(mq::postoffice::Dropbox*, const std::shared_ptr<mq::postoffice::Message>&, const std::string&,
		uint8_t, const MQPluginHandle&) override {}
	mq::postoffice::Dropbox* AddActor(const char*, mq::postoffice::ReceiveCallbackAPI&&, const MQPluginHandle&) override { return nullptr; }
	void RemoveActor(mq::postoffice::Dropbox*&, const MQPluginHandle&) override {}
	bool CreateDetour(uintptr_t, void**, void*, std::string_view, const MQPluginHandle&) override { return false; }
	bool RemoveDetour(uintptr_t, const MQPluginHandle&) override { return false; }
	bool AddPatch(uintptr_t, size_t, std::string_view, const MQPluginHandle&) override { return false; }
	bool AddPatch(uintptr_t, const uint8_t*, size_t, const uint8_t*, std::string_view, const MQPluginHandle&) override { return false; }
	bool RemovePatch(uintptr_t, const MQPluginHandle&) override { return false; }
	bool AddTopLevelObject(const char*, mq::MQTopLevelObjectFunction, const MQPluginHandle&) override { return false; }
	bool RemoveTopLevelObject(const char*, const MQPluginHandle&) override { return false; }
	mq::MQTopLevelObject* FindTopLevelObject(const char*) override { return nullptr; }
};

} // namespace

extern "C" mq::MainInterface* mqemu_test_fake_new() { return new FakeMainInterface(); }

extern "C" void mqemu_test_fake_free(mq::MainInterface* fake) { delete static_cast<FakeMainInterface*>(fake); }

extern "C" const mqemu_fake_log* mqemu_test_fake_log(mq::MainInterface* fake)
{
	return &static_cast<FakeMainInterface*>(fake)->log;
}

// Invokes a registered command the way MacroQuest would. False if unknown.
extern "C" bool mqemu_test_fake_invoke(mq::MainInterface* fake, const char* name, eqlib::PlayerClient* player, const char* line)
{
	auto& commands = static_cast<FakeMainInterface*>(fake)->commands;
	auto it = commands.find(std::string_view(name));
	if (it == commands.end())
		return false;
	it->second(player, line);
	return true;
}
