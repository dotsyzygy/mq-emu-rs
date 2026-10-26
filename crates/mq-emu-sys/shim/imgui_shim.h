// Helpers shared by the ImGui and ImPlot wrappers (imgui*.cpp, implot.cpp).

#pragma once

#include "mq_api.h"

#include "imgui/imgui.h"

#include <cstring>
#include <exception>
#include <string>

namespace mqemu {

// Records an ImGui error for mqemu_imgui_take_error (imgui.cpp). Keeps the
// first one until it's taken: later errors are usually its consequences.
void note_error(const char* what);

// Runs `body`, catching any C++ exception (MacroQuest's ImGui throws
// ImGuiException on user errors) so none crosses into Rust.
template <typename R, typename F>
R guarded(R fallback, F&& body)
{
	try
	{
		return body();
	}
	catch (const std::exception& e)
	{
		note_error(e.what());
	}
	catch (...)
	{
		note_error(nullptr);
	}
	return fallback;
}

template <typename F>
void guarded_void(F&& body)
{
	guarded(0, [&] { body(); return 0; });
}

// A (pointer, length) string as the NUL-terminated one ImGui wants. Short
// strings (nearly every label) stay on the stack.
class CString
{
public:
	CString(const char* text, size_t len)
	{
		if (len < sizeof(m_small))
		{
			if (len)
				memcpy(m_small, text, len);
			m_small[len] = 0;
			m_ptr = m_small;
		}
		else
		{
			m_big.assign(text, len);
			m_ptr = m_big.c_str();
		}
	}

	CString(const CString&) = delete;
	CString& operator=(const CString&) = delete;

	operator const char*() const { return m_ptr; }

private:
	char m_small[128];
	std::string m_big;
	const char* m_ptr;
};

// Null for an absent optional string (len == SIZE_MAX).
class OptionalCString
{
public:
	OptionalCString(const char* text, size_t len)
	{
		if (text && len != SIZE_MAX)
		{
			m_text.assign(text, len);
			m_present = true;
		}
	}

	operator const char*() const { return m_present ? m_text.c_str() : nullptr; }

private:
	std::string m_text;
	bool m_present = false;
};

// Point `index` of an array of (x, y) float pairs.
inline ImVec2 point(const float* xy, int index)
{
	return ImVec2(xy[index * 2], xy[index * 2 + 1]);
}

// An array of (x, y) float pairs as ImVec2s (the same layout).
inline const ImVec2* points(const float* xy)
{
	return reinterpret_cast<const ImVec2*>(xy);
}

inline ImVec4 color(const float* rgba)
{
	return ImVec4(rgba[0], rgba[1], rgba[2], rgba[3]);
}

// On emu (Direct3D 9) an ImTextureID is a texture pointer.
inline ImTextureRef texture_ref(void* texture)
{
	return ImTextureRef(static_cast<ImTextureID>(texture));
}

} // namespace mqemu
