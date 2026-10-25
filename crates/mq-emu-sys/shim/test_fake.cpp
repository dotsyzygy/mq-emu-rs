// A fake MainInterface that records command API calls, so the shim can be
// tested without MacroQuest. Only tests reference it, so it never ends up in
// a plugin.

#include "mq_api.h"

#include <cstring>
#include <map>
#include <string>
#include <vector>

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

namespace po = mq::postoffice;

// What the fake post office saw, for tests.
struct FakeSend
{
	bool had_dropbox = false;
	bool had_callback = false;
	po::Address address;
	std::string data;
};

struct FakeReply
{
	po::Dropbox* dropbox = nullptr;
	std::string data;
	uint8_t status = 0;
};

class FakeMainInterface final : public mq::MainInterface
{
public:
	mqemu_fake_log log{};
	// Registered mailboxes, by local address. The Dropbox pointers are opaque
	// tokens here; the fake never dereferences them.
	std::map<std::string, std::pair<po::Dropbox*, po::ReceiveCallbackAPI>, std::less<>> actors;
	std::vector<FakeSend> sent;
	std::vector<po::ResponseCallbackAPI> pending; // requests awaiting a response, oldest first
	std::vector<FakeReply> replies;
	uintptr_t next_dropbox = 0x1000;
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

	// Actors
	void SendToActor(po::Dropbox* dropbox, const po::Address& address, const std::string& data,
		const po::ResponseCallbackAPI& callback, const MQPluginHandle&) override
	{
		sent.push_back(FakeSend{ dropbox != nullptr, callback != nullptr, address, data });
		if (callback)
			pending.push_back(callback);
	}

	void ReplyToActor(po::Dropbox* dropbox, const std::shared_ptr<po::Message>&, const std::string& data,
		uint8_t status, const MQPluginHandle&) override
	{
		replies.push_back(FakeReply{ dropbox, data, status });
	}

	po::Dropbox* AddActor(const char* localAddress, po::ReceiveCallbackAPI&& receive, const MQPluginHandle&) override
	{
		auto* dropbox = reinterpret_cast<po::Dropbox*>(next_dropbox);
		next_dropbox += 0x10;
		actors[localAddress] = { dropbox, std::move(receive) };
		return dropbox;
	}

	void RemoveActor(po::Dropbox*& dropbox, const MQPluginHandle&) override
	{
		for (auto it = actors.begin(); it != actors.end(); ++it)
		{
			if (it->second.first == dropbox)
			{
				actors.erase(it);
				break;
			}
		}
		dropbox = nullptr;
	}

	// Everything else is unused by the shim.
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

static FakeMainInterface* fake_of(mq::MainInterface* fake) { return static_cast<FakeMainInterface*>(fake); }

static std::shared_ptr<po::Message> make_message(const char* sender_character, const char* sender_mailbox,
	const char* data, size_t len)
{
	std::optional<po::Address> sender;
	if (sender_character || sender_mailbox)
	{
		po::Address address;
		if (sender_character)
			address.Character = sender_character;
		if (sender_mailbox)
			address.Mailbox = sender_mailbox;
		address.AbsoluteMailbox = true;
		sender = address;
	}
	std::optional<std::string> payload;
	if (data)
		payload = std::string(data, len);
	return std::make_shared<po::Message>(po::Message{ nullptr, sender, payload });
}

// Delivers a message to the mailbox registered at `local_address`, as the
// post office would. False if no such mailbox. `data` may be null (no payload).
extern "C" bool mqemu_test_fake_deliver(mq::MainInterface* fake, const char* local_address,
	const char* sender_character, const char* sender_mailbox, const char* data, size_t len)
{
	auto& actors = fake_of(fake)->actors;
	auto it = actors.find(std::string_view(local_address));
	if (it == actors.end())
		return false;
	auto receive = it->second.second; // the handler may remove its own mailbox
	receive(make_message(sender_character, sender_mailbox, data, len));
	return true;
}

// Answers the oldest pending request. False if none is pending.
extern "C" bool mqemu_test_fake_respond(mq::MainInterface* fake, int status, const char* data, size_t len)
{
	auto& pending = fake_of(fake)->pending;
	if (pending.empty())
		return false;
	auto callback = std::move(pending.front());
	pending.erase(pending.begin());
	callback(status, make_message("Responder", nullptr, data, len));
	return true;
}

// Drops every pending request without answering, as when a connection dies.
extern "C" void mqemu_test_fake_drop_pending(mq::MainInterface* fake)
{
	fake_of(fake)->pending.clear();
}

extern "C" size_t mqemu_test_fake_actor_count(mq::MainInterface* fake) { return fake_of(fake)->actors.size(); }
extern "C" bool mqemu_test_fake_has_actor(mq::MainInterface* fake, const char* local_address)
{
	return fake_of(fake)->actors.count(local_address) != 0;
}
extern "C" size_t mqemu_test_fake_sent_count(mq::MainInterface* fake) { return fake_of(fake)->sent.size(); }
extern "C" size_t mqemu_test_fake_pending_count(mq::MainInterface* fake) { return fake_of(fake)->pending.size(); }
extern "C" size_t mqemu_test_fake_reply_count(mq::MainInterface* fake) { return fake_of(fake)->replies.size(); }

// The most recent send, as text: "dropbox=<0|1> callback=<0|1> character=<..> mailbox=<..>
// absolute=<0|1> data=<..>" (absent fields shown as "-").
extern "C" size_t mqemu_test_fake_last_sent(mq::MainInterface* fake, char* out, size_t out_len)
{
	auto& sent = fake_of(fake)->sent;
	if (sent.empty() || out_len == 0)
		return 0;
	const FakeSend& s = sent.back();
	auto opt = [](const std::optional<std::string>& v) { return v ? *v : std::string("-"); };
	std::string text = "dropbox=" + std::to_string(s.had_dropbox) + " callback=" + std::to_string(s.had_callback)
		+ " character=" + opt(s.address.Character) + " mailbox=" + opt(s.address.Mailbox)
		+ " absolute=" + std::to_string(s.address.AbsoluteMailbox) + " data=" + s.data;
	size_t n = std::min(text.size(), out_len - 1);
	std::memcpy(out, text.data(), n);
	out[n] = 0;
	return n;
}

// The most recent reply's data and status. False if none.
extern "C" bool mqemu_test_fake_last_reply(mq::MainInterface* fake, char* out, size_t out_len, uint8_t* status)
{
	auto& replies = fake_of(fake)->replies;
	if (replies.empty() || out_len == 0)
		return false;
	const FakeReply& r = replies.back();
	size_t n = std::min(r.data.size(), out_len - 1);
	std::memcpy(out, r.data.data(), n);
	out[n] = 0;
	*status = r.status;
	return true;
}

// The loader keeps the handle in a record like this before passing it on.
struct PluginInfoRecForTest
{
	mq::MQPlugin* instance;
	mq::MQPluginHandle handle;
};

// Calls `init` exactly the way MacroQuest's loader calls a plugin's
// InitPluginHandle (MQPluginHandler.cpp), to check the by-value ABI.
extern "C" void mqemu_test_call_init_plugin_handle(void* init, mq::MQPlugin* plugin, uint64_t plugin_id)
{
	using InitPluginHandleFunc = void(*)(mq::MQPlugin*, mq::MQPluginHandle);
	PluginInfoRecForTest rec{ plugin, mq::MQPluginHandle(plugin_id) };
	auto initPluginFunc = reinterpret_cast<InitPluginHandleFunc>(init);
	initPluginFunc(rec.instance, rec.handle);
}

// An MQPlugin record with the given name, as the loader would make (tests only).
extern "C" mq::MQPlugin* mqemu_test_new_plugin(const char* name)
{
	auto* plugin = new mq::MQPlugin();
	plugin->name = name;
	return plugin;
}

extern "C" void mqemu_test_free_plugin(mq::MQPlugin* plugin)
{
	delete plugin;
}
