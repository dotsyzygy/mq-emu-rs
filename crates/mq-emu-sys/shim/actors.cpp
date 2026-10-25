// C wrappers around MacroQuest's actor ("post office") API: mailboxes,
// sending, requests with replies. Messages reach other MacroQuest instances
// (other characters) through the launcher.
//
// Callbacks carry a Rust `user` pointer inside a shared_ptr whose deleter
// calls back into Rust, so Rust state is freed exactly when MacroQuest
// destroys the callback, never while C++ might still call it.
//
// No C++ exception may cross into Rust: every entry point catches them.

#include "mq_api.h"

#include <cstring>

using mq::MainInterface;
using mq::MQPluginHandle;
namespace po = mq::postoffice;

// Mirrors `mqemu_address` in mq-emu-sys's shim module. Strings are
// NUL-terminated and null when absent.
struct mqemu_address
{
	const char* uuid;
	const char* peer_ip;
	const char* name;
	const char* mailbox;
	const char* account;
	const char* server;
	const char* character;
	uint32_t pid;
	uint16_t peer_port;
	bool has_pid;
	bool has_peer_port;
	bool absolute_mailbox;
};

// A received message, kept alive (and repliable) until freed from Rust.
struct mqemu_message
{
	std::shared_ptr<po::Message> message;
};

using mqemu_drop_fn = void (*)(void* user);
using mqemu_receive_fn = void (*)(void* user, mqemu_message* message);
using mqemu_response_fn = void (*)(void* user, int status, mqemu_message* message);

namespace {

// Frees the Rust side of a callback when the last copy of it goes away.
std::shared_ptr<void> hold(void* user, mqemu_drop_fn drop)
{
	return std::shared_ptr<void>(user, [drop](void* p) { if (drop) drop(p); });
}

std::optional<std::string> text(const char* s)
{
	return s ? std::make_optional(std::string(s)) : std::nullopt;
}

po::Address to_address(const mqemu_address& a)
{
	po::Address address;
	address.UUID = text(a.uuid);
	if (a.has_pid)
		address.PID = a.pid;
	if (a.peer_ip)
		address.Peer = po::Peer{ a.peer_ip, a.has_peer_port ? std::make_optional(a.peer_port) : std::nullopt };
	address.Name = text(a.name);
	address.Mailbox = text(a.mailbox);
	address.Account = text(a.account);
	address.Server = text(a.server);
	address.Character = text(a.character);
	address.AbsoluteMailbox = a.absolute_mailbox;
	return address;
}

const char* c_str(const std::optional<std::string>& s)
{
	return s ? s->c_str() : nullptr;
}

} // namespace

// Registers a mailbox at `local_address` (MacroQuest's plugin API uses
// "Plugin" or "Plugin:mailbox"). Returns the dropbox, or null on failure;
// `drop(user)` runs when MacroQuest destroys the handler.
extern "C" po::Dropbox* mqemu_add_actor(MainInterface* main, const MQPluginHandle* handle, const char* local_address,
	mqemu_receive_fn receive, void* user, mqemu_drop_fn drop)
{
	auto held = hold(user, drop);
	try
	{
		po::ReceiveCallbackAPI handler = [receive, held](const std::shared_ptr<po::Message>& message) {
			receive(held.get(), new mqemu_message{ message });
		};
		return main->AddActor(local_address, std::move(handler), *handle);
	}
	catch (...)
	{
		return nullptr;
	}
}

extern "C" bool mqemu_remove_actor(MainInterface* main, const MQPluginHandle* handle, po::Dropbox* dropbox)
{
	try
	{
		main->RemoveActor(dropbox, *handle);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// Sends `data` to `address`. With a `respond` callback (which needs a
// `dropbox` to reply to) it's a request: `respond` runs once with the reply's
// status (>= 0) or a routing failure (< 0, see ResponseStatus).
extern "C" bool mqemu_send(MainInterface* main, const MQPluginHandle* handle, po::Dropbox* dropbox,
	const mqemu_address* address, const char* data, size_t data_len,
	mqemu_response_fn respond, void* user, mqemu_drop_fn drop)
{
	auto held = hold(user, drop);
	try
	{
		po::ResponseCallbackAPI callback = nullptr;
		if (respond)
		{
			callback = [respond, held](int status, const std::shared_ptr<po::Message>& message) {
				respond(held.get(), status, message ? new mqemu_message{ message } : nullptr);
			};
		}
		main->SendToActor(dropbox, to_address(*address), std::string(data, data_len), callback, *handle);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// Replies to a received message through the dropbox that received it.
extern "C" bool mqemu_reply(MainInterface* main, const MQPluginHandle* handle, po::Dropbox* dropbox,
	const mqemu_message* message, const char* data, size_t data_len, uint8_t status)
{
	try
	{
		main->ReplyToActor(dropbox, message->message, std::string(data, data_len), status, *handle);
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" void mqemu_message_free(mqemu_message* message)
{
	delete message;
}

// The payload, if any. Valid while the message is alive.
extern "C" bool mqemu_message_payload(const mqemu_message* message, const char** data, size_t* len)
{
	if (!message->message || !message->message->Payload)
		return false;
	*data = message->message->Payload->data();
	*len = message->message->Payload->size();
	return true;
}

// The sender, if known. String pointers stay valid while the message is alive.
extern "C" bool mqemu_message_sender(const mqemu_message* message, mqemu_address* out)
{
	std::memset(out, 0, sizeof(*out));
	if (!message->message || !message->message->Sender)
		return false;
	const po::Address& s = *message->message->Sender;
	out->uuid = c_str(s.UUID);
	out->name = c_str(s.Name);
	out->mailbox = c_str(s.Mailbox);
	out->account = c_str(s.Account);
	out->server = c_str(s.Server);
	out->character = c_str(s.Character);
	if (s.Peer)
	{
		out->peer_ip = s.Peer->IP.c_str();
		out->has_peer_port = s.Peer->Port.has_value();
		out->peer_port = s.Peer->Port.value_or(0);
	}
	out->has_pid = s.PID.has_value();
	out->pid = s.PID.value_or(0);
	out->absolute_mailbox = s.AbsoluteMailbox;
	return true;
}

// For tests: the struct's size as C++ sees it, to compare with Rust's.
extern "C" size_t mqemu_address_size()
{
	return sizeof(mqemu_address);
}

// The plugin's name as MacroQuest knows it (the DLL name without "MQ2"/"MQ"
// and ".dll"), which it uses to name and address the plugin's mailboxes.
// Valid while the plugin is loaded; null for a null plugin.
extern "C" const char* mqemu_plugin_name(const mq::MQPlugin* plugin)
{
	return plugin ? plugin->name.c_str() : nullptr;
}
