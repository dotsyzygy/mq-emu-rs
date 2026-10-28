# mq-rs

Unofficial Rust bindings for writing [MacroQuest](https://github.com/macroquest/macroquest) plugins.

> **Unofficial.** mq-rs is an independent project. It is not affiliated with,
> endorsed by, or supported by the MacroQuest team; please don't take mq-rs
> issues to them.

> **Emulator only.** mq-rs targets the MacroQuest `emu` branch (RoF2 client,
> `EverQuestVersion` = `May 10 2013 23:30:08`), which is 32-bit. It is not
> intended for live EverQuest, and the crates refuse to build for anything
> other than `i686-pc-windows-msvc`.

## Why Rust for MacroQuest plugins

Compared with writing the same plugin in C/C++ against MQ2Main directly:

- **Game objects can't outlive the frame.** A `Spawn` borrows from the `Mq` handle a callback gets,
  so holding one past the callback (when the game may free it) is a compile error, not a crash next
  zone. Long-lived references are `SpawnId`s that are looked up again.
- **A bug doesn't take the client down.** A panic in plugin code is caught at the boundary, reported
  once in chat and counted in `Mq::stats`; EverQuest keeps running. In C++ the same
  null dereference or out-of-range index crashes the game.
- **Re-entrancy is handled for you.** Commands, chat and events that fire while the plugin is already
  busy are queued and delivered right after, in order, with a cap against feedback loops, instead of
  re-entering half-updated state.
- **Async instead of hand-rolled state machines.** Multi-pulse logic ("turn, wait until facing, walk
  for half a second, then stop") is a plain `async` task with `delay`/`wait_until`, run on the game
  thread with no locking.
- **Thread rules are in the types.** `Mq` can't leave the callback or the game thread, so game access
  from another thread doesn't compile.
- **Cargo.** The plugin builds with `cargo build` against an existing MacroQuest build, outside the
  MacroQuest solution, can use crates.io libraries, and links the C runtime statically (no Visual C++
  redistributable needed).
- **Testable without the game.** The runtime's dispatch, queueing and panic handling are tested
  against a fake host, and the bindings are checked against MacroQuest's own exports, headers and PDB
  (see [Tests](#tests)).
- **Raw access is still there.** Anything not wrapped yet is reachable through `mq_emu::sys`, with the
  same `unsafe` freedom as C++, confined to the code that needs it.

## Feature status

What the framework (`mq-emu`) supports today and what it doesn't yet.

> **Keep this current:** when a feature is added (or a gap is found), update these lists in the same
> change.

**Supported**

- Plugin lifecycle via `plugin!` and the `Plugin` trait: init/shutdown, pulse, incoming and written
  chat, zoning (begin/end/zoned), game state changes, spawns and ground items added/removed, UI
  clean/reload, HUD draw, macro start/stop, other plugins loading/unloading
- Chat output, with colors (`mq_emu::color`)
- Running commands (queued, immediate, timed); adding and removing slash commands
- Aliases, including ones saved to MacroQuest.ini
- Settings in INI files (`Ini`): the plugin's own (`mq.ini()`, `config\<Name>.ini` as a C++
  plugin's `INIFileName`) or any other; values as text, parsed or as MacroQuest's switches; keys and
  sections listed, written whole and removed; read and written through the same Windows functions
  as MacroQuest, so `${Ini}`, `/ini` and C++ plugins see the same values
- Chat events, as a macro's `#Event` or Lua's `mq.event` (`mq.add_event`): MacroQuest's own
  matcher (Blech), with `#1#`/`#name#` captures, `#*#` and `|${...}|` data, run on the game's chat
  and MacroQuest's, without color codes and with item links as their names (or kept). Handlers run
  right after the line arrives; no `/doevents`. Patterns that crash the matcher are refused
- Key binds (`mq.add_key_bind`): MacroQuest binds that `/bind` sets and MacroQuest.ini keeps,
  handlers run as the key goes down and up, removed when the plugin unloads (MacroQuest would keep
  calling the unloaded plugin); any bind's keys read, set and pressed; the game's own bindings of its
  commands read, listed and set. Keys and combos (`Key`, `KeyCombo`) in `/bind`'s notation
- Parsing `${...}` text
- Reading TLOs and members (`tlo`, `tlo_at`, `data("Target.Name")`) as typed values
- Providing TLOs from Rust (`add_tlo`) returning scalar values or values of the plugin's own
  datatypes
- Datatypes of the plugin's own (`Mq::add_type`), as a C++ plugin's `MQ2Type` or Lua's
  `mq.DataType`: members (with an index), methods and text, answered by the plugin, for macros
  and Lua scripts (`${Bot[Tank].HP}`, `mq.TLO.Bot('Tank').HP()`); members return values of the
  types too. A value is a key (an integer or text) that the members look up, so MacroQuest frees
  it without the plugin, even after the plugin unloads (a Lua script's member objects wait for
  garbage collection). Macro variables of them (`/declare`, set from a key with `/varset` or from a
  value with `/vardata`); when a type is removed or the plugin unloads, they become strings holding
  their keys (arrays of them, empty strings), because MacroQuest deletes a variable through its
  type and would call into the unloaded plugin. As with a C++ plugin's types, a value a Lua script
  keeps can't be used after the plugin unloads
- Spawns: you, your target, the spawn list, lookup by ID or name, nearest with a filter; name, level,
  type, position, heading, HP, mana, distance; pets and their owners
- Items (`Item`): name, ID, stack, charges, type, container slots and contents, weight, value,
  flags (lore, no drop, no rent, magic), levels, classes, equip slots, AC/HP/mana/endurance,
  damage, delay, haste, and the clicky/proc/worn/focus/scroll effects
- Inventory: worn slots (`InvSlot`, with MacroQuest's slot names), bags and their contents, the
  cursor, finding items by name, ID or filter, counting stacks, free bag slots; bank and shared bank
- Spells (`Spell`, by ID or by name, chosen among same-named spells as `${Spell[name]}` does):
  name, levels per class, mana/endurance cost, cast/recovery/recast times, range, duration,
  beneficial or not, target and resist type, skill, icons, categories, messages, effects
- Your buffs and songs (`Buff`: spell, caster level, ticks left, hit count), spell gems and spell
  book
- Your group: members (name, level, mercenary, offline, roles, spawn when in the zone), leader
- Buffs on others: your target's and your pet's buffs as their windows show them (`WindowBuff`:
  spell, time left, caster), whether the target's buffs have arrived, and MacroQuest's cache of the
  buffs of every spawn you've targeted (`CachedBuff`, as `${Spawn[...].Buff[n]}` reads them)
- Your raid: members (name, level, class, group, leader, main assist, marker, note), leader,
  loot type and looters (and who may loot under them), message of the day, pending invites
- Your mercenary: state (active, suspended, dead), name, stance and stances, its spawn, the
  mercenaries you own
- Extended targets: every slot's role (with the window's name for it), status, spawn and name,
  your aggro on it, and the spawns hating you
- Alternate abilities: the ones you own (by ID, name or rank index) and any the game knows;
  name, description, category, rank, cost, spell, reuse time, whether it's ready and how long
  until it is; AA points
- Skills: value with and without modifiers, cap at your level, name; abilities (the activated
  skills), whether they're ready and their reuse time
- The merchant window (what's for sale, buyback, markup, the selected item, whether it's full) and
  the loot window (the corpse and its items)
- EverQuest's own UI windows (`Window`): found as `${Window[...]}` finds them (by name, a bag's
  window as `pack1`, or a path to a control), every top-level window (with the item and spell
  display windows, which the game keeps apart), the focused one; read (shown,
  enabled, place on screen and in the parent, text as `${Window.Text}` reads it, tooltip, colors,
  style, scrolling, XML name, screen ID and type, the window tree), and by kind: lists, tree views
  and combo boxes (rows, cells, a cell's window, the selection), tabs, checked buttons, gauge values.
  Acted on as `/notify` and `${Window}` do: clicks (left and right, down, up, held), notifications
  from a control, selecting rows and tabs, typing into edit boxes, opening, closing, moving, colors
  and alpha; the open context menu read, picked from and closed (MacroQuest's `${Menu.Items}` and
  `menuselect` don't work on emu: the count they go by isn't the menu's on this client, reading 0
  in one session and 678130844 in another). A control is
  looked for only inside its window (MacroQuest's `Child[]` carries on into the windows after it)
- What EQ windows hold: item slots (`ItemSlotWindow`, `${Window.InvSlot}`: the place and item
  shown, quantity, selection, text, textures), found for a place as `/itemnotify` finds them
  (`Mq::item_slot_window`, with `/itemnotify`'s slot names in `SlotLocation::parse`: worn slots,
  `pack1`, `bank3`, `loot2`, `trade1`, `enviro1`, `in pack1 3`) and clicked like any window; hot
  bars and hot buttons (`HotButtonWindow`, `${Window.HotButton}`: type, slot, label, item, the
  spell gem, social or alternate ability, the page shown, pressing one); the trade window (both
  sides' ready flags, what's offered, its slots); `link` notifications, whose data is the link's
  code (`Window::notify_link`)
- A plugin's own EQ UI windows: UI XML files added as `AddXMLFile` adds them (taken out when the
  plugin unloads), and windows made from their screen pieces as C++ plugins' `CCustomWnd` makes
  them (`Mq::add_window`), with a handler for each notification the window gets (from its
  controls or `/notify`). The runtime makes them whenever the UI loads and deletes them before it's
  cleaned up and when the plugin unloads (their code is in the plugin); one removed from its own
  handler is deleted once the game is done with it. With hooks (`Mq::add_window_with`,
  `WindowHooks`) a window draws on itself each frame, takes the mouse over it and may handle its
  notifications instead of passing them on
- Subclassing the game's own windows as C++ plugins' `WindowOverride` does (`Mq::subclass_window`,
  by name or path): the same hooks get a window's notifications and the mouse over it first (and
  may keep them from it), and draw on it after it draws. Subclassed as soon as the game has the
  window, taken off before the UI is cleaned up (and put back when it loads), when removed (even
  from its own hook) and at unload. It stacks with other hooks on the window: taken off from under
  another, it passes everything on for good, so a copy of its vtable never calls into an unloaded
  plugin. What a window gets while one of the plugin's callbacks runs (from the plugin's own
  click) is delivered after that callback, too late to keep from the window
- Drawing on a window as the game draws (`Painter`, from a draw hook): rectangles, text in EQ's
  fonts (measured), and the game's UI textures, clipped to the window's client area
- What `/itemnotify` does with a place no slot window shows (a closed bag): picking the item up and
  dropping the cursor's there (`Mq::pick_up_item`, `Mq::drop_item`), using it as `/useitem` does
  (`Mq::use_item`), and opening and closing bags (`Mq::open_bag`, `Mq::close_bag`). Outside bags
  these are MacroQuest's `PickupItem` and `DropItem`; inside one mq-emu does what they do (moves
  the item, then puts it on the cursor's icon) with the client's own `AttachToCursor` signature,
  because `PickupItem` and `DropItem` (and so `/itemnotify in ...`'s fallback) corrupt the stack
  there on emu: they call `AttachToCursor` as eqlib declares it, with 14 dwords of arguments, where
  this client's takes 8 (`ret 20h`: the item's GUID by reference, its ID, a name and the quantity,
  as the game's item slots call it). Found when the selftest crashed in game
- Zones: the zone you're in (ID, instance, names, type, gravity, safe point, clip planes) and every
  zone the client knows, by ID or name
- Ground items: the zone's list, lookup by ID, nearest with a filter; name, position, heading,
  weight, and the item itself
- Movement: movement keys, facing a heading or point (smooth or instant), stopping
- Async tasks on the game thread: `delay`, `wait_until`, timeouts, cancellation
- Actors messaging: mailboxes, send, reply, request with timeout, other characters and peers
- Lua interop: exchanging every actors value type with Lua scripts
- ImGui (`mq_emu::imgui`): windows, child regions, menus, tabs, tables, trees, popups, tooltips,
  text, buttons, checkboxes, radio buttons, selectables, sliders, drags, number and text inputs,
  combo boxes, color editing, progress bars, simple plots, ID/style/disabled scopes, invisible
  buttons, the cursor and mouse. Every begin/end pair is a closure scope that always ends, even on
  a panic, and ImGui errors are caught and reported instead of pausing every plugin's overlay
- Draw lists on a window, over every window or under them (`draw_list`, `foreground_draw_list`,
  `background_draw_list`): lines, rectangles (rounded per corner, four-color), triangles, quads,
  circles, regular polygons, ellipses, Bezier curves, polylines, convex and concave polygons, text
  in any font, images, paths (lines, arcs, curves) stroked or filled, clip rectangles and channels
- Textures and images: image files loaded through the game's renderer (`Ui::load_texture`, freed
  only after the frames that drew them), images and image buttons, and the game's own UI textures
  such as item and spell icons (`Ui::find_texture_animation`), which stop drawing when the game
  unloads its UI rather than touching freed textures
- Drag and drop between items and plots, carrying bytes of a named kind
- Fonts: MacroQuest's default, console and large fonts, EverQuest's fonts, font files a plugin
  loads (removed when it unloads), and any size
- Docking: dock spaces (in a window or over the screen) and docking windows into them; layouts
  built in code with ImGui's dock builder (`Ui::dock_builder`: dock space and floating nodes,
  splits, docking windows, copying nodes and dock spaces, ImGui's internal node flags) and read back
  (`Ui::dock_node`). Node 0, which ImGui takes as every node, and rebuilding nodes from inside a
  window docked in them are refused rather than undoing everyone's docking or crashing
- Multi-select (`Ui::multi_select`): lists whose items are selected with ctrl+click, shift+click,
  ctrl+A, the keyboard or a dragged box, into a `Selection` or a selection kept elsewhere
- Custom widgets, from ImGui's internal API: items laid out with an ID (`Ui::add_item`), their
  button behavior with ImGui's private press modes (`Ui::button_behavior`), the active, hovered and
  focused items, splitters (`Ui::splitter`), frames, clipped text and the nav cursor in the
  style's look (`Ui::render_frame` and the like), arrows, bullets and check marks on draw lists,
  buttons with private flags (`Ui::button_with`, `Ui::arrow_button_with`) and vertical separators.
  ID 0, bad axes and directions and separators without thickness, which a release `imgui.dll` only
  asserts on, are refused
- Any ImGui window, this plugin's, another plugin's or MacroQuest's, from ImGui's internal API:
  found by name or ID (`Ui::find_window`), listed back to front (`Ui::windows`), the current,
  focused and hovered ones, and read (`WindowInfo`: kind, parent and root, place, size, content
  size, scrolling, flags, display and focus order, collapsed). Moved, sized and collapsed,
  focused, brought to the front or back or behind another, and scrolled to a place, a position or
  a rectangle (`WindowRef`); the current window scrolled to the cursor (`Ui::set_scroll_here_y`)
  or the last item (`Ui::scroll_to_item`). Windows are found again by ID on every call, so no
  window pointer reaches Rust. Unknown windows, combined conditions, scroll ratios outside 0 to 1
  and two ways to scroll one axis, which a release `imgui.dll` dereferences or only asserts on,
  are refused
- The list clipper (`Ui::list_clipper`, `Table::list_clipper`, `MultiSelect::list_clipper`): long
  lists of evenly spaced items that only draw the ones in view, in a window, over a table's rows or
  in a multi-select list (always drawing the item shift+click selects from, as ImGui needs).
  Stepping a clipper that has ended, including items after the first step and a first item that
  doesn't move the cursor are refused rather than crashing or misdrawing. A list of unknown length
  isn't supported
- ImPlot (`Ui::plot`, `Ui::subplots`): line, scatter, stairs, shaded, bar, bar group, error bar,
  stem, infinite line, pie, heatmap, histogram (1D and 2D), digital, image, text and dummy items
  over every numeric type; axis setup (labels, limits, formats, ticks, scales, constraints),
  legends, drag points/lines/rectangles, annotations, tags, queries, legend popups, drawing in the
  plot, plot styles and colormaps. Setup after plotting panics before ImPlot sees it
- ImPlot's callback forms: items whose points come from a closure (`Plot::line_fn` and the
  like), axis tick formatters and custom scales from closures (`Plot::setup_axis_format_fn`,
  `Plot::setup_axis_scale_fn`, which may borrow from outside the plot but not from inside it, and
  which ImPlot forgets when the plot ends), and pie chart labels. A panic in one is caught where
  ImPlot called it and resumed once the item or plot is done. ImPlot's input map (which buttons pan,
  select, fit and open menus), for this plugin's plots or everyone's
- MacroQuest's EQ-styled widgets: spell gems as the game draws them (`Ui::spell_gem`, optionally
  casting, forgetting or picking up the spell when clicked, and on draw lists), item link text
  (`Ui::item_link_text`) and text laid out in EverQuest's fonts (`DrawList::add_eq_text`)
- Runtime stats and safety nets: caught panics, slow-pulse warnings, queue limits
- Generated raw layouts for spawns, your character and profile, the spawn list, items, spells,
  groups, zones, key state, ground items, client state, the target, pet, merchant and loot windows,
  raids, mercenaries, extended targets, aggro, alternate abilities and skills, and UI windows (the
  window manager, buttons, lists, combo boxes, tabs, gauges, rich text, context menus, item slots
  and their manager, hot buttons and bars, the trade window) (via `sys`)

**Missing**

- Game data beyond the bound set: advanced loot, tasks, achievements, the friends list, tribute
  (reachable as TLO reads, or through their windows)
- Acting on spells (casting, memorizing) and on items beyond moving whole stacks and using them
  (splitting stacks, auto-inventory, selling) (only by running commands)
- Datatypes beyond the bound set: returning MacroQuest's own objects (spawns, items...) from TLOs
  and members (`Value::Object` gives NULL), types inheriting another's members
  (`SetInheritance`), and members added to MacroQuest's own types (`AddMQ2TypeExtension`)
- ImGui beyond the bound set: MacroQuest's other EQ UI drawing (`DrawUITexture`,
  `DrawTexturePiece`, `DrawScreenPiece`) and its ImGui add-ons (file dialog, text and memory
  editors) (reachable as raw `ImGui::*` C++ only)
- The rest of ImGui's internal API (`imgui_internal.h`), in the pieces planned to bind next (custom
  widgets, and windows and scrolling, are done):
  - Input ownership and focus: key owners, shortcut routing, keyboard navigation and focusing or
    activating items by ID, typing-select (`SetKeyOwner`, `TestKeyOwner`, `SetShortcutRouting`,
    `FocusItem`, `ActivateItemByID`, `SetNavID`, `GetTypingSelectRequest`)
  - Lower-level widget behaviors: `DragBehavior`, `SliderBehavior`, `TreeNodeBehavior`,
    `TempInputText`/`TempInputScalar`, `ScrollbarEx`, `CloseButton`, `CollapseButton`,
    `ImageButtonEx`, `TextEx`/`TextAligned`, wrapped and ellipsized text rendering and the
    `ShadeVerts*` helpers
  - Popup, tooltip, menu and combo internals: `OpenPopupEx`, `BeginPopupEx`, `ClosePopupToLevel`,
    `BeginTooltipEx`, `BeginComboPopup`, `MenuItemEx`/`BeginMenuEx` (with icons)
  - Table and tab bar internals: column widths, sorting and settings, and finding, closing and
    reordering tabs (`TableGetColumn*`, `TableSetColumnWidth`, `TabBarFindTabByID`,
    `TabBarCloseTab`, `TabBarQueueReorder`, `TabItemEx`)
  - Ini settings handlers: a plugin's own data saved in MacroQuest's ImGui ini, next to window
    positions (`AddSettingsHandler`, `MarkIniSettingsDirty`). The handler calls into the plugin, so
    it must be removed when the plugin unloads, as loaded fonts and textures are
  - Debug tools for building plugins: outlining the last item, writing to ImGui's debug log, the
    ID stack tool (`DebugDrawItemRect`, `DebugLog`, `DebugLocateItem`, `ShowIDStackToolWindow`)
  - Not planned: what acts on the ImGui context every plugin shares, which MacroQuest sets up,
    runs and renders. A plugin calling these would break other plugins' windows or crash the
    game, or change what isn't its to change (context setup, `NewFrame`/`Render`, viewports,
    localization, garbage collection, error recovery, the font atlas builder). Also not planned:
    context hooks (`Plugin::on_update_imgui` already runs every frame) and the legacy Columns API
    (tables replace it)
- EQ UI windows beyond the bound set: hooking more of a window's virtual functions than its
  notifications, the mouse and drawing (the keyboard, showing and hiding, each frame's
  processing, tooltips, drag and drop), and plugin windows made in code rather than from a screen
  piece
- Hooks/detours on game functions
- Live EverQuest and 64-bit builds (out of scope: emu/RoF2 only)

**Long term**

- Better UI than raw ImGui: once ImGui is bound, look at the UI problems plugins actually have
  (layouts, forms, live lists and tables, settings screens, HUDs, theming, state that survives
  reloads) and see whether `mq-emu` can offer its own higher-level API for them, built on ImGui or
  giving access to better UI systems, rather than only mirroring ImGui's calls

## Crates

| Crate | Published | Purpose |
|---|---|---|
| `crates/mq-emu-sys` | yes | Raw, `unsafe` 1:1 FFI declarations for `MQ2Main.dll` / `eqlib.dll` exports |
| `crates/mq-emu` | yes | Safe, idiomatic API on top of `mq-emu-sys`; what plugins depend on |
| `plugins/MQ2RustPluginTest` | no | Test plugin exercising the bindings in-game (builds `MQ2RustPluginTest.dll`) |
| `tools/mq-emu-bindgen` | no | Generates `mq-emu-sys`'s game-object layouts from `eqlib.pdb` |
| `tools/lua-proto-oracle` | no | Test-only: MQ2Lua's message format via real C++ protobuf, to check `mq-emu::lua` against |

## Trying it in game

```powershell
./scripts/deploy-plugin.ps1        # or, from bash: scripts/deploy-plugin.sh
```

This builds `MQ2RustPluginTest` in release and copies the DLL (and its `.pdb`) into
`$MQ_ROOT/build/bin/release/plugins`, plus the plugin's Lua scripts (`plugins/<name>/lua/*.lua`)
into `$MQ_ROOT/build/bin/release/lua` and its resources (`plugins/<name>/resources`, such as UI XML
in `uifiles/default`) into `$MQ_ROOT/build/bin/release/resources`, finding `MQ_ROOT` the same way the build does: from your
environment, or from `[env]` in `~/.cargo/config.toml`. Use `-Plugin <name>` for another plugin
under `plugins/`, `-DebugBuild` for a debug build, and `-NoBuild` to only copy (from bash:
`--plugin <name>`, `--debug`, `--no-build`). If the plugin is
loaded in game, `/plugin MQ2RustPluginTest unload` first so the DLL can be replaced. Then in game:

| Command | What it does |
|---|---|
| `/plugin MQ2RustPluginTest` | Load it (`/plugin MQ2RustPluginTest unload` to unload) |
| `/rusttest selftest` | Runs every check and prints PASS/FAIL/SKIP for each (details below) |
| `/rusttest stats` | Pulse timing, queued/dropped events, panics |
| `/rusttest stop` | Cancels a running selftest and stops all movement |
| `/rusttest window` | Toggles the debug window: selftest results, runtime stats, the game as mq-emu sees it (with every open EQ window's controls, what item slots and hot buttons hold, the hot bars and the trade window), every bound ImGui widget to try by hand (multi-select, a clipped list and custom widgets too, and every ImGui window to bring to the front or focus), and tabs for ImPlot (with closures, a custom scale and the mouse map), draw lists, images and icons (with drag and drop, spell gems, item links and EQ text), fonts and docking (with a dock builder layout) |

The selftest compares mq-emu's view of the game with MacroQuest's `${Me}`/`${Target}` data
(spawns, inventory and bags, bank, buffs, gems, spell book, spells, group, pet, zone, ground items,
target and pet buffs, raid, mercenary, extended targets, AAs, skills, the merchant and loot
windows when open, every EQ window and the controls of the open ones against `${Window}`, and every
item slot and hot button against `${Window[...].InvSlot}` and `.HotButton`), and exercises commands, aliases, settings, chat events, key binds, re-entrancy, parsing, keys and async
tasks. It also:

- **Touches MacroQuest.ini:** saves a `/rusttestsaved` alias to it and deletes it again, and binds a
  `RustTestBind` key bind to ctrl+shift+F11 (and alt+Num_5) while it runs, then removes it and its
  saved keys.
- **Writes a bind dump:** `/dumpbinds rusttest-binds` writes every key binding to
  `config\rusttest-binds.cfg`, which it compares with mq-emu's and deletes. Skipped when a game
  command is bound to a key MacroQuest has no name for, which would crash `/dumpbinds`.
- **Echoes test lines** for the chat events, one of them with a link to an item you carry.
- **Declares global macro variables** (`rusttest_v1`, `rusttest_v2`, `rusttest_v3`, `rusttest_va`,
  `rusttest_t`, `rusttest_ta`) of its `RustTestCounter` and `RustTestTemp` datatypes, then deletes
  them.
- **Touches its own INI file:** writes a `[RustTestSelftest]` section to
  `config\MQ2RustPluginTest.ini` (also through `/ini`), compares it with `${Ini}`, and removes it.
- **Moves you:** turns 100° and back, then walks forward and back for half a second each.
- **Works your windows:** opens the inventory window, moves it a pixel and back, switches one of its
  tabs and back, closes it with its done button, right-clicks it for its context menu (closed
  again), and types into the chat input (put back as it was). The inventory window ends up open or
  closed as it was. It right-clicks your first bag's slot to open the bag and again to close it,
  sends the chat window a link to an item you wear (opening it in an item display window, closed
  again), and presses an empty hot button if you have one. It subclasses the inventory window
  (drawing a yellow bar along its top while it's open), keeps one click of its done button from
  closing it, clicks the window itself, and takes the subclass off, after which the done button
  closes it. It picks up an item from a closed bag and puts it back (skipped with something on
  the cursor or the merchant window open).
- **Reloads your UI, if it must:** the plugin adds `MQUI_RustTestWnd.xml` (deployed to
  MacroQuest's `resources\uifiles\default`) and a window made from it when it loads; if the UI
  hasn't read the file yet, the selftest runs `/reloadui`, which **freezes the game** for a while
  (it says so in chat first, and reports how long it took). It's needed once per UI load: load the
  plugin before entering the world (at character select, or autoloaded) and the in-game UI reads
  the file with no reload. It then clicks the window's button, types into it, sends it a link and
  `/notify`, removes it and adds it again, and leaves it hidden. The window draws a line of text
  on itself.
- **Targets something** when nothing is targeted (the nearest spawn, then the nearest NPC for the
  target buff checks, waiting for its buffs to arrive), and clears that target afterwards.
- **Forms a raid** when you aren't in one: invites the other characters running the plugin, has
  them accept, checks the raid, and disbands it. The server won't raid-invite your own group, so
  grouped characters leave the group first and are invited back afterwards: you end up leading the
  group, and group roles (main assist and the like) need setting again. Mercenary checks are
  skipped (none on the server it's tested on).
- **Triggers the runtime's safety nets on purpose:** expect a caught-panic message, a slow-pulse
  warning, and a burst of `flood` lines that the runtime cuts off.
- **Opens the debug window** (expanding it if it's collapsed), to check it draws without ImGui
  errors, and leaves it open with the results.
- **Flashes a graphics window** for a few frames, drawing every ImPlot item, draw list feature,
  font, image and drag and drop, a multi-select list, a clipped list, a custom widget and a splitter, plots from closures, finds its own window by name and brings
  it to the front, scrolls a child window by its ID, your spell gems and EQ
  text, with a yellow outline around the screen; it writes a small BMP to the temp directory to load
  as a texture. With docking on, it lays out a small dock space with the dock builder (which ImGui
  keeps in MacroQuest's ImGui ini, like any dock space).
- **Messages:** sends to itself through the launcher.
- **Lua:** when `/lua run rustpeer` is running, exchanges every Lua value type with that script, both
  ways.
- **Other characters:** finds every other character with the plugin loaded, then pings each one and
  shows it a line. Where `rustpeer` runs on them too, it checks Rust and Lua across characters.

Checks that can't run say why and are counted as skipped. Target something first to include the
target checks.

## Talking to Lua scripts

Lua scripts that use MacroQuest's `actors` module send one value per message: a number, boolean,
string, table or ImGui vector. `mq_emu::lua` reads and writes those as `LuaValue`/`LuaTable`:

```rust
use mq_emu::{Address, LuaTable, LuaValue};

// To a script that called actors.register(handler), on character Bob.
let hello = LuaTable::new().with("id", "hello").with("level", 65);
mq.send(&Address::lua_script("myscript").on_character("Bob"), LuaValue::from(hello).encode())?;

// From Lua: in a mailbox handler, or on a request's reply.
let value = message.lua()?;
let id = value.get("id").and_then(LuaValue::as_str);
```

A script reaches the plugin's default mailbox with
`{ character = ..., mailbox = 'Example', absolute_mailbox = true }` (for `MQ2Example.dll`), or a named
one with `mailbox = 'Example:name'`. `plugins/MQ2RustPluginTest/lua/rustpeer.lua` shows both directions.

## Game-object layouts

`mq-emu-sys::layouts` (spawns, your character, the spawn list, zone info, key state…) is generated
from `eqlib.pdb`, the compiler's own record of every eqlib class layout, and committed so users and
docs.rs never need the PDB. To add types, list them in `crates/mq-emu-sys/layouts.txt`, then run:

```sh
cargo run -p mq-emu-bindgen            # regenerate crates/mq-emu-sys/src/layouts/generated.rs
cargo run -p mq-emu-bindgen -- --check # fail if the committed file is out of date
```

If the generator reports that the PDB has several different definitions of a type, the PDB has
outlived an older build: MSVC's linker updates an existing PDB in place and never removes old type
records. Delete `build/bin/<config>/eqlib.pdb` and `eqlib.dll`, rebuild MacroQuest, and regenerate.

Each record is an opaque byte block with accessors named after the C++ members (`X()`,
`set_Heading()`), including inherited ones. Records contained by value are generated in full;
types only reached through pointers are opaque.

## Building

1. Build MacroQuest emu (`Release|Win32`) so `build/bin/release` contains `MQ2Main.lib` and `eqlib.lib`.
2. Set `MQ_ROOT` to that checkout, either as an environment variable or in your
   user-level `~/.cargo/config.toml` (it is machine-specific, so it is not in the repo):
   ```toml
   [env]
   MQ_ROOT = "C:/path/to/macroquest"
   ```
   `MQ_BUILD_CONFIG` selects `build/bin/<config>` and defaults to `release`.
3. `cargo test`

The repo's `.cargo/config.toml` only sets the target to `i686-pc-windows-msvc`
(`rustup target add i686-pc-windows-msvc` if needed).

## Layout

This is a Cargo workspace for convenience (shared lockfile and `target/`), but each crate stands on
its own: it can be built from its own directory and published separately. Crates depend on each
other via `[workspace.dependencies]` entries that give both a `path` and a `version`.
`crates/mq-emu-sys/tests/consumer.rs` builds mq-emu-sys from a standalone consumer crate with no user cargo
config and only explicitly passed settings, to check what a downstream user will see.

## Tests

`cargo test` checks the bindings against the MacroQuest build without running EverQuest:

- **`tests/exports.rs`**: every declared function and global exists in the DLL export tables, and the DLLs are x86.
- **`tests/header_constants.rs`**: mirrored constants, the key name table and `EVERQUEST_VERSION`
  match the MQ headers.
- **`tests/link.rs`**: compiles single-symbol probe DLLs against the import libraries.
- **`examples/link_check.rs`**: a `cdylib` that references every import, so each one must resolve at link time.
- **`tests/layouts.rs`**: every generated field offset and record size matches eqlib's header offset
  comments and `*_size` constants, an independent source (about 2,500 fields are covered; classes
  whose comments are known not to describe the 32-bit build are listed, and the test fails if one
  starts matching).
- **`tests/accessors.rs`**: generated accessors read and write the right bytes, including bitfields,
  C++ `bool`, nested records and arrays of records.
- **`tools/mq-emu-bindgen/tests`**: the committed layouts are up to date with `layouts.txt` and the PDB,
  and generation is deterministic.
- **`tests/plugin_loader.rs`**: the plugin exports we document match every name MacroQuest's loader
  looks up, in both directions.
- **`tests/shim.rs`**: the C++ shim against a fake `MainInterface`: arguments, handles and callback
  routing, without MacroQuest; and that item definitions come through the item's vtable (the game's
  implementation), from the slot eqlib's headers give. Subclassing, against a fake window whose
  vtable holds Rust `thiscall` functions: the replaced entries are eqlib's (read from its header),
  each reaches the hook and then the window's own function with its arguments and result, a
  handled event skips the window, only the kinds asked for reach the hook, the rest of the vtable
  and its RTTI pointer are kept, a window is subclassed once, and taken off the window has its own
  vtable back. Another window given our table (as `WindowOverride` shares its own) only reaches its
  own functions; a hook that copied our table keeps working after ours comes off, going straight to
  the window's functions; a deleted window isn't touched; the hook can take its own subclass off;
  windows without a readable vtable are refused.
- **`tests/events.rs`**: the event matcher, MacroQuest's Blech compiled into the shim: whole lines
  match ignoring case, captures by number and name, wildcards, escapes and print variables, every
  match in the order events were added, and removed events stop matching. The guards: patterns
  without text, deeper than Blech's fixed match stack (or making another pattern so, which leaves
  that one as it was) and with too many print variables are refused, with patterns right at each
  limit still matching. A child process runs Blech without each guard and crashes (an access
  violation, or the C runtime ending the process).
- **`tests/imgui.rs`**: the ImGui shim against MacroQuest's real `imgui.dll`, in a headless

  context: every wrapper draws, simulated clicks register, and ImGui's C++ exceptions (calling `End`
  too often) are caught instead of crossing into Rust. A window left open makes the frame fail, as
  it would pause ImGui in game.
- **`tests/header_constants.rs`** also checks the window kinds (`UIType`), the notifications
  `/notify` sends (`eWndNotification`) and the context menu depth.
- **`tests/imgui_constants.rs`**: every ImGui and ImPlot flag and enum value matches MacroQuest's
  `imgui.h`, `imgui_internal.h` (the dock builder's node flags) and `implot.h`, and MacroQuest's
  spell gem and EQ text flags their headers; every public name of the bound enums is bound.
- **`tests/consumer.rs`**: builds mq-emu-sys as an outside crate would, checking each requirement
  (`MQ_ROOT`, 32-bit target, static C runtime, docs.rs) gives a clear error when missing.
- **`mq-emu` runtime tests**: dispatch, re-entrant events queued and delivered in order, the feedback
  loop cap, caught panics reported once, commands routed to closures, rate-limited warnings, all
  against a fake host and clock.
- **`mq-emu` game data tests**: each area against fake game objects and a stand-in for the shim's
  game functions, with missing data reading as nothing. Target and pet buffs only count with a
  target or pet (the windows keep stale ones), the windows' arrays are as long as the game's buff
  slots, caster names are found through the game's hash table chains (an empty or looping table
  finds nothing), and MacroQuest's cached buffs come back complete however many there are. A raid
  only exists once joined, and loot rules follow the loot type; mercenary states read as
  `${Mercenary.State}` names them; extended target aggro stops where the aggro meter does;
  untrained skills read 0, and abilities are only ready when known, activated and off cooldown;
  passive AAs are never ready; the merchant window needs to be shown and a merchant open.
- **`mq-emu` ImGui tests**: against the real `imgui.dll`, every scope ends when its closure panics,
  closed and collapsed windows skip their contents, clicks and typing (multi-byte, growing the
  buffer) reach Rust values, style variables have the right kinds, and table setup misuse panics
  before ImGui sees it. Doctests show that reaching a table from a window opened in one of its cells
  doesn't compile. Draw lists: every shape adds vertices on every list, colors pack exactly as
  ImGui packs them, paths, clip rectangles and channels stay balanced. A payload dragged with the
  simulated mouse is delivered to its target; windows dock into a dock space; fonts push and pop
  at their sizes, including a font file. Every ImPlot item draws for every value type, plot scopes
  end on a panic, and setup after plotting panics before ImPlot sees it. Textures, the game's UI
  textures and EQ fonts run against stand-ins for MacroQuest's functions: dropped textures are
  freed only once the next frame starts, and icons stop drawing once the UI unloads. Each guard
  has a negative test showing what ImGui or ImPlot does without it (`End()` throwing after
  `SetCursorScreenPos`, popping ImGui's own clip rectangle, a wrong-kind style variable, ImPlot
  keeping a freed formatter after its plot). Dock builder layouts dock windows where they say,
  copies keep their shape, and the shim refuses node 0, changes from inside a docked window,
  splitting a split node and copying a node over its own source (a layout built first survives).
  Multi-select: simulated clicks, shift and ctrl clicks, ctrl+A and a dragged box reach a
  `Selection`, which applies requests as ImGui's own storage does. The list clipper draws only the
  items in view (following the view down a 10,000-item list) and moves the cursor past the whole
  list, draws included items out of view, clips table rows, keeps a multi-select list's
  shift+click item drawn, ends when its closure panics (ImGui's clipper stack is back to empty)
  and nests; the shim refuses stepping an ended clipper and late or backwards included items.
  Each test starts without an earlier one's keyboard focus: ImGui's clipper also draws the rows
  where a window's clicked item last was, as a test shows.
  Custom widgets: items take their space at the cursor and are the last item, simulated clicks
  give hover, hold and press (on release, or as the mouse goes down), clearing the active item
  lets go, a dragged splitter moves size between its sides down to a minimum, private-flag and
  arrow buttons press, and the render helpers draw (text after `##` hidden); the shim refuses ID 0,
  bad axes and directions and separators without thickness.
  Windows: found by name with the ID their name hashes to, unknown names give none; child
  regions, popups and tooltips have their kinds, and child regions their parent and root and no
  focus order; the window list is in display order; bringing to the front, the back and behind
  another reorders them, and focusing brings to the front and last in focus order (clearing focus
  leaves none); focusing another window from inside a popup closes it and the frame ends cleanly;
  another window is moved, sized and collapsed; a window is scrolled to a place and a position by
  ID, the current window to the cursor (top and bottom of the view) and to its last item
  (centered), and to a rectangle by as far as it said; the shim refuses unknown windows, combined
  conditions, ratios outside 0 to 1, a bad axis and two ways to scroll one axis.
  ImPlot's closures: every point
  asked for and none past the count, formatters and custom scales take effect and are forgotten
  when the plot ends, panics in them resume afterwards, and a doctest shows a formatter borrowing
  from inside the plot doesn't compile. The input map changes for a scope and is restored, and
  maps ImGui would index out of bounds with are refused. Spell gems, item links and EQ text reach
  MacroQuest's stand-ins, and gems past the game's last aren't asked for.
- **`mq-emu` window tests**: against fake windows and a stand-in for the shim's window functions:
  the window tree is walked depth first, and a loop in it ends the walk; controls are found by name
  or screen ID ignoring case, only inside their window (a negative test shows the sibling MacroQuest
  would find); `window("Top/Control")` walks paths, skipping empty steps; text follows the kind
  (rich text's markup, a page's tab text); checked and gauge values are only read from their kinds;
  lists read rows, cells and a cell's window and find rows through either, a combo box is its
  drop-down; tabs read and select pages; long text grows the buffer to fit; the window list has only
  top-level screens; the open context menu is the current one, and past the array or with none
  visible there's none. Clicks, notifications, opening, moving, colors and typing reach the game as
  `/notify` names them. Subclasses: on as soon as the game has the window, off after
  `on_clean_ui` and back on when the UI loads, off at unload and when removed (from its own hook
  too), without touching a window the game deleted; one subclass per window; the plugin's answer
  reaches the window, except for what it got during one of the plugin's callbacks (delivered
  after); drawing during a callback is dropped; a panicking hook passes on. The painter clips to
  the window's client area (and less), and draws nothing without one. Plugin windows with hooks
  draw, take the mouse and handle notifications. Item actions pass the place as the game numbers
  it, and only your inventory's items are used.
- **`mq-emu` INI tests**: against real files through Windows' INI functions: values, parsed values
  and MacroQuest's switch words read back; a missing key is told apart from an empty one; values
  Windows would change on reading (spaces at either end, surrounding quotes) are quoted so they come
  back exactly, also through Windows alone as `${Ini}` reads them; long values and long key and
  section lists are read whole; UTF-8 is stored as MacroQuest stores it; names and values that
  would break the file are refused before anything is written; paths the ANSI code page can't hold
  are refused. Negative tests show what Windows does without each guard (missing and empty look
  the same, quotes and spaces are stripped, MacroQuest's buffer cuts long values, an `=` in a key or
  a newline in a value injects keys and sections). The plugin's own file is named as a C++
  plugin's.
- **`mq-emu` event and key bind tests**: events run after the chat callback that received their
  line (even one that panics), from the game's chat and MacroQuest's, with captures, print
  variables (against a stand-in for `${...}`), color codes stripped and item links reduced or kept
  (against a stand-in for the game's link cleaner), in the order added across both kinds; a removed
  event's waiting matches are dropped; handlers can add and remove events; an event feeding itself
  is capped. Key binds, against a stand-in following MacroQuest's rules: keys reach the handler
  going down and up, saved keys are loaded, presses from a callback run after it, bad and taken
  names (another plugin's, a game command's) are refused, keys MacroQuest can't name are never
  bound, only the plugin's own binds are removed, and all of them are at unload (the stand-in, like
  MacroQuest, would keep them). `Key` and `KeyCombo` write `/bind`'s notation and read every combo
  back except the two kinds MacroQuest can't either.

- **`mq-emu` `tests/plugin_exports.rs`**: a DLL built with `plugin!` exports everything the loader
  needs, carries the right `EverQuestVersion` bytes, doesn't need the Visual C++ redistributable, and
  imports `imgui.dll`.
- **`mq-emu` `tests/lua_oracle.rs`**: the Lua message encoding against real C++ protobuf, built
  from MacroQuest's own `Actor.pb.cc` (what MQ2Lua uses), in both directions, including truncated and
  invalid input both sides must reject.
- **Unit tests**: type layouts (`ForeignPointer`, callback pointers), the `imports!` macro, the version
  byte array, the generator's naming and enum literal rules, text conversion, headings and stats.

Test binaries never import MacroQuest's DLLs (checked by the tests themselves), so `cargo test` runs
without the game. The exception is the ImGui tests, which load `imgui.dll`; it needs only Windows,
and the build copies it where `cargo test` finds it. What only the game can show is covered by `/rusttest selftest`.

Each check also has negative tests showing it rejects bad input: names that aren't exported, wrong
constant values, and imports that don't link (a typo, or a symbol from the wrong DLL).

## License

mq-emu-rs is licensed under the [MIT License](LICENSE).

MacroQuest itself is GPL-2.0. Every plugin built with these crates links against MQ2Main.dll, so a
distributed plugin is subject to MacroQuest's license regardless of the license on these crates.
