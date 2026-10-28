# Tests


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
