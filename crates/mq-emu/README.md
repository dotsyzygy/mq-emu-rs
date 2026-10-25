# mq-emu

Unofficial, safe Rust API for writing **MacroQuest emu** plugins.

> **Unofficial.** Part of mq-rs, an independent project not affiliated with,
> endorsed by, or supported by the MacroQuest team.

> **Emulator only.** Targets the MacroQuest `emu` branch for the RoF2 client,
> which is 32-bit. Not intended for live EverQuest.

```rust
use mq_emu::{Mq, Plugin};

struct Hello {
    pulses: u64,
}

impl Plugin for Hello {
    fn new(mq: &Mq<Self>) -> Self {
        mq.add_command("/hello", |plugin: &mut Hello, mq, args| {
            mq.chat(format!("hello {args}! {} pulses so far", plugin.pulses));
        })
        .expect("register /hello");
        Hello { pulses: 0 }
    }

    fn on_pulse(&mut self, _mq: &Mq<Self>) {
        self.pulses += 1;
    }
}

mq_emu::plugin!(Hello);
```

Build it as a `cdylib` whose library name is the DLL name (e.g. `[lib] name = "MQ2Hello"`),
copy the DLL into MacroQuest's `plugins` folder and `/plugin MQ2Hello`.

## What you get

- **Callbacks** for everything MacroQuest offers plugins (pulse, chat, zoning, spawns,
  macros, plugin load/unload), each with an `&Mq` handle.
- **Commands** as Rust closures that receive your plugin state.
- **Game state**: `mq.me()`, `mq.target()`, positions, headings, HP/mana, and
  `mq.parse("${...}")` for any MacroQuest data.
- **Movement and the spawn list**: hold movement keys, turn smoothly or instantly, face points;
  iterate the zone's spawns, look them up by ID or name, find the nearest match.
- **Async tasks** for logic that spans many pulses, without a state machine:

  ```rust
  mq.spawn_fallible_task(|task: Task<Hello>| async move {
      task.with(|mq| mq.face_point(&destination))?;
      task.wait_until(|mq| !mq.is_turning()).await;
      task.with(|mq| mq.set_key(MoveKey::Forward, true))?;
      task.delay(Duration::from_secs(2)).await;
      task.with(|mq| mq.stop_moving())
  });
  ```

  Tasks run once per pulse on the game thread (no threads, no async runtime) and reach the game
  only inside `task.with(...)`, so nothing from the game can be held across an `.await`.
- **Safety**:
  - Game access stays on the game thread, because `Mq` is `!Send`.
  - Spawns can't outlive the callback they came from.
  - Panics are caught at the boundary.
  - Callbacks that re-enter your plugin while it's busy are queued and delivered right after,
    with a cap against feedback loops.
- **Diagnostics**: `mq.stats()` (pulse timing, queued/dropped events, panics) is collected
  silently, with rate-limited chat warnings only when something is wrong.

See `crates/mq-emu-sys/README.md` for build requirements (`MQ_ROOT`, 32-bit target, static C
runtime).
