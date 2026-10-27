//! Chat events: MacroQuest's `#Event`-style matching of chat lines (Blech,
//! as macros and Lua use it), run on the chat this plugin receives.

use std::ffi::{c_char, c_void};
use std::fmt;
use std::ptr::NonNull;
use std::rc::Rc;

use mq_emu_sys::events::{self as sys, mqemu_event_status};

use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::text::{from_c, to_c};

/// Where a matched chat line came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatSource {
    /// The game's chat ([`Plugin::on_incoming_chat`]): tells, says, combat...
    Game,
    /// MacroQuest's chat window ([`Plugin::on_write_chat_color`]): `/echo`,
    /// plugins and macros writing to it.
    MacroQuest,
}

/// A chat line that matched an event's pattern, and what its `#variables#`
/// captured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventMatch {
    line: String,
    values: Vec<(String, String)>,
    source: ChatSource,
}

impl EventMatch {
    pub(crate) fn new(line: String, values: Vec<(String, String)>, source: ChatSource) -> EventMatch {
        EventMatch { line, values, source }
    }

    /// The line as matched: without MacroQuest's color codes, and with item
    /// links reduced to their text unless the event keeps them.
    pub fn line(&self) -> &str {
        &self.line
    }

    /// What `#n#` captured (`#1#`, `#2#`...), as `Param<n>` gets it in a
    /// macro's event sub. None if the pattern has no `#n#`.
    pub fn arg(&self, n: usize) -> Option<&str> {
        self.get(&n.to_string())
    }

    /// What `#name#` captured; the first, if the name is used more than once.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.iter().find(|(n, _)| n == name).map(|(_, value)| value.as_str())
    }

    /// Every `#variable#`'s name and capture, in the pattern's order,
    /// `#*#`s included.
    pub fn values(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values.iter().map(|(name, value)| (name.as_str(), value.as_str()))
    }

    pub fn source(&self) -> ChatSource {
        self.source
    }
}

/// Why an event couldn't be added.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventError {
    /// Event names must not be empty.
    InvalidName,
    /// This plugin already has an event with that name.
    AlreadyAdded,
    /// The pattern has no text between its `#` and `|` delimiters (it's
    /// empty, or only delimiters): MacroQuest's matcher would crash on it.
    NoText,
    /// The pattern has too many parts (text and variables), or makes another
    /// of the plugin's patterns have too many, by sharing its beginning: the
    /// matcher would overrun its fixed stack (see [`MAX_DEPTH`](crate::sys::events::MAX_DEPTH)).
    TooDeep,
    /// More than 7 `|print variables|`: their values could overflow the
    /// matcher's buffer, which ends the game.
    TooManyPrintVariables,
    /// Patterns must be shorter than `MAX_STRING` (2048 bytes).
    TooLong,
    /// Out of memory.
    Failed,
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            EventError::InvalidName => "event names must not be empty",
            EventError::AlreadyAdded => "this plugin already has an event with that name",
            EventError::NoText => "the pattern has no text, only delimiters",
            EventError::TooDeep => "the pattern has too many parts for MacroQuest's matcher",
            EventError::TooManyPrintVariables => "the pattern has more than 7 |print variables|",
            EventError::TooLong => "patterns must be shorter than 2048 bytes",
            EventError::Failed => "out of memory",
        })
    }
}

impl std::error::Error for EventError {}

/// One of the shim's matchers, freed when dropped.
pub(crate) struct Matcher(NonNull<sys::mqemu_matcher>);

impl Matcher {
    pub(crate) fn new(print_var: sys::mqemu_print_var_fn) -> Option<Matcher> {
        NonNull::new(unsafe { sys::mqemu_matcher_new(Some(print_var)) }).map(Matcher)
    }

    pub(crate) fn add(&self, pattern: &str, key: u64) -> Result<(), EventError> {
        let pattern = to_c(pattern);
        match unsafe { sys::mqemu_matcher_add(self.0.as_ptr(), pattern.as_ptr(), key) } {
            mqemu_event_status::OK => Ok(()),
            mqemu_event_status::NO_TEXT => Err(EventError::NoText),
            mqemu_event_status::TOO_DEEP => Err(EventError::TooDeep),
            mqemu_event_status::TOO_MANY_PRINT_VARS => Err(EventError::TooManyPrintVariables),
            mqemu_event_status::TOO_LONG => Err(EventError::TooLong),
            _ => Err(EventError::Failed),
        }
    }

    pub(crate) fn remove(&self, key: u64) -> bool {
        unsafe { sys::mqemu_matcher_remove(self.0.as_ptr(), key) }
    }

    pub(crate) fn is_empty(&self) -> bool {
        unsafe { sys::mqemu_matcher_is_empty(self.0.as_ptr()) }
    }

    /// The events `line` matches, by key, with their variables.
    pub(crate) fn feed(&self, line: &str) -> Vec<(u64, Vec<(String, String)>)> {
        unsafe extern "C" fn record(user: *mut c_void, key: u64, names: *const *const c_char, values: *const *const c_char, count: usize) {
            let matches = unsafe { &mut *user.cast::<Vec<(u64, Vec<(String, String)>)>>() };
            let pairs = (0..count)
                .map(|i| unsafe { (from_c(*names.add(i)).into_owned(), from_c(*values.add(i)).into_owned()) })
                .collect();
            matches.push((key, pairs));
        }
        let line = to_c(line);
        let mut matches = Vec::new();
        unsafe { sys::mqemu_matcher_feed(self.0.as_ptr(), line.as_ptr(), record, (&raw mut matches).cast()) };
        matches
    }
}

impl Drop for Matcher {
    fn drop(&mut self) {
        unsafe { sys::mqemu_matcher_free(self.0.as_ptr()) }
    }
}

pub(crate) type EventHandler<P> = dyn Fn(&mut P, &Mq<P>, &EventMatch);

pub(crate) struct EventSlot<P: Plugin> {
    pub key: u64,
    pub name: String,
    pub keep_links: bool,
    pub handler: Rc<EventHandler<P>>,
}

/// A plugin's events, in two matchers as MQ2Lua keeps them: lines are
/// cleaned of item links for one and not the other.
pub(crate) struct Events<P: Plugin> {
    pub slots: Vec<EventSlot<P>>,
    pub stripped: Option<Matcher>,
    pub linked: Option<Matcher>,
    pub next_key: u64,
}

impl<P: Plugin> Events<P> {
    pub(crate) const fn new() -> Events<P> {
        Events { slots: Vec::new(), stripped: None, linked: None, next_key: 1 }
    }
}

impl<P: Plugin> Mq<P> {
    /// Adds a chat event, like a macro's `#Event` or Lua's `mq.event`: every
    /// chat line matching `pattern` runs the handler, after the chat
    /// callback that received it ([`Plugin::on_incoming_chat`] or
    /// [`Plugin::on_write_chat_color`]). The plugin doesn't need those
    /// callbacks itself, and there's no `/doevents`: events run as their
    /// lines arrive.
    ///
    /// A pattern matches the whole line, ignoring case and MacroQuest's
    /// color codes, with item links reduced to their text (see
    /// [`Mq::add_event_with_links`]). In it, `#1#`, `#2#`... capture text
    /// ([`EventMatch::arg`]), as does any `#name#` ([`EventMatch::get`]), and
    /// `#*#` matches text without needing it. `|${...}|` is MacroQuest data,
    /// expanded each time a line is matched: `"|${Me.CleanName}| hits #*#"`.
    /// `##` and `||` stand for `#` and `|`.
    ///
    /// Events are removed when the plugin unloads.
    pub fn add_event<F>(&self, name: &str, pattern: &str, handler: F) -> Result<(), EventError>
    where
        F: Fn(&mut P, &Mq<P>, &EventMatch) + 'static,
    {
        self.runtime().add_event(name, pattern, false, Rc::new(handler))
    }

    /// Like [`Mq::add_event`], keeping item links in the line it matches
    /// (Lua's `keepLinks`): they're raw link text, `\x12` and all.
    pub fn add_event_with_links<F>(&self, name: &str, pattern: &str, handler: F) -> Result<(), EventError>
    where
        F: Fn(&mut P, &Mq<P>, &EventMatch) + 'static,
    {
        self.runtime().add_event(name, pattern, true, Rc::new(handler))
    }

    /// Removes one of this plugin's events. False if it had none by that name.
    /// Its matches still waiting to run are dropped.
    pub fn remove_event(&self, name: &str) -> bool {
        self.runtime().remove_event(name)
    }
}
