// C wrappers around MacroQuest's macro data API: reading top-level objects
// (${Me.Level}) as typed values, and adding top-level objects and datatypes
// of our own.
//
// This file imports MQ2Main.dll and eqlib.dll (an MQTypeVar can hold an
// eqlib CXStr), so only plugins link it; mq-emu's tests use fakes instead.
//
// No C++ exception may cross into Rust: every entry point catches them.

#include "mq_api.h"

#include "mq/api/MacroAPI.h"
// Macro variables (pGlobalVariables, gMacroStack) and CDataArray.
#include "main/MQ2Main.h"

#include <cerrno>
#include <cstdlib>
#include <cstring>
#include <memory>
#include <string>
#include <type_traits>
#include <variant>

using mq::MainInterface;
using mq::MQPluginHandle;
using mq::MQTypeVar;
using mq::MQVarPtr;
using mq::datatypes::MQ2Type;

// An evaluated value (a TLO's or a member's), owned by Rust until freed.
struct mqemu_data
{
	MQTypeVar var;
	// A string value's text, copied at once: string values point into
	// buffers that the next evaluation reuses.
	std::string text;
	bool has_text = false;
};

// Mirrors `mqemu_scalar` in mq-emu-sys's shim module: the value's raw data
// read as each kind of number. Which one is meaningful depends on its type.
struct mqemu_scalar
{
	int64_t integer;
	double real;
	bool boolean;
};

namespace {

mqemu_data* wrap(MQTypeVar&& var)
{
	if (var.Type == nullptr)
		return nullptr;
	auto* data = new mqemu_data{ std::move(var) };
	if (data->var.Type == mq::FindMQ2DataType("string"))
	{
		char buffer[MAX_STRING] = {};
		if (data->var.Type->ToString(data->var.GetVarPtr(), buffer))
		{
			data->text = buffer;
			data->has_text = true;
		}
	}
	return data;
}

size_t copy_out(const std::string& text, char* out, size_t cap)
{
	if (cap == 0)
		return 0;
	size_t n = std::min(text.size(), cap - 1);
	std::memcpy(out, text.data(), n);
	out[n] = 0;
	return n;
}

} // namespace

// ----- reading ---------------------------------------------------------------

// Evaluates TLO `name` with `index` ("" for none). Null if there's no such
// TLO or it has no value (MacroQuest's NULL).
extern "C" mqemu_data* mqemu_data_tlo(MainInterface* main, const char* name, const char* index)
{
	try
	{
		mq::MQTopLevelObject* tlo = main->FindTopLevelObject(name);
		if (tlo == nullptr || !tlo->Function)
			return nullptr;
		MQTypeVar var;
		if (!tlo->Function(index ? index : "", var))
			return nullptr;
		return wrap(std::move(var));
	}
	catch (...)
	{
		return nullptr;
	}
}

// Evaluates `member` (with `index`, "" for none) of a value. Null if the type
// has no such member or it has no value.
extern "C" mqemu_data* mqemu_data_member(const mqemu_data* data, const char* member, const char* index)
{
	try
	{
		// Members may write to their index, so they get a copy.
		char buffer[MAX_STRING] = {};
		strncpy_s(buffer, index ? index : "", _TRUNCATE);
		MQTypeVar var;
		if (mq::EvaluateMacroDataMember(data->var.Type, data->var.GetVarPtr(), var, member, buffer) != 1)
			return nullptr;
		return wrap(std::move(var));
	}
	catch (...)
	{
		return nullptr;
	}
}

extern "C" void mqemu_data_free(mqemu_data* data)
{
	delete data;
}

// The value's type name ("int", "spawn"...); valid while MacroQuest runs.
extern "C" const char* mqemu_data_type_name(const mqemu_data* data)
{
	try
	{
		return data->var.Type->GetName();
	}
	catch (...)
	{
		return "";
	}
}

// The value as ${...} would show it. Returns the length written.
extern "C" size_t mqemu_data_text(const mqemu_data* data, char* out, size_t cap)
{
	try
	{
		if (data->has_text)
			return copy_out(data->text, out, cap);
		char buffer[MAX_STRING] = {};
		if (!data->var.Type->ToString(data->var.GetVarPtr(), buffer))
			buffer[0] = 0;
		return copy_out(buffer, out, cap);
	}
	catch (...)
	{
		return copy_out("", out, cap);
	}
}

extern "C" void mqemu_data_scalar(const mqemu_data* data, mqemu_scalar* out)
{
	*out = {};
	std::visit([out](const auto& value) {
		using T = std::decay_t<decltype(value)>;
		if constexpr (std::is_same_v<T, bool>)
		{
			*out = { value ? 1 : 0, value ? 1.0 : 0.0, value };
		}
		else if constexpr (std::is_same_v<T, float> || std::is_same_v<T, double>)
		{
			*out = { static_cast<int64_t>(value), static_cast<double>(value), value != 0 };
		}
		else if constexpr (std::is_same_v<T, uint64_t>)
		{
			*out = { static_cast<int64_t>(value), static_cast<double>(static_cast<int64_t>(value)), value != 0 };
		}
		else if constexpr (std::is_same_v<T, void*>)
		{
			auto n = static_cast<int64_t>(reinterpret_cast<intptr_t>(value));
			*out = { n, static_cast<double>(n), value != nullptr };
		}
	}, data->var.Data);
}

// ----- providing -------------------------------------------------------------

// Where a TLO callback puts its value. Opaque to Rust.
struct mqemu_tlo_result
{
	MQTypeVar* dest;
	// The TLO's string storage: a string value points here until the TLO is
	// evaluated again, as MacroQuest's own TLOs do with a shared buffer.
	std::string* text;
};

using mqemu_drop_fn = void (*)(void* user);
using mqemu_tlo_fn = bool (*)(void* user, const char* index, mqemu_tlo_result* result);

namespace {

struct TloState
{
	mqemu_tlo_fn callback;
	std::shared_ptr<void> user;
	std::string text;
};

} // namespace

// Adds TLO `name`. Ownership of `user` passes here: `drop` is called exactly
// once, when MacroQuest lets go of the TLO (or right away if adding fails).
extern "C" bool mqemu_add_tlo(MainInterface* main, const MQPluginHandle* handle, const char* name,
	mqemu_tlo_fn callback, void* user, mqemu_drop_fn drop)
{
	std::shared_ptr<void> held(user, [drop](void* p) { if (drop) drop(p); });
	try
	{
		auto state = std::make_shared<TloState>(TloState{ callback, std::move(held), {} });
		// One shared_ptr: small enough for std::function's inline storage.
		mq::MQTopLevelObjectFunction function = [state](const char* index, MQTypeVar& dest) {
			auto keep = state; // the TLO may be removed while it runs
			mqemu_tlo_result result{ &dest, &keep->text };
			return keep->callback(keep->user.get(), index ? index : "", &result);
		};
		return main->AddTopLevelObject(name, std::move(function), *handle);
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_remove_tlo(MainInterface* main, const MQPluginHandle* handle, const char* name)
{
	try
	{
		return main->RemoveTopLevelObject(name, *handle);
	}
	catch (...)
	{
		return false;
	}
}

// Value kinds for mqemu_tlo_set; mirrors `TloKind` in mq-emu-sys.
enum mqemu_tlo_kind : int
{
	MQEMU_TLO_BOOL = 0,
	MQEMU_TLO_INT = 1,
	MQEMU_TLO_INT64 = 2,
	MQEMU_TLO_FLOAT = 3,
	MQEMU_TLO_STRING = 4,
};

// Sets a TLO callback's value. False if MacroQuest lacks the type.
extern "C" bool mqemu_tlo_set(mqemu_tlo_result* result, int kind, int64_t integer, double real,
	const char* text, size_t len)
{
	try
	{
		static const char* const names[] = { "bool", "int", "int64", "float", "string" };
		if (kind < 0 || kind > MQEMU_TLO_STRING)
			return false;
		MQ2Type* type = mq::FindMQ2DataType(names[kind]);
		if (type == nullptr)
			return false;

		MQTypeVar& dest = *result->dest;
		dest.Type = type;
		switch (kind)
		{
		case MQEMU_TLO_BOOL: dest.Set<bool>(integer != 0); break;
		case MQEMU_TLO_INT: dest.Set<int32_t>(static_cast<int32_t>(integer)); break;
		case MQEMU_TLO_INT64: dest.Set<int64_t>(integer); break;
		case MQEMU_TLO_FLOAT: dest.Set<float>(static_cast<float>(real)); break;
		case MQEMU_TLO_STRING:
			result->text->assign(text ? text : "", text ? len : 0);
			dest.Set<void*>(result->text->data());
			break;
		}
		return true;
	}
	catch (...)
	{
		return false;
	}
}

// ----- datatypes -------------------------------------------------------------
//
// A plugin's own datatype, as C++ plugins derive from MQ2Type: its members,
// methods and text are answered by Rust.
//
// A value of the type is a key the members look up: an integer (in the
// uint64 slot) or text (a CXStr, which eqlib frees). Neither runs plugin code
// when MacroQuest frees the value, which may be after the plugin unloads: a
// Lua script's member objects (mq.TLO.X.Member) wait for garbage collection.
// Using one after that would still call into the unloaded plugin, as with
// any C++ plugin's type.

// Key kinds; mirrors `key_kind` in mq-emu-sys.
enum mqemu_key_kind : int32_t
{
	MQEMU_KEY_INT = 0,
	MQEMU_KEY_TEXT = 1,
};

// Mirrors `mqemu_key` in mq-emu-sys. `text` is valid during the call it's passed to.
struct mqemu_key
{
	int32_t kind;
	int64_t integer;
	const char* text;
	size_t len;
};

using mqemu_member_fn = bool (*)(void* user, const mqemu_key* key, int32_t member, const char* index,
	mqemu_tlo_result* result);
using mqemu_type_text_fn = bool (*)(void* user, const mqemu_key* key, char* out, size_t cap);

namespace {

// Reads a value's key into `key`; text is copied to `text`. False for a value
// that holds no key (a /declare'd variable never set).
bool read_key(const mq::MQVarPtr& var, mqemu_key& key, std::string& text)
{
	if (var.IsType(mq::MQVarPtr::VariantIdx::UInt64))
	{
		key = { MQEMU_KEY_INT, static_cast<int64_t>(std::get<uint64_t>(var.Data)), nullptr, 0 };
		return true;
	}
	if (var.IsType(mq::MQVarPtr::VariantIdx::String))
	{
		text = std::string_view(std::get<eqlib::CXStr>(var.Data));
		key = { MQEMU_KEY_TEXT, 0, text.data(), text.size() };
		return true;
	}
	return false;
}

void write_key(mq::MQVarPtr& var, const mqemu_key& key)
{
	if (key.kind == MQEMU_KEY_TEXT)
		var.SetString(std::string_view(key.text ? key.text : "", key.text ? key.len : 0));
	else
		var.Set<uint64_t>(static_cast<uint64_t>(key.integer));
}

// Turns a variable holding a value of `type` (or an array of them) into a
// string, when the type is removed: deleting a variable calls its type, and
// a variable can outlive the plugin (a global, or a macro's local while the
// macro runs). A value keeps its key, which /varset would take back; an
// array becomes empty.
void detach_variable(MQTypeVar& var, MQ2Type* type)
{
	std::string text;
	if (var.Type == type)
	{
		mqemu_key key;
		if (read_key(var, key, text) && key.kind == MQEMU_KEY_INT)
			text = std::to_string(key.integer);
		type->FreeVariable(var);
	}
	else if (var.Type == mq::datatypes::pArrayType)
	{
		auto array = var.Get<mq::datatypes::CDataArray>();
		if (!array || array->GetType() != type)
			return;
		array.reset();
		// MacroQuest's array frees its elements through `type`, now, while the plugin is loaded.
		var.Set<void*>(nullptr);
	}
	else
	{
		return;
	}
	var.Type = mq::datatypes::pStringType;
	var.Type->InitVariable(var);
	var.Type->FromString(var, text.c_str());
}

void detach_variables(mq::MQDataVar* list, MQ2Type* type)
{
	for (mq::MQDataVar* var = list; var != nullptr; var = var->pNext)
		detach_variable(var->Var, type);
}

// Every variable MacroQuest has: globals, macro-wide (outer) ones, and each
// running macro frame's parameters and locals.
void detach_all_variables(MQ2Type* type)
{
	detach_variables(mq::pGlobalVariables, type);
	detach_variables(mq::pMacroVariables, type);
	for (mq::MQMacroStack* frame = mq::gMacroStack; frame != nullptr; frame = frame->pNext)
	{
		detach_variables(frame->Parameters, type);
		detach_variables(frame->LocalVariables, type);
	}
}

} // namespace

struct mqemu_type final : MQ2Type
{
	mqemu_type(const char* name, const MQPluginHandle& handle, int32_t key_kind, mqemu_member_fn member,
		mqemu_type_text_fn text, std::shared_ptr<void> user)
		: MQ2Type(name, handle), m_keyKind(key_kind), m_member(member), m_text(text), m_user(std::move(user))
	{
	}

	bool Registered() const { return m_owned; }

	bool Add(int32_t id, const char* name, bool method)
	{
		return method ? AddMethod(id, name) : AddMember(id, name);
	}

	// MacroQuest stops finding the type, its variables become strings, and
	// its members answer NULL. Values that still point here (a Lua script's)
	// keep working (as NULL) until it's freed.
	void Unregister()
	{
		if (m_owned)
		{
			mq::RemoveMQ2Type(*this);
			m_owned = false; // so the destructor doesn't remove another type by this name
			detach_all_variables(this);
		}
		m_user.reset();
	}

	bool GetMember(MQVarPtr VarPtr, const char* Member, char* Index, MQTypeVar& Dest) override
	{
		auto keep = m_user; // the type may be removed while its member runs
		mqemu_key key;
		std::string text;
		if (!keep || !read_key(VarPtr, key, text))
			return false;
		const mq::MQTypeMember* found = FindMember(Member);
		if (found == nullptr)
			found = FindMethod(Member);
		if (found == nullptr)
			return false;
		mqemu_tlo_result result{ &Dest, &m_buffer };
		return m_member(keep.get(), &key, found->ID, Index ? Index : "", &result);
	}

	bool ToString(MQVarPtr VarPtr, char* Destination) override
	{
		auto keep = m_user;
		mqemu_key key;
		std::string text;
		if (!keep || !read_key(VarPtr, key, text))
			return false;
		return m_text(keep.get(), &key, Destination, MAX_STRING);
	}

	// /varset of a variable of this type: another value of it, or a key.
	bool FromData(MQVarPtr& VarPtr, const MQTypeVar& Source) override
	{
		if (Source.Type != this)
			return false;
		mqemu_key key;
		std::string text;
		if (!read_key(Source, key, text))
			return false;
		write_key(VarPtr, key);
		return true;
	}

	bool FromString(MQVarPtr& VarPtr, const char* Source) override
	{
		if (Source == nullptr)
			return false;
		if (m_keyKind == MQEMU_KEY_TEXT)
		{
			VarPtr.SetString(Source);
			return true;
		}
		char* end = nullptr;
		errno = 0;
		long long n = std::strtoll(Source, &end, 10);
		if (end == Source || *end != 0 || errno == ERANGE)
			return false;
		VarPtr.Set<uint64_t>(static_cast<uint64_t>(n));
		return true;
	}

private:
	int32_t m_keyKind;
	mqemu_member_fn m_member;
	mqemu_type_text_fn m_text;
	std::shared_ptr<void> m_user;
	// String members' text, valid until the type is evaluated again (as
	// MacroQuest's own types share DataTypeTemp).
	std::string m_buffer;
};

// Adds datatype `name`, whose values hold keys of `key_kind`. Null if
// MacroQuest refuses it (the name is taken). Ownership of `user` passes here:
// `drop` is called exactly once, by mqemu_remove_type or mqemu_free_type (at
// once if adding fails).
extern "C" mqemu_type* mqemu_add_type(const MQPluginHandle* handle, const char* name, int32_t key_kind,
	mqemu_member_fn member, mqemu_type_text_fn text, void* user, mqemu_drop_fn drop)
{
	std::shared_ptr<void> held(user, [drop](void* p) { if (drop) drop(p); });
	try
	{
		auto type = std::make_unique<mqemu_type>(name, *handle, key_kind, member, text, std::move(held));
		if (!type->Registered())
			return nullptr;
		return type.release();
	}
	catch (...)
	{
		return nullptr;
	}
}

// Adds a member (or a method) with `id`, which its callback gets. False if
// the type already has one by that name.
extern "C" bool mqemu_type_add_member(mqemu_type* type, int32_t id, const char* name, bool method)
{
	try
	{
		return type->Add(id, name, method);
	}
	catch (...)
	{
		return false;
	}
}

// Unregisters the type (turning its variables into strings) and drops its
// user pointer. The type stays allocated for values that still point at it;
// free it with mqemu_free_type when the plugin unloads.
extern "C" void mqemu_remove_type(mqemu_type* type)
{
	try
	{
		type->Unregister();
	}
	catch (...)
	{
	}
}


extern "C" void mqemu_free_type(mqemu_type* type)
{
	try
	{
		type->Unregister();
		delete type;
	}
	catch (...)
	{
	}
}

// Sets a TLO or member callback's value to a value of `type` holding `key`.
// False if the type was removed.
extern "C" bool mqemu_tlo_set_key(mqemu_tlo_result* result, mqemu_type* type, const mqemu_key* key)
{
	try
	{
		if (!type->Registered())
			return false;
		MQTypeVar& dest = *result->dest;
		dest.Type = type;
		write_key(dest, *key);
		return true;
	}
	catch (...)
	{
		return false;
	}
}
