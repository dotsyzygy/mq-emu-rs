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
- Parsing `${...}` text
- Reading TLOs and members (`tlo`, `tlo_at`, `data("Target.Name")`) as typed values
- Providing TLOs from Rust (`add_tlo`) returning scalar values
- Spawns: you, your target, the spawn list, lookup by ID or name, nearest with a filter; name, level,
  type, position, heading, HP, mana, distance
- Movement: movement keys, facing a heading or point (smooth or instant), stopping
- Async tasks on the game thread: `delay`, `wait_until`, timeouts, cancellation
- Actors messaging: mailboxes, send, reply, request with timeout, other characters and peers
- Lua interop: exchanging every actors value type with Lua scripts
- ImGui (`mq_emu::imgui`): windows, child regions, menus, tabs, tables, trees, popups, tooltips,
  text, buttons, checkboxes, radio buttons, selectables, sliders, drags, number and text inputs,
  combo boxes, color editing, progress bars, plots, ID/style/disabled scopes, and lines, shapes and
  text drawn on a window (`Ui::draw_list`). Every begin/end
  pair is a closure scope that always ends, even on a panic, and ImGui errors are caught and
  reported instead of pausing every plugin's overlay
- Runtime stats and safety nets: caught panics, slow-pulse warnings, queue limits
- Generated raw layouts for spawns, your character, the spawn list, zone info, key state, ground items
  and client state (via `sys`)

**Missing**

- Safe wrappers for items, inventory, spells, buffs, groups, pets, zone info and ground items (reachable
  only as raw layouts or through TLO reads)
- Custom datatypes: TLOs from Rust can't return objects with members (`Value::Object` gives NULL)
- ImGui beyond the bound set: textures and images, the rest of draw lists (paths, polygons,
  foreground/background lists), drag and drop,
  fonts, docking control, ImPlot (reachable as raw `ImGui::*` C++ only)
- MacroQuest events (`#Event`-style chat matching) and key binds
- EQ UI windows (`CXWnd` and friends): finding, reading and clicking them
- Settings helpers for reading and writing plugin INI files
- Hooks/detours on game functions
- Live EverQuest and 64-bit builds (out of scope: emu/RoF2 only)

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
into `$MQ_ROOT/build/bin/release/lua`, finding `MQ_ROOT` the same way the build does: from your
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
| `/rusttest window` | Toggles the debug window: selftest results, runtime stats, the game as mq-emu sees it, and every bound ImGui widget to try by hand |

The selftest compares mq-emu's view of the game with MacroQuest's `${Me}`/`${Target}` data, and
exercises commands, aliases, re-entrancy, parsing, keys and async tasks. It also:

- **Touches MacroQuest.ini:** saves a `/rusttestsaved` alias to it and deletes it again.
- **Moves you:** turns 100° and back, then walks forward and back for half a second each.
- **Triggers the runtime's safety nets on purpose:** expect a caught-panic message, a slow-pulse
  warning, and a burst of `flood` lines that the runtime cuts off.
- **Opens the debug window**, to check it draws without ImGui errors, and leaves it open with the
  results.
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
- **`tests/header_constants.rs`**: mirrored constants and `EVERQUEST_VERSION` match the MQ headers.
- **`tests/link.rs`**: compiles single-symbol probe DLLs against the import libraries.
- **`examples/link_check.rs`**: a `cdylib` that references every import, so each one must resolve at link time.
- **`tests/layouts.rs`**: every generated field offset and record size matches eqlib's header offset
  comments and `*_size` constants, an independent source (about 980 fields are covered).
- **`tests/accessors.rs`**: generated accessors read and write the right bytes, including bitfields,
  C++ `bool`, nested records and arrays of records.
- **`tools/mq-emu-bindgen/tests`**: the committed layouts are up to date with `layouts.txt` and the PDB,
  and generation is deterministic.
- **`tests/plugin_loader.rs`**: the plugin exports we document match every name MacroQuest's loader
  looks up, in both directions.
- **`tests/shim.rs`**: the C++ shim against a fake `MainInterface`: arguments, handles and callback
  routing, without MacroQuest.
- **`tests/imgui.rs`**: the ImGui shim against MacroQuest's real `imgui.dll`, in a headless
  context: every wrapper draws, simulated clicks register, and ImGui's C++ exceptions (calling `End`
  too often) are caught instead of crossing into Rust. A window left open makes the frame fail, as
  it would pause ImGui in game.
- **`tests/imgui_constants.rs`**: every ImGui flag and enum value matches MacroQuest's `imgui.h`, and
  every public name of the bound enums is bound.
- **`tests/consumer.rs`**: builds mq-emu-sys as an outside crate would, checking each requirement
  (`MQ_ROOT`, 32-bit target, static C runtime, docs.rs) gives a clear error when missing.
- **`mq-emu` runtime tests**: dispatch, re-entrant events queued and delivered in order, the feedback
  loop cap, caught panics reported once, commands routed to closures, rate-limited warnings, all
  against a fake host and clock.
- **`mq-emu` ImGui tests**: against the real `imgui.dll`, every scope ends when its closure panics,
  closed and collapsed windows skip their contents, clicks and typing (multi-byte, growing the
  buffer) reach Rust values, style variables have the right kinds, and table setup misuse panics
  before ImGui sees it. Doctests show that reaching a table from a window opened in one of its cells
  doesn't compile.
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
