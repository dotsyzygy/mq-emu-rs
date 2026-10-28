# Feature status

What `mq-emu` supports today, MacroQuest bugs it works around, and what it doesn't do yet.

> **Keep this current:** when a feature is added (or a gap is found), update these lists in the same
> change. Keep **Supported** to one short line per area and record what an area lacks under
> **Missing**; details belong in the rustdoc.

**Supported**

Each area is supported in full, except for what's listed under **Missing**.

- **Plugin basics:** the `plugin!` macro and `Plugin` trait with every MacroQuest callback; chat
  output with colors; running, adding and removing commands; aliases (temporary or saved); settings in INI files, shared with `${Ini}`, `/ini` and C++ plugins
- **Events and keys:** chat events as a macro's `#Event` (MacroQuest's own matcher, run as lines
  arrive, no `/doevents`); key binds of the plugin's own, and reading, setting and pressing any bind
- **MacroQuest data:** parsing `${...}`, reading TLOs as typed values, and providing TLOs and
  datatypes of the plugin's own (members, methods, macro variables, inheriting another type's
  members) to macros and Lua, returning MacroQuest's own values (spawns, items...) from them, and adding members to MacroQuest's types
- **Game data:** spawns, items, inventory, bank, spells, buffs (yours, your target's and pet's, MacroQuest's cache), spell gems and book, group, raid, mercenary, extended targets, alternate abilities, skills, zones, ground items, the merchant and loot windows, tasks (objectives, progress, shared task members), the friends list, tribute and achievements
- **Acting in game:** movement and facing; casting, stopping, memorizing and forgetting spells;
  picking up, dropping, splitting and using items, `/autoinventory`, opening bags; buying and selling at a merchant
- **EverQuest's UI windows:** found as `${Window}` finds them, read, and acted on as `/notify` does
  (clicks, lists, tabs, typing, context menus); item slots, hot buttons and the trade window; the
  plugin's own windows from UI XML; subclassing the game's windows to see their notifications and
  the mouse first and draw on them
- **ImGui and ImPlot:** widgets as closure scopes that always end, even on a panic, with ImGui's
  errors caught; draw lists, textures and the game's icons, fonts, drag and drop, docking and the
  dock builder, multi-select, the list clipper, custom widgets and any window's placement and
  scrolling from `imgui_internal.h`; all of ImPlot, with its callback forms; MacroQuest's
  EQ-styled widgets (spell gems, item links, EQ text) and EQ UI drawing (the game's UI texture
  files and pieces of them, screen pieces); MacroQuest's ImGui add-ons: the text editor (with
  languages of your own, by regexes or a tokenizer function), the memory editor (with closures for its bytes) and the file dialog
- **Runtime:** async tasks on the game thread, caught panics, re-entrant callbacks queued, slow-pulse warnings and queue limits
- **Talking to others:** actors messaging between plugins and characters, and with Lua scripts
- **Raw access:** generated layouts of the game's objects, and every import, through `sys`

**Found on emu, and worked around**

- MacroQuest's `PickupItem` and `DropItem` (and so `/itemnotify in ...`'s fallback) corrupt the stack for places inside bags: they call `AttachToCursor` as eqlib declares it, with 14 dwords of arguments, where this client's takes 8. mq-emu moves those items with the client's signature.
- `${Menu.Items}` and `menuselect` find nothing: the count they go by isn't the menu's on this client. mq-emu reads and picks from context menus itself.
- Forgetting gem 16 or later stops the client on a breakpoint (`int3`); gems past the 12 are refused.
- The game's `/autoinventory` does nothing unless the cursor's icon shows the item, and `CInvSlot::SliderComplete` (the quantity window's split) leaves the icon alone. mq-emu shows it after splitting and before `/autoinventory`.
- MacroQuest keeps calling a plugin's key binds after it unloads, never removes its aliases, and deletes macro variables through their datatype; mq-emu removes binds and aliases at unload and turns variables of its types into strings first.
- `Child[]` carries on into the windows after the one searched; mq-emu looks only inside it.
- MacroQuest's `DrawUITexture` draws at the screen's top left rather than the cursor, and nothing
  at its default size; mq-emu lays the item out itself and always passes a size. A texture piece's
  default size is the whole texture's; mq-emu draws a piece at its own size.
- The text editor's undo moves the start of the step it undoes (and redo the start of the one it
  redoes), so after an undo, redo deletes nothing, and after a redo, undo removes nothing. mq-emu
  puts each step's starts back around undo, redo and rendering (ctrl+Z and ctrl+Y). Its
  `IsHandleMouseInputsEnabled` reports the keyboard setting; mq-emu remembers the mouse one.

**Missing**

- Advanced loot (`${AdvLoot}`): the RoF2 client has no advanced loot window (the selftest checks), and MacroQuest's emu build has no offsets for its actions, so there is nothing to bind
- Acting on spells and items beyond the bound set: splash spells landed at a place (MacroQuest's `/cast ... loc`), spell sets saved in the game (`/memspellset`), destroying items, and opening a merchant's window (only by running commands)
- Methods added to MacroQuest's types: MacroQuest looks only for members before asking a type's extensions, so an extension's methods would never be called (extensions have members only)
- ImGui add-ons beyond the bound set: the memory editor's data preview type and endianness, which
  only its own options menu sets. MacroQuest's `DrawScreenPiece` draws only animations (text, frames and headers are
  left to its TODO)
- The rest of ImGui's internal API (`imgui_internal.h`), in the pieces planned to bind next (custom widgets, and windows and scrolling, are done):
  - Input ownership and focus: key owners, shortcut routing, keyboard navigation and focusing or activating items by ID, typing-select (`SetKeyOwner`, `TestKeyOwner`, `SetShortcutRouting`, `FocusItem`, `ActivateItemByID`, `SetNavID`, `GetTypingSelectRequest`)
  - Lower-level widget behaviors: `DragBehavior`, `SliderBehavior`, `TreeNodeBehavior`,
    `TempInputText`/`TempInputScalar`, `ScrollbarEx`, `CloseButton`, `CollapseButton`,
    `ImageButtonEx`, `TextEx`/`TextAligned`, wrapped and ellipsized text rendering and the
    `ShadeVerts*` helpers
  - Popup, tooltip, menu and combo internals: `OpenPopupEx`, `BeginPopupEx`, `ClosePopupToLevel`, `BeginTooltipEx`, `BeginComboPopup`, `MenuItemEx`/`BeginMenuEx` (with icons)
  - Table and tab bar internals: column widths, sorting and settings, and finding, closing and
    reordering tabs (`TableGetColumn*`, `TableSetColumnWidth`, `TabBarFindTabByID`,
    `TabBarCloseTab`, `TabBarQueueReorder`, `TabItemEx`)
  - Ini settings handlers: a plugin's own data saved in MacroQuest's ImGui ini, next to window
    positions (`AddSettingsHandler`, `MarkIniSettingsDirty`). The handler calls into the plugin, so it must be removed when the plugin unloads, as loaded fonts and textures are
  - Debug tools for building plugins: outlining the last item, writing to ImGui's debug log, the
    ID stack tool (`DebugDrawItemRect`, `DebugLog`, `DebugLocateItem`, `ShowIDStackToolWindow`)

- EQ UI windows beyond the bound set: hooking more of a window's virtual functions than its notifications, the mouse and drawing (the keyboard, showing and hiding, each frame's processing, tooltips, drag and drop), and plugin windows made in code rather than from a screen piece
- Hooks/detours on game functions

**Out of Scope**

- Live EverQuest and 64-bit builds (emu/RoF2 only). Possibly ToB in the future
- What acts on the ImGui context every plugin shares, which MacroQuest sets up, runs and renders. A plugin calling these would break other plugins' windows or crash the game, or change what isn't its to change (context setup, `NewFrame`/`Render`, viewports, localization, garbage collection, error recovery, the font atlas builder).
- ImGui context hooks (`Plugin::on_update_imgui` already runs every frame) and the legacy Columns API (tables replace it)

**Long term**

- Better UI than raw ImGui: once ImGui is bound, look at the UI problems plugins actually have (layouts, forms, live lists and tables, settings screens, HUDs, theming, state that survives reloads) and see whether `mq-emu` can offer its own higher-level API for them, built on ImGui or giving access to better UI systems, rather than only mirroring ImGui's calls
