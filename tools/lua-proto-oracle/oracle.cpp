// MQ2Lua's actor messages, encoded and decoded by real C++ protobuf with the
// code MacroQuest generated (src/plugins/lua/Actor.pb.cc). Tests compare
// mq-emu's hand-written Rust encoding against it.

#include "Actor.pb.h"

#include <google/protobuf/text_format.h>

#include <cstring>
#include <string>

namespace pb = mq::proto::lua::actor;

namespace {

size_t copy_out(const std::string& data, char* out, size_t cap)
{
	if (data.size() > cap)
		return SIZE_MAX;
	std::memcpy(out, data.data(), data.size());
	return data.size();
}

} // namespace

// Parses protobuf text format (e.g. `str: "hi"`) into a Variant and
// serializes it. SIZE_MAX on a parse error or if `cap` is too small.
extern "C" size_t lua_oracle_from_text(const char* text, char* out, size_t cap)
{
	pb::Variant variant;
	if (!google::protobuf::TextFormat::ParseFromString(text, &variant))
		return SIZE_MAX;
	return copy_out(variant.SerializeAsString(), out, cap);
}

// Parses a serialized Variant and prints it in text format, one line, with
// map entries sorted. SIZE_MAX if it doesn't parse.
extern "C" size_t lua_oracle_to_text(const char* data, size_t len, char* out, size_t cap)
{
	pb::Variant variant;
	if (!variant.ParseFromArray(data, static_cast<int>(len)))
		return SIZE_MAX;
	std::string text;
	google::protobuf::TextFormat::Printer printer;
	printer.SetSingleLineMode(true);
	printer.PrintToString(variant, &text);
	while (!text.empty() && text.back() == ' ')
		text.pop_back();
	return copy_out(text, out, cap);
}

// Parses and re-serializes a Variant, as a C++ sender would encode the same
// value. SIZE_MAX if it doesn't parse.
extern "C" size_t lua_oracle_reencode(const char* data, size_t len, char* out, size_t cap)
{
	pb::Variant variant;
	if (!variant.ParseFromArray(data, static_cast<int>(len)))
		return SIZE_MAX;
	return copy_out(variant.SerializeAsString(), out, cap);
}
