//! [`Mq`]: what a plugin uses to talk to MacroQuest and read the game.

use std::ffi::c_char;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;
use std::time::Duration;

use mq_emu_sys::constants::{MAX_STRING, USERCOLOR_DEFAULT};

use crate::game::{GameState, Spawn};
use crate::host::{Host, truncate};
use crate::plugin::Plugin;
use crate::runtime::Runtime;
use crate::stats::Stats;
use crate::text::from_c_array;

/// Access to MacroQuest from inside a plugin callback.
///
/// Every [`Plugin`] callback receives `&Mq<Self>`. It can't be stored or sent
/// to another thread (it's `!Send`), which is what keeps all game access on
/// the game thread; values borrowed from it, like [`Spawn`], can't outlive the
/// callback either.
pub struct Mq<P: Plugin> {
    runtime: &'static Runtime<P>,
    _not_send: PhantomData<*const ()>,
}

/// Why a command couldn't be registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    /// Names must start with `/`, have at least one more character, and contain no whitespace.
    InvalidName,
    /// This plugin already registered a command with that name.
    AlreadyRegistered,
    /// MacroQuest refused it, usually because another plugin owns the name.
    Rejected,
    /// MacroQuest's command interface isn't available.
    Unavailable,
}

impl fmt::Display for CommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CommandError::InvalidName => "command names must look like /name",
            CommandError::AlreadyRegistered => "this plugin already registered that command",
            CommandError::Rejected => "MacroQuest rejected the command (is the name taken?)",
            CommandError::Unavailable => "MacroQuest's command interface is unavailable",
        })
    }
}

impl std::error::Error for CommandError {}

/// Why an alias couldn't be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasError {
    /// Names must start with `/`, have at least one more character, and contain
    /// no whitespace or `=` (it would break the alias's line in MacroQuest.ini).
    InvalidName,
    /// The replacement must be non-empty, a single line without NULs, and
    /// shorter than MacroQuest's `MAX_STRING` (2048 bytes).
    InvalidReplacement,
    /// MacroQuest refused it: an alias with that name already exists, or a
    /// command does (aliases may only shadow EverQuest's own commands).
    Rejected,
    /// MacroQuest's command interface isn't available.
    Unavailable,
}

impl fmt::Display for AliasError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            AliasError::InvalidName => "alias names must look like /name",
            AliasError::InvalidReplacement => "alias replacements must be one non-empty line under 2048 bytes",
            AliasError::Rejected => "MacroQuest rejected the alias (is the name taken?)",
            AliasError::Unavailable => "MacroQuest's command interface is unavailable",
        })
    }
}

impl std::error::Error for AliasError {}

impl<P: Plugin> Mq<P> {
    pub(crate) fn new(runtime: &'static Runtime<P>) -> Mq<P> {
        Mq { runtime, _not_send: PhantomData }
    }

    pub(crate) fn host(&self) -> &Host {
        self.runtime.host()
    }

    pub(crate) fn runtime(&self) -> &'static Runtime<P> {
        self.runtime
    }

    /// The plugin's name (its crate name), as used in its chat messages.
    pub fn plugin_name(&self) -> &'static str {
        self.runtime.name()
    }

    // ----- chat -----

    /// Writes a line to the MacroQuest chat window. Color it with the codes in
    /// [`crate::color`]; a literal `\ag` prints as text here, unlike in macros.
    pub fn chat(&self, text: impl AsRef<str>) {
        (self.runtime.host().write_chat)(text.as_ref(), USERCOLOR_DEFAULT);
    }

    /// Writes a line in a specific chat color (`USERCOLOR_*`/`CONCOLOR_*` in `mq_emu::sys::constants`).
    pub fn chat_color(&self, text: impl AsRef<str>, color: i32) {
        (self.runtime.host().write_chat)(text.as_ref(), color);
    }

    // ----- commands -----

    /// Runs `line` as if typed, on the next pulse. This is what plugins should
    /// normally use: the command can't call back into the plugin mid-callback.
    pub fn command(&self, line: impl AsRef<str>) -> bool {
        self.runtime.do_command(line.as_ref(), true)
    }

    /// Runs `line` immediately. If it triggers events for this plugin (chat
    /// output, its own commands), they're delivered after the current callback.
    pub fn command_now(&self, line: impl AsRef<str>) -> bool {
        self.runtime.do_command(line.as_ref(), false)
    }

    /// Runs `line` after `delay`.
    pub fn timed_command(&self, line: impl AsRef<str>, delay: Duration) -> bool {
        self.runtime.timed_command(line.as_ref(), delay)
    }

    /// Registers a slash command such as `/mycmd`. The handler gets the plugin,
    /// this `Mq`, and the text after the command name. Commands are removed
    /// automatically when the plugin unloads.
    pub fn add_command<F>(&self, name: &str, handler: F) -> Result<(), CommandError>
    where
        F: Fn(&mut P, &Mq<P>, &str) + 'static,
    {
        self.runtime.add_command(name, Rc::new(handler))
    }

    /// Removes a command this plugin registered. False if it hadn't.
    pub fn remove_command(&self, name: &str) -> bool {
        self.runtime.remove_command(name)
    }

    /// True if any plugin (or MacroQuest) has a command with this name.
    pub fn is_command(&self, name: &str) -> bool {
        self.runtime.is_command(name)
    }

    // ----- aliases -----

    /// Adds an alias, as `/alias /name replacement` does: typing `/name args`
    /// runs `replacement args`. Unlike `/alias`, it isn't saved to
    /// MacroQuest.ini, and it's removed when the plugin unloads (MacroQuest
    /// itself would leave it behind until the game restarts).
    pub fn add_alias(&self, name: &str, replacement: &str) -> Result<(), AliasError> {
        self.runtime.add_alias(name, replacement, false)
    }

    /// Adds an alias and saves it to MacroQuest.ini, exactly like
    /// `/alias /name replacement`. It belongs to the user from then on: it
    /// stays after the plugin unloads and comes back when MacroQuest starts.
    pub fn add_persistent_alias(&self, name: &str, replacement: &str) -> Result<(), AliasError> {
        self.runtime.add_alias(name, replacement, true)
    }

    /// Removes an alias, like `/alias /name delete`: any alias, whoever added
    /// it, including MacroQuest's built-in ones, and it's deleted from
    /// MacroQuest.ini too. False if there was no such alias.
    pub fn remove_alias(&self, name: &str) -> bool {
        self.runtime.remove_alias(name)
    }

    /// True if an alias with this name exists (as `${Alias[/name]}`).
    pub fn is_alias(&self, name: &str) -> bool {
        self.runtime.is_alias(name)
    }

    // ----- macro data -----

    /// Expands MacroQuest data, e.g. `mq.parse("${Me.Level}")`, the same way
    /// macros and `/echo` do. Unknown data expands to `NULL`, as in MacroQuest.
    /// Results are limited to MacroQuest's `MAX_STRING` (2048 bytes).
    pub fn parse(&self, text: &str) -> String {
        let mut buffer = vec![0 as c_char; MAX_STRING];
        for (dst, src) in buffer.iter_mut().zip(truncate(text, MAX_STRING - 1).bytes()) {
            *dst = src as c_char;
        }
        // The return value only says whether anything was substituted.
        unsafe { mq_emu_sys::mq2main::ParseMacroData(buffer.as_mut_ptr(), buffer.len()) };
        from_c_array(&buffer)
    }

    // ----- game state -----

    pub fn game_state(&self) -> GameState {
        GameState::from((self.runtime.host().game_state)())
    }

    /// Your character's spawn, if in game.
    pub fn me(&self) -> Option<Spawn<'_>> {
        // SAFETY: the game keeps the spawn alive at least until this callback
        // returns, which the borrow of `self` guarantees.
        unsafe { Spawn::from_raw((self.host().local_player)()) }
    }

    /// Your current target, if any.
    pub fn target(&self) -> Option<Spawn<'_>> {
        unsafe { Spawn::from_raw((self.host().target)()) }
    }

    // ----- diagnostics -----

    /// Counters the runtime keeps for this plugin (pulse timing, re-entrant
    /// events, panics). Collected silently; nothing is printed unless asked.
    pub fn stats(&self) -> Stats {
        self.runtime.stats()
    }

    pub fn reset_stats(&self) {
        self.runtime.reset_stats()
    }

    /// Forgets problems not yet reported in a warning, e.g. right after
    /// causing them on purpose. [`Mq::stats`] still counts them, and later
    /// problems are warned about as usual.
    pub fn dismiss_pending_warnings(&self) {
        self.runtime.dismiss_pending_warnings()
    }

    /// Pulses slower than this produce a rate-limited chat warning (at most
    /// one every 30 seconds). `None` disables it. Default: 16 ms.
    pub fn set_slow_pulse_warning(&self, threshold: Option<Duration>) {
        self.runtime.set_slow_pulse_warning(threshold)
    }
}
