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

## Crates

| Crate                       | Published | Purpose                                                                                  |
| --------------------------- | --------- | ---------------------------------------------------------------------------------------- |
| `crates/mq-emu-sys`         | yes       | Raw, `unsafe` 1:1 FFI declarations for `MQ2Main.dll` / `eqlib.dll` exports               |
| `crates/mq-emu`             | yes       | Safe, idiomatic API on top of `mq-emu-sys`; what plugins depend on                       |
| `plugins/MQ2RustPluginTest` | no        | Test plugin exercising the bindings in-game (builds `MQ2RustPluginTest.dll`)             |
| `tools/mq-emu-bindgen`      | no        | Generates `mq-emu-sys`'s game-object layouts from `eqlib.pdb`                            |
| `tools/lua-proto-oracle`    | no        | Test-only: MQ2Lua's message format via real C++ protobuf, to check `mq-emu::lua` against |

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

| Command                     | What it does                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/plugin MQ2RustPluginTest` | Load it (`/plugin MQ2RustPluginTest unload` to unload)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `/rusttest selftest`        | Runs every check and prints PASS/FAIL/SKIP for each (details below)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `/rusttest stats`           | Pulse timing, queued/dropped events, panics                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `/rusttest stop`            | Cancels a running selftest and stops all movement                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `/rusttest window`          | Toggles the debug window: selftest results, runtime stats, the game as mq-emu sees it (with every open EQ window's controls, what item slots and hot buttons hold, the hot bars and the trade window), every bound ImGui widget to try by hand (multi-select, a clipped list and custom widgets too, and every ImGui window to bring to the front or focus), and tabs for ImPlot (with closures, a custom scale and the mouse map), draw lists, images and icons (with drag and drop, spell gems, item links and EQ text), fonts and docking (with a dock builder layout), and the add-ons (UI texture files and screen pieces, the text and memory editors and the file dialog) |

The selftest compares mq-emu's view of the game with MacroQuest's `${Me}`/`${Target}` data
(spawns, inventory and bags, bank, buffs, gems and whether they're ready, spell book, spells, group, pet, zone, ground items,
target and pet buffs, raid, mercenary, extended targets, AAs, skills, the merchant and loot
windows when open, tasks and their objectives, the friends list, tribute and achievements (skipped
when the server sends none), every EQ window and the controls of the open ones against `${Window}`, and every
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
  `rusttest_t`, `rusttest_ta`) of its `RustTestCounter` and `RustTestTemp` datatypes, and a spawn
  variable (`rusttest_sp`) set from its `RustTestBot` datatype, then deletes them.
- **Adds members to MacroQuest's types:** `RustTestTag` on `spawn` and `character` while the plugin
  is loaded, and `RustTestTemp` on `int`, removed again.
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
- **Casts and stops a spell:** a self or group buff (casting for a second or more) from a ready
  gem, stopped before it lands, so no mana is spent. Skipped for bards, and while a merchant,
  trade, give, loot or bank window is open (the game won't cast then).
- **Memorizes and forgets a spell:** a spell from your book into an empty gem, then forgotten
  again; with no empty gem, your last gem's spell is forgotten and memorized back (if that fails,
  the gem is left empty and the failure says so). Memorizing sits you down; you're stood back up
  if you were standing.
- **Splits a stack:** one off a stack in a bag (a closed bag if there's one), dropped back on, then
  one off again put away with `/autoinventory` (which may join another stack of the same item).
  Skipped with something on the cursor or the merchant window open.
- **Selects at a merchant**, when its window is open: one of its items, then one of yours.
  Nothing is bought or sold.
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
- **`tests/imgui_addons.rs`**: the text editor, memory editor and file dialog against the real
  `imgui.dll`. Values set through `imgui.dll`'s methods and read through the inline ones the shim
  compiles (and the other way) show the two agree on the editor's layout. The editor's own undo
  and redo are shown losing their place, and the shim's keeping it, from code and the keyboard.

- **`tests/header_constants.rs`** also checks the window kinds (`UIType`), the notifications
  `/notify` sends (`eWndNotification`) and the context menu depth.
- **`tests/imgui_constants.rs`**: every ImGui and ImPlot flag and enum value matches MacroQuest's
  `imgui.h`, `imgui_internal.h` (the dock builder's node flags) and `implot.h`, and MacroQuest's
  spell gem and EQ text flags, screen piece kinds and the add-ons' enums (the text editor's are
  `enum class`es) their headers; every public name of the bound enums is bound.
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
  Casting runs the game's `/cast` with the gem counted from 1; empty gems, gems past the 12,
  spells not in the book or above your level (and sets holding one) aren't passed on, and gems
  left alone are -1 in the set memorized. Stacks are split and items selected at the place as the
  game numbers it (a merchant's item at its own slot, not its list row); only the merchant's
  selected item is bought and only yours sold, and nothing is done with the window closed.
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
