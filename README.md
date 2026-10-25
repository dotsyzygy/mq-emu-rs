# mq-rs

Unofficial Rust bindings for writing [MacroQuest](https://github.com/macroquest/macroquest) plugins.

> **Unofficial.** mq-rs is an independent project. It is not affiliated with,
> endorsed by, or supported by the MacroQuest team; please don't take mq-rs
> issues to them.

> **Emulator only.** mq-rs targets the MacroQuest `emu` branch (RoF2 client,
> `EverQuestVersion` = `May 10 2013 23:30:08`), which is 32-bit. It is not
> intended for live EverQuest, and the crates refuse to build for anything
> other than `i686-pc-windows-msvc`.

## Crates

| Crate | Published | Purpose |
|---|---|---|
| `crates/mq-emu-sys` | yes | Raw, `unsafe` 1:1 FFI declarations for `MQ2Main.dll` / `eqlib.dll` exports |
| `crates/mq-emu` | yes | Safe, idiomatic API on top of `mq-emu-sys`; what plugins depend on |
| `plugins/MQ2RustPluginTest` | no | Test plugin exercising the bindings in-game (builds `MQ2RustPluginTest.dll`) |
| `tools/mq-emu-bindgen` | no | Generates `mq-emu-sys`'s game-object layouts from `eqlib.pdb` |

## Trying it in game

```powershell
./scripts/deploy-plugin.ps1        # or, from bash: scripts/deploy-plugin.sh
```

This builds `MQ2RustPluginTest` in release and copies the DLL (and its `.pdb`) into
`$MQ_ROOT/build/bin/release/plugins`, finding `MQ_ROOT` the same way the build does: from your
environment, or from `[env]` in `~/.cargo/config.toml`. Use `-Plugin <name>` for another plugin
under `plugins/`, `-DebugBuild` for a debug build, and `-NoBuild` to only copy (from bash:
`--plugin <name>`, `--debug`, `--no-build`). If the plugin is
loaded in game, `/plugin MQ2RustPluginTest unload` first so the DLL can be replaced. Then in game:

| Command | What it does |
|---|---|
| `/plugin MQ2RustPluginTest` | Load it (`/plugin MQ2RustPluginTest unload` to unload) |
| `/rusttest selftest` | Compares mq-emu's view of the game with MacroQuest's `${Me}`/`${Target}` data and exercises commands, re-entrancy and parsing; prints PASS/FAIL per check |
| `/rusttest stats` | Pulse timing, queued/dropped events, panics |
| `/rusttest me`, `/rusttest target` | What mq-emu reads for you and your target |
| `/rusttest spawns [n]`, `find <name>` | The zone's spawn count and your nearest `n` spawns; look a spawn up by its shown name |
| `/rusttest ping <character>`, `tell <character> <text>` | Messaging between characters running the plugin: time a round trip, or show a line on the other character |
| `/rusttest face`, `turn [deg]`, `walk [ms]`, `stop` | Movement: smoothly face your target, turn left by some degrees (negative for right), walk forward for a while, or stop everything |
| `/rusttest panic`, `slow [ms]`, `flood` | Deliberately trigger the runtime's safety nets: a caught panic, a slow-pulse warning, a capped feedback loop |

Target something before `selftest` to include the target checks.

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
- **`tests/consumer.rs`**: builds mq-emu-sys as an outside crate would, checking each requirement
  (`MQ_ROOT`, 32-bit target, static C runtime, docs.rs) gives a clear error when missing.
- **`mq-emu` runtime tests**: dispatch, re-entrant events queued and delivered in order, the feedback
  loop cap, caught panics reported once, commands routed to closures, rate-limited warnings, all
  against a fake host and clock.
- **`mq-emu` `tests/plugin_exports.rs`**: a DLL built with `plugin!` exports everything the loader
  needs, carries the right `EverQuestVersion` bytes, and doesn't need the Visual C++ redistributable.
- **Unit tests**: type layouts (`ForeignPointer`, callback pointers), the `imports!` macro, the version
  byte array, the generator's naming and enum literal rules, text conversion, headings and stats.

Test binaries never import MacroQuest's DLLs (checked by the tests themselves), so `cargo test` runs
without the game. What only the game can show is covered by `/rusttest selftest`.

Each check also has negative tests showing it rejects bad input: names that aren't exported, wrong
constant values, and imports that don't link (a typo, or a symbol from the wrong DLL).

## License

mq-emu-rs is licensed under the [MIT License](LICENSE).

MacroQuest itself is GPL-2.0. Every plugin built with these crates links against MQ2Main.dll, so a
distributed plugin is subject to MacroQuest's license regardless of the license on these crates.
