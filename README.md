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

What `mq-emu` supports, what it works around and what's missing: [docs/status.md](docs/status.md).

## Crates

| Crate                       | Published | Purpose                                                                                  |
| --------------------------- | --------- | ---------------------------------------------------------------------------------------- |
| `crates/mq-emu-sys`         | yes       | Raw, `unsafe` 1:1 FFI declarations for `MQ2Main.dll` / `eqlib.dll` exports               |
| `crates/mq-emu`             | yes       | Safe, idiomatic API on top of `mq-emu-sys`; what plugins depend on                       |
| `plugins/MQ2RustPluginTest` | no        | Test plugin exercising the bindings in-game (builds `MQ2RustPluginTest.dll`)             |
| `tools/mq-emu-bindgen`      | no        | Generates `mq-emu-sys`'s game-object layouts from `eqlib.pdb`                            |
| `tools/lua-proto-oracle`    | no        | Test-only: MQ2Lua's message format via real C++ protobuf, to check `mq-emu::lua` against |
| `tools/cargo-mq`            | no        | `cargo mq deploy` and `cargo mq doctor`, for building and deploying plugins              |

To write your own plugin in its own repo, see
[Using it in your own plugin](crates/mq-emu/README.md#using-it-in-your-own-plugin).

## Trying it in game

```powershell
cargo install --path tools/cargo-mq   # once, and again when the tool changes
cargo mq deploy
```

This builds `MQ2RustPluginTest` in release and copies the DLL (and its `.pdb`) into the `plugins`
folder of `MQ_INSTALL`, plus the plugin's Lua scripts (`plugins/<name>/lua/*.lua`) into `lua` and its
resources (`plugins/<name>/resources`, such as UI XML in `uifiles/default`) into `resources`. Without
`MQ_INSTALL` it uses `$MQ_ROOT/build/bin/release`. Both are found the way the build finds `MQ_ROOT`:
from your environment, or from `[env]` in `~/.cargo/config.toml`. Use `-p <package>` to choose a
plugin, `--debug` for a debug build, and `--no-build` to only copy. If the plugin is
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
config and only explicitly passed settings, to check what a downstream user will see;
`crates/mq-emu/tests/consumer.rs` does the same for a whole plugin.

## Tests

`cargo test` checks the bindings against the MacroQuest build without running EverQuest; what only
the game can show is covered by `/rusttest selftest`. What each test file checks: [docs/tests.md](docs/tests.md).

## License

mq-emu-rs is licensed under the [MIT License](LICENSE).

MacroQuest itself is GPL-2.0. Every plugin built with these crates links against MQ2Main.dll, so a
distributed plugin is subject to MacroQuest's license regardless of the license on these crates.
