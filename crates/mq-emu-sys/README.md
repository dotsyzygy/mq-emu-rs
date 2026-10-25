# mq-emu-sys

Unofficial raw, `unsafe` FFI bindings to **MacroQuest emu** (`MQ2Main.dll` and `eqlib.dll`).

> **Unofficial.** Part of mq-rs, an independent project not affiliated with,
> endorsed by, or supported by the MacroQuest team.

> **Emulator only.** Targets the MacroQuest `emu` branch for the RoF2 client
> (`EverQuestVersion` = `May 10 2013 23:30:08`), which is 32-bit. Not intended
> for live EverQuest; building for anything other than `i686-pc-windows-msvc`
> is a compile error.

Most plugins should use the safe wrapper crate, `mq-emu`, instead of this one.

## Requirements

- A **built** MacroQuest emu checkout (`Release|Win32`), so that
  `<MQ_ROOT>/build/bin/release` contains `MQ2Main.lib` and `eqlib.lib`.
- `MQ_ROOT` set to that checkout, either in your environment or in
  `~/.cargo/config.toml`:

  ```toml
  [env]
  MQ_ROOT = "C:/path/to/macroquest"
  ```

  `MQ_BUILD_CONFIG` picks `build/bin/<config>` and defaults to `release`.
- The 32-bit target and a static C runtime, set in your project's `.cargo/config.toml`:

  ```toml
  [build]
  target = "i686-pc-windows-msvc"

  # Link the C runtime statically, as MacroQuest does, so plugins don't need
  # the Visual C++ redistributable on players' machines.
  [target.i686-pc-windows-msvc]
  rustflags = ["-C", "target-feature=+crt-static"]
  ```

  (`rustup target add i686-pc-windows-msvc` if it isn't installed. To accept a
  dependency on `VCRUNTIME140.dll` instead, set `MQ_EMU_DYNAMIC_CRT=1`.)
- MacroQuest's vcpkg dependencies installed (they are after building MacroQuest):
  a small C++ shim is compiled against MacroQuest's headers, because commands
  are only reachable through its C++ `MainInterface`.

If any of these is missing, the build fails with a message saying what to set.
