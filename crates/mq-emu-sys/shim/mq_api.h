// MacroQuest's public API headers, as the shim sees them.
//
// Some MacroQuest headers rely on its precompiled header for standard library
// includes (e.g. ActorAPI.h uses std::function without <functional>), so
// include those first.

#pragma once

#include <cstdint>
#include <functional>
#include <memory>
#include <string>
#include <string_view>

#include "mq/api/Main.h"
#include "mq/api/PluginAPI.h"
