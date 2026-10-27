// MacroQuest's chat event matching for Rust: Blech, the matcher behind
// macros' #Event and Lua's mq.event, compiled in from MacroQuest's
// contrib/Blech exactly as MacroQuest and MQ2Lua use it.
//
// Blech checks nothing (its asserts only exist in debug builds), and some
// patterns crash it; mqemu_matcher_add refuses those:
// - A pattern with no text between its delimiters ("", "#", "#|") leaves
//   AddEvent without a node, which it then writes through.
// - Feeding walks the tree with a fixed stack of 100 entries and an 8-bit
//   index: a node deeper than MAX_DEPTH writes past it, and from 256 deep
//   the index wraps and Blech follows garbage.
// - Text between two #variables# is gathered in a 16 KB buffer with
//   strcat_s, which ends the process when it overflows; so patterns are
//   shorter than MAX_STRING and |print variables| (each up to MAX_STRING)
//   are limited to MAX_PRINT_VARS.
//
// Matches are collected while Blech feeds a line and only handed to Rust
// after it returns, so Rust code (which may add or remove events) never runs
// inside Blech.
//
// This file imports nothing from MacroQuest, so tests run it directly.
// No C++ exception may cross into Rust: every entry point catches them.

#include <windows.h>

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <memory>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

// Blech keeps its tree private, and the depth check has to walk it. The
// standard headers it includes are already in, so only Blech sees this.
#define private public
#include "Blech/Blech.h"
#undef private

// A |print variable|'s value: `name` is the text between the bars; write the
// value into `value` (`cap` bytes, NUL included) and return its length.
using mqemu_print_var_fn = fBlechVariableValue;
// One match: the event's key, and its #variables# as (name, value) pairs in
// the pattern's order.
using mqemu_event_match_fn = void (*)(void* user, uint64_t key, const char* const* names,
	const char* const* values, size_t count);

enum mqemu_event_status : int32_t
{
	MQEMU_EVENT_OK = 0,
	MQEMU_EVENT_NO_TEXT = 1,
	MQEMU_EVENT_TOO_DEEP = 2,
	MQEMU_EVENT_TOO_MANY_PRINT_VARS = 3,
	MQEMU_EVENT_TOO_LONG = 4,
	MQEMU_EVENT_DUPLICATE_KEY = 5,
	MQEMU_EVENT_FAILED = 6,
};

namespace {

// MacroQuest's MAX_STRING: patterns must be shorter, and print variables get
// this much room (MQ2Lua feeds Blech with the same).
constexpr size_t MAX_STRING = 2048;
// Text between #variables#, in a 16384-byte buffer: a pattern's own text
// (under MAX_STRING) plus this many print variables of up to MAX_STRING - 1.
constexpr int MAX_PRINT_VARS = 7;
// Chew's MatchStack has 100 entries; at a node this deep it writes entry
// depth + 1.
constexpr int MAX_DEPTH = 98;

struct Pending
{
	uint64_t key;
	std::vector<std::pair<std::string, std::string>> values;
};

struct Parts
{
	int text = 0;
	int print_vars = 0;
};

// The parts Blech::AddEvent makes nodes for, found the way it finds them:
// '#' and '|' open and close variables, and doubled outside one stand for
// themselves.
Parts count_parts(const char* pattern)
{
	Parts parts;
	eBlechStringType type = BST_NORMAL;
	const char* part = pattern;
	auto add = [&](const char* end) {
		if (part != end)
		{
			++parts.text;
			if (type == BST_PRINTVAR)
				++parts.print_vars;
		}
	};
	const char* p = pattern;
	for (; *p; ++p)
	{
		if (*p != '#' && *p != '|')
			continue;
		eBlechStringType variable = *p == '#' ? BST_SCANVAR : BST_PRINTVAR;
		if (type == BST_NORMAL && p[1] == *p)
		{
			add(p);
			part = p + 1;
			++p;
		}
		else
		{
			add(p);
			part = p + 1;
			type = type == variable ? BST_NORMAL : variable;
		}
	}
	if (*part)
		add(p);
	return parts;
}

int max_depth(const Blech& blech)
{
	int deepest = 0;
	for (const auto& [id, event] : blech.m_eventMap)
	{
		int depth = 0;
		for (const BlechNode* node = event.pBlechNode; node; node = node->pParent)
			++depth;
		deepest = std::max(deepest, depth);
	}
	return deepest;
}

} // namespace

struct mqemu_matcher
{
	mqemu_print_var_fn print_var;
	std::unique_ptr<Blech> blech;
	// Patterns by key, in the order they were added.
	std::vector<std::pair<uint64_t, std::string>> patterns;
	std::unordered_map<uint64_t, unsigned int> ids;
	std::unordered_map<unsigned int, uint64_t> keys;
	std::vector<Pending> pending;
};

namespace {

void CALLBACK collect(unsigned int id, void* data, BLECHVALUE* values)
{
	auto* matcher = static_cast<mqemu_matcher*>(data);
	auto key = matcher->keys.find(id);
	if (key == matcher->keys.end())
		return;
	Pending match{ key->second, {} };
	for (BLECHVALUE* value = values; value; value = value->pNext)
		match.values.emplace_back(value->Name, value->Value);
	matcher->pending.push_back(std::move(match));
}

} // namespace

// A matcher whose |print variables| `print_var` expands; null if it's null.
extern "C" mqemu_matcher* mqemu_matcher_new(mqemu_print_var_fn print_var)
{
	if (print_var == nullptr)
		return nullptr;
	try
	{
		auto* matcher = new mqemu_matcher;
		matcher->print_var = print_var;
		matcher->blech = std::make_unique<Blech>('#', '|', print_var);
		return matcher;
	}
	catch (...)
	{
		return nullptr;
	}
}

extern "C" void mqemu_matcher_free(mqemu_matcher* matcher)
{
	delete matcher;
}

// Adds `pattern` as event `key`, or says why not (mqemu_event_status). The
// matcher is rebuilt with it, and kept as it was if it's refused.
extern "C" int32_t mqemu_matcher_add(mqemu_matcher* matcher, const char* pattern, uint64_t key)
{
	try
	{
		if (std::strlen(pattern) >= MAX_STRING)
			return MQEMU_EVENT_TOO_LONG;
		Parts parts = count_parts(pattern);
		if (parts.text == 0)
			return MQEMU_EVENT_NO_TEXT;
		if (parts.print_vars > MAX_PRINT_VARS)
			return MQEMU_EVENT_TOO_MANY_PRINT_VARS;
		if (matcher->ids.count(key))
			return MQEMU_EVENT_DUPLICATE_KEY;

		// Built afresh: Blech never undoes the node splits an add makes, so
		// adding a refused pattern to the live tree could leave other
		// patterns too deep.
		auto blech = std::make_unique<Blech>('#', '|', matcher->print_var);
		std::unordered_map<uint64_t, unsigned int> ids;
		std::unordered_map<unsigned int, uint64_t> keys;
		auto add = [&](uint64_t k, const char* text) {
			unsigned int id = blech->AddEvent(text, collect, matcher);
			ids[k] = id;
			keys[id] = k;
		};
		for (const auto& [k, text] : matcher->patterns)
			add(k, text.c_str());
		add(key, pattern);
		if (max_depth(*blech) > MAX_DEPTH)
			return MQEMU_EVENT_TOO_DEEP;

		matcher->patterns.emplace_back(key, pattern);
		matcher->blech = std::move(blech);
		matcher->ids = std::move(ids);
		matcher->keys = std::move(keys);
		return MQEMU_EVENT_OK;
	}
	catch (...)
	{
		return MQEMU_EVENT_FAILED;
	}
}

// Removes event `key`; false if there's none.
extern "C" bool mqemu_matcher_remove(mqemu_matcher* matcher, uint64_t key)
{
	try
	{
		auto id = matcher->ids.find(key);
		if (id == matcher->ids.end())
			return false;
		matcher->blech->RemoveEvent(id->second);
		matcher->keys.erase(id->second);
		matcher->ids.erase(id);
		matcher->patterns.erase(std::find_if(matcher->patterns.begin(), matcher->patterns.end(),
			[key](const auto& entry) { return entry.first == key; }));
		return true;
	}
	catch (...)
	{
		return false;
	}
}

extern "C" bool mqemu_matcher_is_empty(const mqemu_matcher* matcher)
{
	return matcher->patterns.empty();
}

// How deep the matcher's deepest pattern sits in Blech's tree.
extern "C" int32_t mqemu_matcher_depth(const mqemu_matcher* matcher)
{
	return max_depth(*matcher->blech);
}

// Matches `line` against every event, then calls `on_match` for each event
// that matched, in the order the events were added. False if matching failed.
extern "C" bool mqemu_matcher_feed(mqemu_matcher* matcher, const char* line, mqemu_event_match_fn on_match, void* user)
{
	std::vector<Pending> matches;
	try
	{
		matcher->pending.clear();
		matcher->blech->Feed(line, MAX_STRING);
		matches = std::move(matcher->pending);
		matcher->pending.clear();
		std::stable_sort(matches.begin(), matches.end(), [](const Pending& a, const Pending& b) { return a.key < b.key; });
	}
	catch (...)
	{
		matcher->pending.clear();
		return false;
	}

	for (const Pending& match : matches)
	{
		std::vector<const char*> names;
		std::vector<const char*> values;
		try
		{
			for (const auto& [name, value] : match.values)
			{
				names.push_back(name.c_str());
				values.push_back(value.c_str());
			}
		}
		catch (...)
		{
			return false;
		}
		on_match(user, match.key, names.data(), values.data(), names.size());
	}
	return true;
}

// ----- tests only ------------------------------------------------------------

// Blech as MacroQuest uses it, without the guards above: adds `patterns`
// and feeds `line`. For tests that show, in a child process, what the
// guards prevent. Windows' crash dialog is turned off first.
extern "C" uint32_t mqemu_test_blech_unguarded(const char* const* patterns, size_t count, const char* line,
	mqemu_print_var_fn print_var)
{
	SetErrorMode(SEM_FAILCRITICALERRORS | SEM_NOGPFAULTERRORBOX);
	Blech blech('#', '|', print_var);
	for (size_t i = 0; i < count; ++i)
		blech.AddEvent(patterns[i], [](unsigned int, void*, BLECHVALUE*) {}, nullptr);
	return blech.Feed(line, MAX_STRING);
}
