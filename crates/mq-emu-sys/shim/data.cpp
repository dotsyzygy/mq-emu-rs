// C wrappers around MacroQuest's macro data API: reading top-level objects
// (${Me.Level}) as typed values, and adding top-level objects of our own.
//
// This file imports MQ2Main.dll and eqlib.dll (an MQTypeVar can hold an
// eqlib CXStr), so only plugins link it; mq-emu's tests use fakes instead.
//
// No C++ exception may cross into Rust: every entry point catches them.

#include "mq_api.h"

#include "mq/api/MacroAPI.h"

#include <cstring>
#include <type_traits>
#include <variant>

using mq::MainInterface;
using mq::MQPluginHandle;
using mq::MQTypeVar;
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
