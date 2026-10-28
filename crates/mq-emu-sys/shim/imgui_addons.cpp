// MacroQuest's ImGui add-ons, all in imgui.dll: its text editor
// (mq::imgui::TextEditor), the memory editor (header-only, so compiled here
// from MacroQuest's copy) and ImGuiFileDialog (through its C API).
// Conventions as in imgui.cpp.
//
// The text editor is a C++ class: its exported methods take and return
// std::string and std::vector, and its inline ones (compiled here) read its
// fields. That works because this shim and imgui.dll are built with the same
// compiler, STL and release settings (cc never uses the debug CRT), and every
// C runtime since VS2015 allocates from the process heap, so a string
// allocated in imgui.dll can be freed here. MacroQuest's own plugins (MQ2Lua)
// use it the same way.

#include "imgui_shim.h"

#include "imgui/ImGuiTextEditor.h"
#include "imgui/ImGuiMemoryEditor.h"
// Without this, the header declares its C API dllexport and every plugin
// would export it again.
#define IMGUIFILEDIALOG_NO_EXPORT
#include "imgui/ImGuiFileDialog.h"

#include <algorithm>
#include <cfloat>
#include <regex>

using namespace mqemu;
using mq::imgui::texteditor::Coordinates;
using mq::imgui::texteditor::LanguageDefinition;
using mq::imgui::texteditor::Palette;
using mq::imgui::texteditor::PaletteIndex;
using mq::imgui::texteditor::SelectionMode;
using mq::imgui::texteditor::TextEditor;

extern "C" {

typedef void (*mqemu_string_sink)(void* user, const char* text, size_t len);
typedef void (*mqemu_selection_sink)(void* user, const char* name, size_t name_len, const char* path, size_t path_len);

struct mqemu_strings
{
	const char* const* texts;
	const size_t* lens;
	size_t count;
};

struct mqemu_text_editor_language
{
	const char* name;
	size_t name_len;
	mqemu_strings keywords;
	mqemu_strings identifiers;
	mqemu_strings declarations;
	mqemu_strings preproc_identifiers;
	mqemu_strings preproc_declarations;
	const char* comment_start;
	size_t comment_start_len;
	const char* comment_end;
	size_t comment_end_len;
	const char* single_line_comment;
	size_t single_line_comment_len;
	char preproc_char;
	bool auto_indentation;
	mqemu_strings token_regexes;
	const int* token_palette;
	bool case_sensitive;
};

struct mqemu_memory_editor_options
{
	bool read_only;
	int cols;
	bool show_options;
	bool show_data_preview;
	bool show_hexii;
	bool show_ascii;
	bool grey_out_zeroes;
	bool upper_case_hex;
	int mid_cols_count;
	int addr_digits_count;
	float footer_extra_height;
	ImU32 highlight_color;
};

struct mqemu_memory_editor_hooks
{
	ImU8 (*read)(const ImU8* data, size_t offset, void* user);
	void (*write)(ImU8* data, size_t offset, ImU8 value, void* user);
	bool (*highlight)(const ImU8* data, size_t offset, void* user);
	ImU32 (*background)(const ImU8* data, size_t offset, void* user);
	void* user;
};

typedef void (*mqemu_file_dialog_pane_fn)(void* user, const char* filter, size_t filter_len, bool* can_continue);

} // extern "C"

namespace {

std::string_view view(const char* text, size_t len)
{
	return len ? std::string_view(text, len) : std::string_view();
}

std::string string(const char* text, size_t len)
{
	return std::string(view(text, len));
}

void send(mqemu_string_sink sink, void* user, std::string_view text)
{
	sink(user, text.data(), text.size());
}

std::string strings_at(const mqemu_strings& strings, size_t index)
{
	return string(strings.texts[index], strings.lens[index]);
}

Coordinates coordinates(int line, int column)
{
	// Coordinates(int, int) asserts both aren't negative; the fields don't.
	Coordinates result;
	result.line = line;
	result.column = column;
	return result;
}

// The editor's GetCursorPosition is inline but calls private methods
// imgui.dll doesn't export, so the cursor (and the selection, which has no
// getter) is read from its private state: explicit instantiation may name
// private members, and the pointers it gets are used through these.
template <typename Tag, typename Tag::type Member>
struct PrivateAccess
{
	friend typename Tag::type private_member(Tag) { return Member; }
};

struct StateTag
{
	using type = mq::imgui::texteditor::EditorState TextEditor::*;
	friend type private_member(StateTag);
};

struct LinesTag
{
	using type = mq::imgui::texteditor::Lines TextEditor::*;
	friend type private_member(LinesTag);
};

struct UndoBufferTag
{
	using type = mq::imgui::texteditor::UndoBuffer TextEditor::*;
	friend type private_member(UndoBufferTag);
};

template struct PrivateAccess<StateTag, &TextEditor::m_state>;
template struct PrivateAccess<LinesTag, &TextEditor::m_lines>;
template struct PrivateAccess<UndoBufferTag, &TextEditor::m_undoBuffer>;

// MacroQuest's UndoRecord::Undo passes the record's removedStart to
// InsertTextAt, which moves it to the end of what it inserts (Redo does the
// same to addedStart), so after an undo, redo deletes nothing, and after a
// redo, undo removes nothing. This keeps each record's starts from before a
// call that may undo or redo (Undo, Redo, and Render for ctrl+Z and ctrl+Y)
// and puts them back after. A record the call replaced (a new edit after an
// undo) has other ends or text, and is left alone.
class UndoStarts
{
public:
	explicit UndoStarts(TextEditor* editor)
		: m_buffer(editor->*private_member(UndoBufferTag()))
	{
		m_records.reserve(m_buffer.size());
		for (const mq::imgui::texteditor::UndoRecord& record : m_buffer)
		{
			m_records.push_back({ record.addedStart, record.addedEnd, record.removedStart, record.removedEnd,
				record.added.size(), record.removed.size() });
		}
	}

	UndoStarts(const UndoStarts&) = delete;
	UndoStarts& operator=(const UndoStarts&) = delete;

	~UndoStarts()
	{
		for (size_t i = 0; i < m_records.size() && i < m_buffer.size(); ++i)
		{
			mq::imgui::texteditor::UndoRecord& record = m_buffer[i];
			const Saved& saved = m_records[i];
			if (record.addedEnd == saved.added_end && record.removedEnd == saved.removed_end
				&& record.added.size() == saved.added_size && record.removed.size() == saved.removed_size)
			{
				record.addedStart = saved.added_start;
				record.removedStart = saved.removed_start;
			}
		}
	}

private:
	struct Saved
	{
		Coordinates added_start, added_end, removed_start, removed_end;
		size_t added_size, removed_size;
	};

	mq::imgui::texteditor::UndoBuffer& m_buffer;
	std::vector<Saved> m_records;
};

// As the editor's UTF8CharLength: the bytes of the character starting with `c`.
int utf8_char_length(uint8_t c)
{
	if ((c & 0xFE) == 0xFC)
		return 6;
	if ((c & 0xFC) == 0xF8)
		return 5;
	if ((c & 0xF8) == 0xF0)
		return 4;
	if ((c & 0xF0) == 0xE0)
		return 3;
	if ((c & 0xE0) == 0xC0)
		return 2;
	return 1;
}

// As the editor's GetLineMaxColumn: the columns line `index` takes.
int line_max_column(const TextEditor* editor, int index)
{
	const mq::imgui::texteditor::Lines& lines = editor->*private_member(LinesTag());
	if (index < 0 || index >= static_cast<int>(lines.size()))
		return 0;
	const int tab_size = editor->GetTabSize();
	int column = 0;
	const auto& glyphs = lines[index].glyphs;
	for (size_t i = 0; i < glyphs.size(); i += utf8_char_length(glyphs[i].ch))
		column = glyphs[i].ch == '\t' ? (column / tab_size) * tab_size + tab_size : column + 1;
	return column;
}

// As the editor's SanitizeCoordinates: `value` clamped to the text.
Coordinates sanitized(const TextEditor* editor, Coordinates value)
{
	const int count = static_cast<int>((editor->*private_member(LinesTag())).size());
	if (value.line >= count)
		return count == 0 ? coordinates(0, 0) : coordinates(count - 1, line_max_column(editor, count - 1));
	return coordinates(value.line, count == 0 ? 0 : std::min(value.column, line_max_column(editor, value.line)));
}

} // namespace

// ----- text editor -----

extern "C" TextEditor* mqemu_text_editor_new()
{
	return guarded<TextEditor*>(nullptr, [] { return new TextEditor(); });
}

extern "C" void mqemu_text_editor_delete(TextEditor* editor)
{
	guarded_void([&] { delete editor; });
}

extern "C" void mqemu_text_editor_render(TextEditor* editor, const char* title, size_t title_len, float w, float h, bool border)
{
	guarded_void([&] {
		UndoStarts starts(editor);
		editor->Render(CString(title, title_len), ImVec2(w, h), border);
	});
}

extern "C" void mqemu_text_editor_set_language(TextEditor* editor, int language)
{
	guarded_void([&] {
		switch (language)
		{
		case 0: editor->SetLanguageDefinition(LanguageDefinition::CPlusPlus()); break;
		case 1: editor->SetLanguageDefinition(LanguageDefinition::HLSL()); break;
		case 2: editor->SetLanguageDefinition(LanguageDefinition::GLSL()); break;
		case 3: editor->SetLanguageDefinition(LanguageDefinition::C()); break;
		case 4: editor->SetLanguageDefinition(LanguageDefinition::SQL()); break;
		case 5: editor->SetLanguageDefinition(LanguageDefinition::Lua()); break;
		default: editor->SetLanguageDefinition(LanguageDefinition::PlainText()); break;
		}
	});
}

// Builds the definition here and lets the editor copy it. Each token regex is
// compiled first as the editor compiles them (std::regex with `optimize`),
// so a bad one is reported rather than thrown out of SetLanguageDefinition.
extern "C" bool mqemu_text_editor_set_language_definition(TextEditor* editor, const mqemu_text_editor_language* language,
	mqemu_string_sink error, void* user)
{
	return guarded(false, [&] {
		LanguageDefinition definition;
		definition.name = string(language->name, language->name_len);
		for (size_t i = 0; i < language->keywords.count; ++i)
			definition.keywords.insert(strings_at(language->keywords, i));
		for (size_t i = 0; i < language->identifiers.count; ++i)
		{
			mq::imgui::texteditor::Identifier identifier;
			identifier.declaration = strings_at(language->declarations, i);
			definition.identifiers.insert_or_assign(strings_at(language->identifiers, i), identifier);
		}
		for (size_t i = 0; i < language->preproc_identifiers.count; ++i)
		{
			mq::imgui::texteditor::Identifier identifier;
			identifier.declaration = strings_at(language->preproc_declarations, i);
			definition.preprocIdentifiers.insert_or_assign(strings_at(language->preproc_identifiers, i), identifier);
		}
		definition.commentStart = string(language->comment_start, language->comment_start_len);
		definition.commentEnd = string(language->comment_end, language->comment_end_len);
		definition.singleLineComment = string(language->single_line_comment, language->single_line_comment_len);
		definition.preprocChar = language->preproc_char;
		definition.autoIndentation = language->auto_indentation;
		definition.caseSensitive = language->case_sensitive;
		for (size_t i = 0; i < language->token_regexes.count; ++i)
		{
			std::string pattern = strings_at(language->token_regexes, i);
			try
			{
				std::regex check(pattern, std::regex_constants::optimize);
			}
			catch (const std::regex_error& e)
			{
				send(error, user, std::string("token regex ") + std::to_string(i) + " (" + pattern + "): " + e.what());
				return false;
			}
			definition.tokenRegexStrings.emplace_back(std::move(pattern), static_cast<PaletteIndex>(language->token_palette[i]));
		}
		editor->SetLanguageDefinition(definition);
		return true;
	});
}

extern "C" void mqemu_text_editor_language_name(const TextEditor* editor, mqemu_string_sink sink, void* user)
{
	guarded_void([&] { send(sink, user, editor->GetLanguageDefinition().name); });
}

extern "C" void mqemu_text_editor_palette(const TextEditor* editor, ImU32* out)
{
	const Palette& palette = editor->GetPalette();
	std::copy(palette.begin(), palette.end(), out);
}

extern "C" void mqemu_text_editor_set_palette(TextEditor* editor, const ImU32* palette)
{
	guarded_void([&] {
		Palette value;
		std::copy(palette, palette + value.size(), value.begin());
		editor->SetPalette(value);
	});
}

extern "C" void mqemu_text_editor_builtin_palette(int which, ImU32* out)
{
	guarded_void([&] {
		const Palette& palette = which == 1 ? TextEditor::GetLightPalette()
			: which == 2 ? TextEditor::GetRetroBluePalette() : TextEditor::GetDarkPalette();
		std::copy(palette.begin(), palette.end(), out);
	});
}

extern "C" void mqemu_text_editor_set_error_markers(TextEditor* editor, const int* lines, mqemu_strings messages)
{
	guarded_void([&] {
		mq::imgui::texteditor::ErrorMarkers markers;
		for (size_t i = 0; i < messages.count; ++i)
			markers.insert_or_assign(lines[i], strings_at(messages, i));
		editor->SetErrorMarkers(markers);
	});
}

extern "C" void mqemu_text_editor_set_breakpoints(TextEditor* editor, const int* lines, size_t count)
{
	guarded_void([&] { editor->SetBreakpoints(mq::imgui::texteditor::Breakpoints(lines, lines + count)); });
}

extern "C" void mqemu_text_editor_set_text(TextEditor* editor, const char* text, size_t len)
{
	guarded_void([&] { editor->SetText(view(text, len)); });
}

extern "C" void mqemu_text_editor_set_text_colored(TextEditor* editor, const char* text, size_t len, ImU32 color)
{
	guarded_void([&] { editor->SetText(view(text, len), color); });
}

extern "C" void mqemu_text_editor_set_text_lines(TextEditor* editor, mqemu_strings lines)
{
	guarded_void([&] {
		std::vector<std::string> value;
		value.reserve(lines.count);
		for (size_t i = 0; i < lines.count; ++i)
			value.push_back(strings_at(lines, i));
		editor->SetTextLines(value);
	});
}

extern "C" void mqemu_text_editor_insert_text(TextEditor* editor, const char* text, size_t len)
{
	guarded_void([&] { editor->InsertText(view(text, len)); });
}

extern "C" void mqemu_text_editor_insert_text_colored(TextEditor* editor, const char* text, size_t len, ImU32 color)
{
	guarded_void([&] { editor->InsertText(view(text, len), color); });
}

extern "C" void mqemu_text_editor_text(const TextEditor* editor, mqemu_string_sink sink, void* user)
{
	guarded_void([&] { send(sink, user, editor->GetText()); });
}

extern "C" void mqemu_text_editor_text_lines(const TextEditor* editor, mqemu_string_sink sink, void* user)
{
	guarded_void([&] {
		for (const std::string& line : editor->GetTextLines())
			send(sink, user, line);
	});
}

extern "C" void mqemu_text_editor_selected_text(const TextEditor* editor, mqemu_string_sink sink, void* user)
{
	guarded_void([&] { send(sink, user, editor->GetSelectedText()); });
}

extern "C" void mqemu_text_editor_current_line_text(const TextEditor* editor, mqemu_string_sink sink, void* user)
{
	guarded_void([&] { send(sink, user, editor->GetCurrentLineText()); });
}

extern "C" int mqemu_text_editor_total_lines(const TextEditor* editor) { return editor->GetTotalLines(); }
extern "C" bool mqemu_text_editor_is_overwrite(const TextEditor* editor) { return editor->IsOverwrite(); }
extern "C" bool mqemu_text_editor_is_read_only(const TextEditor* editor) { return editor->IsReadOnly(); }
extern "C" void mqemu_text_editor_set_read_only(TextEditor* editor, bool value) { editor->SetReadOnly(value); }
extern "C" bool mqemu_text_editor_is_text_changed(const TextEditor* editor) { return editor->IsTextChanged(); }
extern "C" bool mqemu_text_editor_is_cursor_position_changed(const TextEditor* editor) { return editor->IsCursorPositionChanged(); }
extern "C" void mqemu_text_editor_set_render_cursor(TextEditor* editor, bool value) { editor->SetRenderCursor(value); }
extern "C" void mqemu_text_editor_set_render_line_numbers(TextEditor* editor, bool value) { editor->SetRenderLineNumbers(value); }
extern "C" bool mqemu_text_editor_is_colorizer_enabled(const TextEditor* editor) { return editor->IsColorizerEnabled(); }

extern "C" void mqemu_text_editor_set_colorizer_enabled(TextEditor* editor, bool value)
{
	guarded_void([&] { editor->SetColorizerEnable(value); });
}

// The editor's IsHandleMouseInputsEnabled reports the keyboard setting, so
// there's no getter here: mq-emu remembers what it set.
extern "C" void mqemu_text_editor_set_handle_mouse_inputs(TextEditor* editor, bool value) { editor->SetHandleMouseInputs(value); }
extern "C" bool mqemu_text_editor_is_handle_keyboard_inputs(const TextEditor* editor) { return editor->IsHandleKeyboardInputsEnabled(); }
extern "C" void mqemu_text_editor_set_handle_keyboard_inputs(TextEditor* editor, bool value) { editor->SetHandleKeyboardInputs(value); }
extern "C" bool mqemu_text_editor_is_imgui_child_ignored(const TextEditor* editor) { return editor->IsImGuiChildIgnored(); }
extern "C" void mqemu_text_editor_set_imgui_child_ignored(TextEditor* editor, bool value) { editor->SetImGuiChildIgnored(value); }
extern "C" bool mqemu_text_editor_is_showing_whitespace(const TextEditor* editor) { return editor->IsShowingWhitespace(); }
extern "C" void mqemu_text_editor_set_show_whitespace(TextEditor* editor, bool value) { editor->SetShowWhitespace(value); }
extern "C" bool mqemu_text_editor_is_showing_short_tab_glyphs(const TextEditor* editor) { return editor->IsShowingShortTabGlyphs(); }
extern "C" void mqemu_text_editor_set_show_short_tab_glyphs(TextEditor* editor, bool value) { editor->SetShowShortTabGlyphs(value); }
extern "C" int mqemu_text_editor_tab_size(const TextEditor* editor) { return editor->GetTabSize(); }

extern "C" void mqemu_text_editor_set_tab_size(TextEditor* editor, int value)
{
	guarded_void([&] { editor->SetTabSize(value); });
}

extern "C" void mqemu_text_editor_cursor_position(const TextEditor* editor, int* line, int* column)
{
	Coordinates position = sanitized(editor, (editor->*private_member(StateTag())).cursorPosition);
	*line = position.line;
	*column = position.column;
}

// The selection's start and end (equal when nothing is selected), as the
// editor keeps them: already clamped to the text.
extern "C" void mqemu_text_editor_selection(const TextEditor* editor, int* start_line, int* start_column, int* end_line,
	int* end_column)
{
	const mq::imgui::texteditor::EditorState& state = editor->*private_member(StateTag());
	*start_line = state.selectionStart.line;
	*start_column = state.selectionStart.column;
	*end_line = state.selectionEnd.line;
	*end_column = state.selectionEnd.column;
}

extern "C" void mqemu_text_editor_set_cursor_position(TextEditor* editor, int line, int column)
{
	guarded_void([&] { editor->SetCursorPosition(coordinates(line, column)); });
}

extern "C" void mqemu_text_editor_move_up(TextEditor* editor, int amount, bool select)
{
	guarded_void([&] { editor->MoveUp(amount, select); });
}

extern "C" void mqemu_text_editor_move_down(TextEditor* editor, int amount, bool select)
{
	guarded_void([&] { editor->MoveDown(amount, select); });
}

extern "C" void mqemu_text_editor_move_left(TextEditor* editor, int amount, bool select, bool word_mode)
{
	guarded_void([&] { editor->MoveLeft(amount, select, word_mode); });
}

extern "C" void mqemu_text_editor_move_right(TextEditor* editor, int amount, bool select, bool word_mode)
{
	guarded_void([&] { editor->MoveRight(amount, select, word_mode); });
}

extern "C" void mqemu_text_editor_move_top(TextEditor* editor, bool select)
{
	guarded_void([&] { editor->MoveTop(select); });
}

extern "C" void mqemu_text_editor_move_bottom(TextEditor* editor, bool select)
{
	guarded_void([&] { editor->MoveBottom(select); });
}

extern "C" void mqemu_text_editor_move_home(TextEditor* editor, bool select)
{
	guarded_void([&] { editor->MoveHome(select); });
}

extern "C" void mqemu_text_editor_move_end(TextEditor* editor, bool select)
{
	guarded_void([&] { editor->MoveEnd(select); });
}

extern "C" void mqemu_text_editor_set_selection_start(TextEditor* editor, int line, int column)
{
	guarded_void([&] { editor->SetSelectionStart(coordinates(line, column)); });
}

extern "C" void mqemu_text_editor_set_selection_end(TextEditor* editor, int line, int column)
{
	guarded_void([&] { editor->SetSelectionEnd(coordinates(line, column)); });
}

extern "C" void mqemu_text_editor_set_selection(TextEditor* editor, int start_line, int start_column, int end_line,
	int end_column, int mode)
{
	guarded_void([&] {
		editor->SetSelection(coordinates(start_line, start_column), coordinates(end_line, end_column), static_cast<SelectionMode>(mode));
	});
}

extern "C" void mqemu_text_editor_select_word_under_cursor(TextEditor* editor)
{
	guarded_void([&] { editor->SelectWordUnderCursor(); });
}

extern "C" void mqemu_text_editor_select_all(TextEditor* editor)
{
	guarded_void([&] { editor->SelectAll(); });
}

extern "C" bool mqemu_text_editor_has_selection(const TextEditor* editor)
{
	return guarded(false, [&] { return editor->HasSelection(); });
}

extern "C" void mqemu_text_editor_copy(TextEditor* editor)
{
	guarded_void([&] { editor->Copy(); });
}

extern "C" void mqemu_text_editor_cut(TextEditor* editor)
{
	guarded_void([&] { editor->Cut(); });
}

extern "C" void mqemu_text_editor_paste(TextEditor* editor)
{
	guarded_void([&] { editor->Paste(); });
}

extern "C" void mqemu_text_editor_delete_forward(TextEditor* editor)
{
	guarded_void([&] { editor->Delete(); });
}

extern "C" bool mqemu_text_editor_can_undo(const TextEditor* editor)
{
	return guarded(false, [&] { return editor->CanUndo(); });
}

extern "C" bool mqemu_text_editor_can_redo(const TextEditor* editor)
{
	return guarded(false, [&] { return editor->CanRedo(); });
}

extern "C" void mqemu_text_editor_undo(TextEditor* editor, int steps)
{
	guarded_void([&] {
		UndoStarts starts(editor);
		editor->Undo(steps);
	});
}

extern "C" void mqemu_text_editor_redo(TextEditor* editor, int steps)
{
	guarded_void([&] {
		UndoStarts starts(editor);
		editor->Redo(steps);
	});
}

// ----- memory editor -----

extern "C" MemoryEditor* mqemu_memory_editor_new()
{
	return guarded<MemoryEditor*>(nullptr, [] { return new MemoryEditor(); });
}

extern "C" void mqemu_memory_editor_delete(MemoryEditor* editor)
{
	delete editor;
}

// The settings go in before drawing and come back after, as the user may
// change them from the options menu. The hooks are set for this draw only.
extern "C" void mqemu_memory_editor_draw(MemoryEditor* editor, mqemu_memory_editor_options* options, const char* title,
	size_t title_len, bool* open, ImU8* data, size_t size, size_t base, const mqemu_memory_editor_hooks* hooks)
{
	editor->ReadOnly = options->read_only;
	editor->Cols = options->cols;
	editor->OptShowOptions = options->show_options;
	editor->OptShowDataPreview = options->show_data_preview;
	editor->OptShowHexII = options->show_hexii;
	editor->OptShowAscii = options->show_ascii;
	editor->OptGreyOutZeroes = options->grey_out_zeroes;
	editor->OptUpperCaseHex = options->upper_case_hex;
	editor->OptMidColsCount = options->mid_cols_count;
	editor->OptAddrDigitsCount = options->addr_digits_count;
	editor->OptFooterExtraHeight = options->footer_extra_height;
	editor->HighlightColor = options->highlight_color;
	editor->ReadFn = hooks ? hooks->read : nullptr;
	editor->WriteFn = hooks ? hooks->write : nullptr;
	editor->HighlightFn = hooks ? hooks->highlight : nullptr;
	editor->BgColorFn = hooks ? hooks->background : nullptr;
	editor->UserData = hooks ? hooks->user : nullptr;

	guarded_void([&] {
		if (title_len == SIZE_MAX)
		{
			editor->DrawContents(data, size, base);
		}
		else
		{
			editor->DrawWindow(CString(title, title_len), data, size, base);
			*open = editor->Open;
		}
	});

	editor->ReadFn = nullptr;
	editor->WriteFn = nullptr;
	editor->HighlightFn = nullptr;
	editor->BgColorFn = nullptr;
	editor->UserData = nullptr;
	options->cols = editor->Cols;
	options->show_data_preview = editor->OptShowDataPreview;
	options->show_hexii = editor->OptShowHexII;
	options->show_ascii = editor->OptShowAscii;
	options->grey_out_zeroes = editor->OptGreyOutZeroes;
	options->upper_case_hex = editor->OptUpperCaseHex;
}

extern "C" void mqemu_memory_editor_goto(MemoryEditor* editor, size_t min, size_t max)
{
	editor->GotoAddrAndHighlight(min, max);
}

extern "C" bool mqemu_memory_editor_hovered(const MemoryEditor* editor, size_t* offset)
{
	*offset = editor->MouseHoveredAddr;
	return editor->MouseHovered;
}

// ----- file dialog -----

// The dialog, and the side pane to draw during the current display call
// (the pane function IGFD keeps is pane_trampoline, with this as its data).
struct mqemu_file_dialog
{
	ImGuiFileDialog* dialog;
	mqemu_file_dialog_pane_fn pane;
	void* pane_user;
};

namespace {

void pane_trampoline(const char* filter, void* data, bool* can_continue)
{
	auto* self = static_cast<mqemu_file_dialog*>(data);
	if (self->pane)
		self->pane(self->pane_user, filter ? filter : "", filter ? strlen(filter) : 0, can_continue);
}

// Sends an IGFD string (allocated by imgui.dll) and frees it there.
void send_igfd(mqemu_string_sink sink, void* user, char* text)
{
	if (text)
	{
		send(sink, user, text);
		IGFD_DestroyString(text);
	}
	else
	{
		send(sink, user, {});
	}
}

} // namespace

extern "C" mqemu_file_dialog* mqemu_file_dialog_new()
{
	return guarded<mqemu_file_dialog*>(nullptr, [] { return new mqemu_file_dialog{ IGFD_Create(), nullptr, nullptr }; });
}

extern "C" void mqemu_file_dialog_delete(mqemu_file_dialog* dialog)
{
	guarded_void([&] {
		IGFD_Destroy(dialog->dialog);
		delete dialog;
	});
}

extern "C" void mqemu_file_dialog_open(mqemu_file_dialog* dialog, const char* key, size_t key_len, const char* title,
	size_t title_len, const char* filters, size_t filters_len, const char* path, size_t path_len, const char* file_name,
	size_t file_name_len, int max_selection, int flags, float pane_width)
{
	guarded_void([&] {
		OptionalCString filter_text(filters, filters_len);
		if (pane_width > 0)
		{
			IGFD_OpenDialogWithPane(dialog->dialog, CString(key, key_len), CString(title, title_len), filter_text,
				CString(path, path_len), CString(file_name, file_name_len), pane_trampoline, pane_width, max_selection,
				dialog, flags);
		}
		else
		{
			IGFD_OpenDialog(dialog->dialog, CString(key, key_len), CString(title, title_len), filter_text,
				CString(path, path_len), CString(file_name, file_name_len), max_selection, dialog, flags);
		}
	});
}

extern "C" bool mqemu_file_dialog_display(mqemu_file_dialog* dialog, const char* key, size_t key_len, int window_flags,
	float min_w, float min_h, float max_w, float max_h, mqemu_file_dialog_pane_fn pane, void* user)
{
	dialog->pane = pane;
	dialog->pane_user = user;
	bool closed = guarded(false, [&] {
		return IGFD_DisplayDialog(dialog->dialog, CString(key, key_len), window_flags, ImVec2(min_w, min_h), ImVec2(max_w, max_h));
	});
	dialog->pane = nullptr;
	dialog->pane_user = nullptr;
	return closed;
}

extern "C" void mqemu_file_dialog_close(mqemu_file_dialog* dialog)
{
	guarded_void([&] { IGFD_CloseDialog(dialog->dialog); });
}

extern "C" bool mqemu_file_dialog_is_ok(const mqemu_file_dialog* dialog)
{
	return guarded(false, [&] { return IGFD_IsOk(dialog->dialog); });
}

extern "C" bool mqemu_file_dialog_is_opened(const mqemu_file_dialog* dialog, const char* key, size_t key_len)
{
	return guarded(false, [&] {
		OptionalCString name(key, key_len);
		return name ? IGFD_IsKeyOpened(dialog->dialog, name) : IGFD_IsOpened(dialog->dialog);
	});
}

extern "C" bool mqemu_file_dialog_was_opened_this_frame(const mqemu_file_dialog* dialog, const char* key, size_t key_len)
{
	return guarded(false, [&] {
		OptionalCString name(key, key_len);
		return name ? IGFD_WasKeyOpenedThisFrame(dialog->dialog, name) : IGFD_WasOpenedThisFrame(dialog->dialog);
	});
}

extern "C" void mqemu_file_dialog_selection(mqemu_file_dialog* dialog, int mode, mqemu_selection_sink sink, void* user)
{
	guarded_void([&] {
		IGFD_Selection selection = IGFD_GetSelection(dialog->dialog, mode);
		for (size_t i = 0; i < selection.count; ++i)
		{
			const IGFD_Selection_Pair& pair = selection.table[i];
			const char* name = pair.fileName ? pair.fileName : "";
			const char* path = pair.filePathName ? pair.filePathName : "";
			sink(user, name, strlen(name), path, strlen(path));
		}
		IGFD_Selection_DestroyContent(&selection);
	});
}

extern "C" void mqemu_file_dialog_file_path_name(mqemu_file_dialog* dialog, int mode, mqemu_string_sink sink, void* user)
{
	guarded_void([&] { send_igfd(sink, user, IGFD_GetFilePathName(dialog->dialog, mode)); });
}

extern "C" void mqemu_file_dialog_current_file_name(mqemu_file_dialog* dialog, int mode, mqemu_string_sink sink, void* user)
{
	guarded_void([&] { send_igfd(sink, user, IGFD_GetCurrentFileName(dialog->dialog, mode)); });
}

extern "C" void mqemu_file_dialog_current_path(mqemu_file_dialog* dialog, mqemu_string_sink sink, void* user)
{
	guarded_void([&] { send_igfd(sink, user, IGFD_GetCurrentPath(dialog->dialog)); });
}

extern "C" void mqemu_file_dialog_current_filter(mqemu_file_dialog* dialog, mqemu_string_sink sink, void* user)
{
	guarded_void([&] { send_igfd(sink, user, IGFD_GetCurrentFilter(dialog->dialog)); });
}

extern "C" void mqemu_file_dialog_set_file_style(mqemu_file_dialog* dialog, int flags, const char* criteria, size_t criteria_len,
	const float* rgba, const char* icon, size_t icon_len, ImFont* font)
{
	guarded_void([&] {
		IGFD_SetFileStyle(dialog->dialog, flags, CString(criteria, criteria_len), color(rgba), CString(icon, icon_len), font);
	});
}

extern "C" bool mqemu_file_dialog_file_style(mqemu_file_dialog* dialog, int flags, const char* criteria, size_t criteria_len,
	float* rgba, mqemu_string_sink icon, void* user, ImFont** font)
{
	return guarded(false, [&] {
		ImVec4 found_color;
		char* found_icon = nullptr;
		ImFont* found_font = nullptr;
		if (!IGFD_GetFileStyle(dialog->dialog, flags, CString(criteria, criteria_len), &found_color, &found_icon, &found_font))
			return false;
		rgba[0] = found_color.x;
		rgba[1] = found_color.y;
		rgba[2] = found_color.z;
		rgba[3] = found_color.w;
		send_igfd(icon, user, found_icon);
		*font = found_font;
		return true;
	});
}

extern "C" void mqemu_file_dialog_clear_file_styles(mqemu_file_dialog* dialog)
{
	guarded_void([&] { IGFD_ClearFilesStyle(dialog->dialog); });
}

// ----- for tests -----

// The editor's own Undo and Redo, without UndoStarts, to show what it fixes.
extern "C" void mqemu_text_editor_test_undo_raw(TextEditor* editor, int steps)
{
	editor->Undo(steps);
}

extern "C" void mqemu_text_editor_test_redo_raw(TextEditor* editor, int steps)
{
	editor->Redo(steps);
}
