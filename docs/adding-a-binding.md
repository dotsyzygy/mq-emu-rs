# Adding a binding

Where a new MacroQuest capability goes, in order. Look at a recent commit for the same kind of
binding (`git show --stat <commit>`, then only the files you need) rather than reading whole files.

## 1. Find it in MacroQuest

The MacroQuest source is at `$MQ_ROOT`. Grep for the function or type, then read only the lines
around it. `src/main/datatypes/MQ2*Type.cpp` shows how MacroQuest itself reads a value for `${...}`,
which is usually the behaviour to match.

## 2. mq-emu-sys (raw FFI)

- **An exported function or global:** declare it in `src/mq2main.rs` or `src/eqlib.rs`.
  `tests/exports.rs` and `tests/link.rs` check it exists and links.
- **A game-object field:** add it to `layouts.txt`, and regenerate `src/layouts/` with
  `tools/mq-emu-bindgen` (fields come from `eqlib.pdb`). `tests/layouts.rs` checks them.
- **Anything needing C++** (inline functions, classes, templates, catching exceptions): add an
  `extern "C"` function to a `shim/*.cpp` file. Declare it in a `function_table!` (`src/macros.rs`) in
  the matching `src/*.rs`, and add a new `.cpp` to `build.rs`. Shim functions never let exceptions
  out (`guarded`), take strings as pointer and length, and return strings through a sink callback.
- **Constants and enums:** mirror them in the area's `src/*.rs`. `tests/header_constants.rs` or
  `tests/imgui_constants.rs` checks them against the headers.

## 3. mq-emu (safe API)

- Put the safe wrapper in the area's module in `crates/mq-emu/src/`. ImGui goes under `src/imgui/`
  (never read `src/imgui.rs` whole), and gets an export added in `imgui.rs`.
- Closures called from C++ go through `imgui/plot.rs`'s `Callback`, which catches a panic and resumes
  it after the C++ call returns.
- MacroQuest bugs are worked around here or in the shim, never patched upstream. Each one gets a
  test showing MacroQuest's own behaviour failing (a permanent negative test) next to the fixed one.

## 4. Tests without the game

- Game data: `crates/mq-emu/src/runtime/tests/*.rs` against the fake game.
- ImGui: `crates/mq-emu/src/imgui/tests/*.rs`, driving the real `imgui.dll`; MacroQuest-only
  functions are faked in `imgui/fake.rs`.
- The shim directly: `crates/mq-emu-sys/tests/*.rs`.
- Run `cargo test --workspace` and `cargo clippy --workspace --all-targets`.

## 5. In game

- Add checks to the test plugin, `plugins/MQ2RustPluginTest/src/*_checks.rs`, using
  `report.print(mq, label, Check::...)` in a `report.section` of `lib.rs`. Checks that need you in
  game return `Check::Skip` otherwise.
- For something to look at or try by hand, add it to the debug window (`window.rs`).
- Deploy with `cargo mq deploy`. Then in game: `/plugin MQ2RustPluginTest` and
  `/rusttest selftest`.

## 6. Docs and commit

- Update `docs/status.md`: Supported (one short line per area), Found on emu (MacroQuest bugs worked
  around) and Missing.
- If a test file was added, describe it in `docs/tests.md`.
- After the selftest passes, commit on main and push.
