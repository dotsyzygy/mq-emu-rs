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
- **Commands** as Rust closures that receive your plugin state, and **aliases**: temporary ones
  (`mq.add_alias`, removed when the plugin unloads) or saved to MacroQuest.ini like `/alias`.
- **Settings in INI files**: `mq.ini()` is the plugin's own `config\<Name>.ini` (the file a C++
  plugin's `INIFileName` names); `Ini::new(path)` opens any other. Read values as text, parsed or
  as MacroQuest's switches (`on`/`yes`/`1`...), write anything `Display`, list and remove keys and
  sections. Files stay compatible with `${Ini[...]}`, `/ini` and C++ plugins.
- **Chat events**, like a macro's `#Event`: `mq.add_event("tell", "#1# tells you, '#2#'", handler)`
  runs the handler with the captures for every matching line, matched by MacroQuest's own matcher.
- **Key binds**: `mq.add_key_bind("MyBind", handler)` adds a bind that `/bind MyBind ctrl+F1` sets;
  read, set and press any bind, and the game's own bindings, with keys as `KeyCombo`s.
- **Game state**: `mq.me()`, `mq.target()`, positions, headings, HP/mana, and

  `mq.parse("${...}")` for any MacroQuest data.
- **Movement and the spawn list**: hold movement keys, turn smoothly or instantly, face points;
  iterate the zone's spawns, look them up by ID or name, find the nearest match.
- **Your character's things**: `mq.inventory()` (worn slots, bags and their contents, the cursor;
  find items by name or ID, count stacks), `mq.bank()`, `mq.buffs()`/`mq.songs()`, spell gems
  and book, `mq.spell(id)`/`mq.spell_by_name(..)`, `mq.group()`, `mq.pet()`, `mq.zone()` and the
  zone list, and `mq.ground_items()`.
- **The world around you**: `mq.target_buffs()`, `mq.pet_buffs()` and `mq.cached_buffs(&spawn)`,
  `mq.raid()`, `mq.mercenary()`, `mq.xtargets()` and `mq.haters()`, `mq.alt_abilities()`,
  `mq.skills()` and `mq.abilities()`, `mq.merchant()` and `mq.loot_window()`.
- **EverQuest's UI windows**: `mq.window("InventoryWindow/IW_DoneButton")` finds a window or
  control as `${Window[...]}` does; read its state, text, lists, tabs and gauges, and act on it as
  `/notify` does: `button.click(Click::Left)`, `list.select(row)`, `edit.set_text(..)`,
  `window.notify(Some(control), Notification::NewValue, 5)`, and the open `mq.context_menu()`.
  What windows hold: `window.as_item_slot()` and `as_hot_button()`, `mq.hot_bar(0)`,
  `mq.trade_window()`, and an item's slot as `/itemnotify` finds it:
  `mq.item_slot_window(SlotLocation::parse("in pack1 3")?)?.window().click(Click::Left)`.
  With no window showing a place (a closed bag): `mq.pick_up_item(place)`, `mq.drop_item(place)`,
  `mq.use_item(place)` (`/useitem`) and `mq.open_bag(place)`.
- **Windows of your own**: `mq.add_xml_file("MQUI_MyWnd.xml")` and
  `mq.add_window("MyWnd", |plugin, mq, event| ...)`, made whenever the UI loads and deleted for you
  before it's cleaned up and when the plugin unloads. `mq.add_window_with("MyWnd", hooks)` also
  draws on it and takes the mouse.
- **Subclassing the game's windows**: `mq.subclass_window("InventoryWindow", hooks)` with
  `WindowHooks::new().on_notification(..).on_mouse(..).on_draw(..)`: see what a window gets first
  (returning `ControlFlow::Break(())` keeps it from the window) and draw on it with a `Painter`
  (rectangles, EQ text, UI textures). On whenever the game has the window, off for you before the
  UI is cleaned up and when the plugin unloads.
- **Messaging between characters** through MacroQuest's actor system (via the launcher, on this PC
  or others): every plugin has a default mailbox (`Plugin::on_message`), `mq.add_mailbox(name, ...)`
  adds named ones, `mq.send(&Address::character("Bob").mailbox("follow"), bytes)` sends, and
  `task.request(&address, bytes).await` waits for a reply (`mq.reply` answers).
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
  - Spawns, items, spells and other game objects can't outlive the callback they came from.
  - Panics are caught at the boundary.
  - Callbacks that re-enter your plugin while it's busy are queued and delivered right after,
    with a cap against feedback loops.
- **Diagnostics**: `mq.stats()` (pulse timing, queued/dropped events, panics) is collected
  silently, with rate-limited chat warnings only when something is wrong.

See `crates/mq-emu-sys/README.md` for build requirements (`MQ_ROOT`, 32-bit target, static C
runtime).
