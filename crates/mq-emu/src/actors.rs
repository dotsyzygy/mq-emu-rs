//! Messaging between MacroQuest instances (your other characters, on this PC
//! or others) through MacroQuest's actor system, the "post office".
//!
//! Every plugin gets a default mailbox named after itself, delivered to
//! [`Plugin::on_message`](crate::Plugin::on_message); [`Mq::add_mailbox`] adds
//! named ones. Mailbox names are per plugin: sending to mailbox `"follow"`
//! reaches `"<Plugin>:follow"` on the other side, so the same plugin on two
//! characters lines up. `<Plugin>` is MacroQuest's name for the plugin: the
//! DLL name without "MQ2" and ".dll" (MQ2Example.dll is "Example"). Use
//! [`Address::absolute`] to reach a mailbox by its full name (e.g. a Lua
//! script's).
//!
//! Messages are bytes; use any encoding you like between your own plugins.
//! MacroQuest delivers them on the game thread during its pulse, and they
//! reach the plugin like any other event (queued if one is already running).

use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::fmt;
use std::ptr::NonNull;
use std::rc::Rc;
use std::time::Duration;

use mq_emu_sys::shim::{self, Dropbox, mqemu_address, mqemu_message};

use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::runtime::{Event, Runtime};
use crate::tasks::Task;

/// Where a message goes (or came from). Build one with [`Address::character`]
/// and refine it:
///
/// ```ignore
/// Address::character("Bob").mailbox("follow")
/// Address::character("Bob").on_server("myserver")
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Address {
    pub character: Option<String>,
    pub server: Option<String>,
    pub account: Option<String>,
    /// Without one, messages go to the plugin's default mailbox.
    pub mailbox: Option<String>,
    /// Use `mailbox` as given, instead of prefixed with your plugin's name.
    pub absolute_mailbox: bool,
    /// A non-EverQuest program registered with the launcher by name.
    pub name: Option<String>,
    pub pid: Option<u32>,
    pub uuid: Option<String>,
    /// Another machine's launcher.
    pub peer: Option<(String, Option<u16>)>,
}

impl Address {
    /// Any character with this name (on any server you're connected to).
    pub fn character(name: impl Into<String>) -> Address {
        Address { character: Some(name.into()), ..Address::default() }
    }

    pub fn on_server(mut self, server: impl Into<String>) -> Address {
        self.server = Some(server.into());
        self
    }

    /// A named mailbox, as registered with [`Mq::add_mailbox`] by the same
    /// plugin on the other side.
    pub fn mailbox(mut self, mailbox: impl Into<String>) -> Address {
        self.mailbox = Some(mailbox.into());
        self
    }

    /// Treat the mailbox name as complete (e.g. another plugin's or a Lua
    /// script's), instead of prefixing your plugin's name.
    pub fn absolute(mut self) -> Address {
        self.absolute_mailbox = true;
        self
    }

    /// Reach a launcher on another machine.
    pub fn via_peer(mut self, ip: impl Into<String>, port: Option<u16>) -> Address {
        self.peer = Some((ip.into(), port));
        self
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        for (label, value) in [
            ("character", &self.character),
            ("server", &self.server),
            ("account", &self.account),
            ("mailbox", &self.mailbox),
            ("name", &self.name),
            ("uuid", &self.uuid),
        ] {
            if let Some(value) = value {
                parts.push(format!("{label}={value}"));
            }
        }
        if let Some(pid) = self.pid {
            parts.push(format!("pid={pid}"));
        }
        if let Some((ip, port)) = &self.peer {
            parts.push(match port {
                Some(port) => format!("peer={ip}:{port}"),
                None => format!("peer={ip}"),
            });
        }
        if parts.is_empty() { f.write_str("(any)") } else { f.write_str(&parts.join(" ")) }
    }
}

/// An `mqemu_address` and the C strings it points into.
struct CAddress {
    raw: mqemu_address,
    _strings: Vec<CString>,
}

fn to_c_address(address: &Address) -> CAddress {
    let mut strings = Vec::new();
    let mut keep = |value: &Option<String>| -> *const c_char {
        match value {
            Some(text) => {
                let c = crate::text::to_c(text);
                let ptr = c.as_ptr();
                strings.push(c);
                ptr
            }
            None => std::ptr::null(),
        }
    };
    let peer_ip = address.peer.as_ref().map(|(ip, _)| ip.clone());
    let raw = mqemu_address {
        uuid: keep(&address.uuid),
        peer_ip: keep(&peer_ip),
        name: keep(&address.name),
        mailbox: keep(&address.mailbox),
        account: keep(&address.account),
        server: keep(&address.server),
        character: keep(&address.character),
        pid: address.pid.unwrap_or(0),
        peer_port: address.peer.as_ref().and_then(|(_, port)| *port).unwrap_or(0),
        has_pid: address.pid.is_some(),
        has_peer_port: address.peer.as_ref().is_some_and(|(_, port)| port.is_some()),
        absolute_mailbox: address.absolute_mailbox,
    };
    CAddress { raw, _strings: strings }
}

/// # Safety
/// The pointers in `raw` must be null or valid C strings.
unsafe fn from_c_address(raw: &mqemu_address) -> Address {
    let text = |ptr: *const c_char| (!ptr.is_null()).then(|| unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned());
    Address {
        character: text(raw.character),
        server: text(raw.server),
        account: text(raw.account),
        mailbox: text(raw.mailbox),
        absolute_mailbox: raw.absolute_mailbox,
        name: text(raw.name),
        pid: raw.has_pid.then_some(raw.pid),
        uuid: text(raw.uuid),
        peer: text(raw.peer_ip).map(|ip| (ip, raw.has_peer_port.then_some(raw.peer_port))),
    }
}

/// Why a messaging action failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorError {
    /// MacroQuest's messaging isn't available.
    Unavailable,
    /// Mailbox names must be non-empty, without whitespace or `:`.
    InvalidName,
    /// This plugin already has a mailbox with that name.
    AlreadyRegistered,
    /// MacroQuest wouldn't register the mailbox or send the message.
    Rejected,
    /// The mailbox that received this message has been removed, so it can't reply.
    MailboxGone,
    /// No reply arrived in time.
    Timeout,
    /// The connection to the launcher closed.
    ConnectionClosed,
    /// Not connected to the launcher (is MacroQuest.exe running?).
    NoConnection,
    /// Nobody at that address.
    RoutingFailed,
    /// A request matched more than one recipient; make the address more specific.
    AmbiguousRecipient,
    /// Another negative status from the post office.
    Status(i32),
}

impl ActorError {
    fn from_status(status: i32) -> ActorError {
        match status {
            -1 => ActorError::ConnectionClosed,
            -2 => ActorError::NoConnection,
            -3 => ActorError::RoutingFailed,
            -4 => ActorError::AmbiguousRecipient,
            other => ActorError::Status(other),
        }
    }
}

impl fmt::Display for ActorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActorError::Unavailable => f.write_str("MacroQuest's messaging is unavailable"),
            ActorError::InvalidName => f.write_str("mailbox names must be non-empty, without spaces or ':'"),
            ActorError::AlreadyRegistered => f.write_str("this plugin already has that mailbox"),
            ActorError::Rejected => f.write_str("MacroQuest rejected the request"),
            ActorError::MailboxGone => f.write_str("the mailbox that received the message is gone"),
            ActorError::Timeout => f.write_str("no reply in time"),
            ActorError::ConnectionClosed => f.write_str("the connection to the launcher closed"),
            ActorError::NoConnection => f.write_str("not connected to the launcher (is MacroQuest.exe running?)"),
            ActorError::RoutingFailed => f.write_str("nobody at that address"),
            ActorError::AmbiguousRecipient => f.write_str("more than one recipient matched"),
            ActorError::Status(status) => write!(f, "post office status {status}"),
        }
    }
}

impl std::error::Error for ActorError {}

/// The dropbox a message arrived through, shared with its mailbox so a reply
/// can tell whether the mailbox still exists.
pub(crate) struct ReplyTo {
    dropbox: Cell<*mut Dropbox>,
}

/// A received message (or a reply to a request). Can be kept and replied to
/// later, from a task for instance.
pub struct Message {
    raw: NonNull<mqemu_message>,
    reply_to: Option<Rc<ReplyTo>>,
}

impl Message {
    /// # Safety
    /// `raw` must be a message from the shim that nothing else will free.
    pub(crate) unsafe fn from_raw(raw: *mut mqemu_message, reply_to: Option<Rc<ReplyTo>>) -> Option<Message> {
        NonNull::new(raw).map(|raw| Message { raw, reply_to })
    }

    /// The payload; empty if the message had none.
    pub fn payload(&self) -> &[u8] {
        let (mut data, mut len) = (std::ptr::null(), 0usize);
        if unsafe { shim::mqemu_message_payload(self.raw.as_ptr(), &mut data, &mut len) } && !data.is_null() {
            unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len) }
        } else {
            &[]
        }
    }

    /// The payload as text (invalid UTF-8 replaced).
    pub fn text(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(self.payload())
    }

    /// Who sent it, if the post office says. Its mailbox is the full name,
    /// so replying with [`Mq::send`] to this address reaches the sender.
    pub fn sender(&self) -> Option<Address> {
        let mut raw = mqemu_address::default();
        unsafe { shim::mqemu_message_sender(self.raw.as_ptr(), &mut raw) }.then(|| unsafe { from_c_address(&raw) })
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        unsafe { shim::mqemu_message_free(self.raw.as_ptr()) }
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message").field("sender", &self.sender()).field("text", &self.text()).finish()
    }
}

pub(crate) type MailboxHandler<P> = dyn Fn(&mut P, &Mq<P>, Message);

/// One registered mailbox. Its `Rc` is shared with the C++ handler, which
/// holds it until MacroQuest destroys the handler.
pub(crate) struct MailboxSlot<P: Plugin> {
    pub(crate) id: u64,
    /// The plugin-relative name ("" for the default mailbox).
    pub(crate) name: String,
    /// None for the default mailbox, which goes to `Plugin::on_message`.
    pub(crate) handler: Option<Rc<MailboxHandler<P>>>,
    pub(crate) reply_to: Rc<ReplyTo>,
    runtime: &'static Runtime<P>,
}

impl<P: Plugin> MailboxSlot<P> {
    pub(crate) fn dropbox(&self) -> *mut Dropbox {
        self.reply_to.dropbox.get()
    }
}

unsafe extern "C" fn mailbox_receive<P: Plugin>(user: *mut c_void, raw: *mut mqemu_message) {
    // SAFETY: `user` is the slot's Rc, alive until mailbox_drop.
    let slot = unsafe { &*user.cast::<MailboxSlot<P>>() };
    let Some(message) = (unsafe { Message::from_raw(raw, Some(slot.reply_to.clone())) }) else { return };
    let (runtime, id) = (slot.runtime, slot.id);
    crate::runtime::guard(|| runtime.deliver_message(id, message));
}

unsafe extern "C" fn mailbox_drop<P: Plugin>(user: *mut c_void) {
    drop(unsafe { Rc::from_raw(user.cast::<MailboxSlot<P>>()) });
}

/// A request's outcome, filled in by the response callback.
#[derive(Default)]
struct Pending {
    result: RefCell<Option<Result<Message, ActorError>>>,
}

unsafe extern "C" fn request_respond(user: *mut c_void, status: c_int, raw: *mut mqemu_message) {
    let pending = unsafe { &*user.cast::<Pending>() };
    let message = unsafe { Message::from_raw(raw, None) };
    let outcome = if status < 0 { Err(ActorError::from_status(status)) } else { message.ok_or(ActorError::Status(status)) };
    let mut result = pending.result.borrow_mut();
    if result.is_none() {
        *result = Some(outcome);
    }
}

unsafe extern "C" fn request_drop(user: *mut c_void) {
    drop(unsafe { Rc::from_raw(user.cast::<Pending>()) });
}

fn valid_mailbox_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(|c: char| c.is_whitespace() || c == ':')
}

impl<P: Plugin> Runtime<P> {
    /// Registers a mailbox; `name` "" is the default one (the plugin's name).
    pub(crate) fn register_mailbox(&'static self, name: &str, handler: Option<Rc<MailboxHandler<P>>>) -> Result<(), ActorError> {
        if self.mailboxes().borrow().iter().any(|slot| slot.name == name) {
            return Err(ActorError::AlreadyRegistered);
        }
        let main = (self.host().main_interface)();
        if main.is_null() {
            return Err(ActorError::Unavailable);
        }
        // MacroQuest prefixes outgoing mailboxes with *its* name for the plugin,
        // so ours must match it exactly or nothing is ever delivered.
        let owner = self.mq_name();
        let local = if name.is_empty() { owner } else { format!("{owner}:{name}") };
        let local = crate::text::to_c(&local);

        let slot = Rc::new(MailboxSlot {
            id: self.next_mailbox_id(),
            name: name.to_owned(),
            handler,
            reply_to: Rc::new(ReplyTo { dropbox: Cell::new(std::ptr::null_mut()) }),
            runtime: self,
        });
        let user = Rc::into_raw(slot.clone()).cast_mut().cast::<c_void>();
        let handle = self.handle();
        // Ownership of `user` passes to C++, which drops it exactly once.
        let dropbox = unsafe {
            shim::mqemu_add_actor(main, &handle, local.as_ptr(), mailbox_receive::<P>, user, Some(mailbox_drop::<P>))
        };
        if dropbox.is_null() {
            return Err(ActorError::Rejected);
        }
        slot.reply_to.dropbox.set(dropbox);
        self.mailboxes().borrow_mut().push(slot);
        Ok(())
    }

    pub(crate) fn unregister_mailbox(&self, name: &str) -> bool {
        let position = self.mailboxes().borrow().iter().position(|slot| slot.name == name);
        let Some(position) = position else { return false };
        let slot = self.mailboxes().borrow_mut().remove(position);
        self.close_mailbox(&slot);
        true
    }

    pub(crate) fn unregister_all_mailboxes(&self) {
        let slots: Vec<_> = self.mailboxes().borrow_mut().drain(..).collect();
        for slot in &slots {
            self.close_mailbox(slot);
        }
    }

    fn close_mailbox(&self, slot: &MailboxSlot<P>) {
        let dropbox = slot.reply_to.dropbox.replace(std::ptr::null_mut());
        let main = (self.host().main_interface)();
        if !main.is_null() && !dropbox.is_null() {
            let handle = self.handle();
            unsafe { shim::mqemu_remove_actor(main, &handle, dropbox) };
        }
    }

    fn deliver_message(&'static self, mailbox: u64, message: Message) {
        self.enter(Event::Message(mailbox, message));
    }

    /// Calls a mailbox's handler (or `on_message` for the default one).
    pub(crate) fn dispatch_message(&self, plugin: &mut P, mq: &Mq<P>, mailbox: u64, message: Message) {
        let slot = self.mailboxes().borrow().iter().find(|slot| slot.id == mailbox).cloned();
        let Some(slot) = slot else { return }; // removed while the message waited
        match &slot.handler {
            Some(handler) => handler(plugin, mq, message),
            None => plugin.on_message(mq, message),
        }
    }

    fn send_raw(&self, dropbox: *mut Dropbox, to: &Address, data: &[u8], pending: Option<Rc<Pending>>) -> Result<(), ActorError> {
        let main = (self.host().main_interface)();
        if main.is_null() {
            return Err(ActorError::Unavailable);
        }
        // Lua scripts' mailboxes carry MQ2Lua's name, which depends on how it was loaded.
        let to = to.for_loaded_lua(|| (self.host().loaded_plugin_name)(crate::lua::LUA_PLUGIN));
        let address = to_c_address(&to);
        let handle = self.handle();
        let (respond, user, drop): (Option<shim::mqemu_response_fn>, *mut c_void, Option<shim::mqemu_drop_fn>) = match pending {
            Some(pending) => (Some(request_respond), Rc::into_raw(pending).cast_mut().cast(), Some(request_drop)),
            None => (None, std::ptr::null_mut(), None),
        };
        // Ownership of `user` passes to C++, which drops it exactly once.
        let sent = unsafe {
            shim::mqemu_send(main, &handle, dropbox, &address.raw, data.as_ptr().cast(), data.len(), respond, user, drop)
        };
        if sent { Ok(()) } else { Err(ActorError::Rejected) }
    }
}

impl<P: Plugin> Mq<P> {
    /// Adds a mailbox: messages sent to this plugin's mailbox `name` (e.g. by
    /// the same plugin on another character) call `handler`. Removed
    /// automatically when the plugin unloads.
    pub fn add_mailbox<F>(&self, name: &str, handler: F) -> Result<(), ActorError>
    where
        F: Fn(&mut P, &Mq<P>, Message) + 'static,
    {
        if !valid_mailbox_name(name) {
            return Err(ActorError::InvalidName);
        }
        self.runtime().register_mailbox(name, Some(Rc::new(handler)))
    }

    /// Removes a mailbox added with [`Mq::add_mailbox`]. False if there was none.
    pub fn remove_mailbox(&self, name: &str) -> bool {
        valid_mailbox_name(name) && self.runtime().unregister_mailbox(name)
    }

    /// Sends a message without waiting for a reply. For a reply, use
    /// [`Task::request`] from a task.
    pub fn send(&self, to: &Address, data: impl AsRef<[u8]>) -> Result<(), ActorError> {
        self.runtime().send_raw(std::ptr::null_mut(), to, data.as_ref(), None)
    }

    /// Replies to a message received by one of this plugin's mailboxes.
    /// `status` is passed to the requester (0 conventionally means success).
    pub fn reply(&self, message: &Message, data: impl AsRef<[u8]>, status: u8) -> Result<(), ActorError> {
        let dropbox = message.reply_to.as_ref().map_or(std::ptr::null_mut(), |r| r.dropbox.get());
        if dropbox.is_null() {
            return Err(ActorError::MailboxGone);
        }
        let main = (self.host().main_interface)();
        if main.is_null() {
            return Err(ActorError::Unavailable);
        }
        let data = data.as_ref();
        let handle = self.runtime().handle();
        let replied = unsafe {
            shim::mqemu_reply(main, &handle, dropbox, message.raw.as_ptr(), data.as_ptr().cast(), data.len(), status)
        };
        if replied { Ok(()) } else { Err(ActorError::Rejected) }
    }
}

/// How long [`Task::request`] waits for a reply.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

impl<P: Plugin> Task<P> {
    /// Sends `data` and waits for the reply (up to [`DEFAULT_REQUEST_TIMEOUT`]).
    /// The reply arrives as a [`Message`]; the replier's status is not exposed
    /// beyond success.
    pub async fn request(&self, to: &Address, data: impl AsRef<[u8]>) -> Result<Message, ActorError> {
        self.request_timeout(to, data, DEFAULT_REQUEST_TIMEOUT).await
    }

    /// Like [`Task::request`], with a chosen timeout.
    pub async fn request_timeout(&self, to: &Address, data: impl AsRef<[u8]>, timeout: Duration) -> Result<Message, ActorError> {
        let pending = Rc::new(Pending::default());
        // Requests are sent from the plugin's default mailbox, which receives the reply.
        self.with(|mq| {
            let runtime = mq.runtime();
            let dropbox = runtime.default_dropbox().ok_or(ActorError::Unavailable)?;
            runtime.send_raw(dropbox, to, data.as_ref(), Some(pending.clone()))
        })?;
        let answered = self.wait_until_timeout(|_| pending.result.borrow().is_some(), timeout).await;
        if !answered {
            return Err(ActorError::Timeout);
        }
        pending.result.borrow_mut().take().unwrap_or(Err(ActorError::Timeout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_names() {
        assert!(valid_mailbox_name("follow"));
        assert!(valid_mailbox_name("buff_requests"));
        assert!(!valid_mailbox_name(""));
        assert!(!valid_mailbox_name("two words"));
        assert!(!valid_mailbox_name("Plugin:follow"));
    }

    #[test]
    fn addresses_build_and_display() {
        let address = Address::character("Bob").on_server("emu").mailbox("follow");
        assert_eq!(address.to_string(), "character=Bob server=emu mailbox=follow");
        assert!(!address.absolute_mailbox);
        assert!(Address::character("Bob").mailbox("lua_box").absolute().absolute_mailbox);
        assert_eq!(Address::default().to_string(), "(any)");
        assert_eq!(Address::character("Bob").via_peer("10.0.0.2", Some(7781)).to_string(), "character=Bob peer=10.0.0.2:7781");
    }

    #[test]
    fn addresses_survive_the_c_round_trip() {
        let address = Address {
            character: Some("Bob".into()),
            server: Some("emu".into()),
            account: None,
            mailbox: Some("follow".into()),
            absolute_mailbox: true,
            name: None,
            pid: Some(1234),
            uuid: Some("abc".into()),
            peer: Some(("10.0.0.2".into(), Some(7781))),
        };
        let c = to_c_address(&address);
        assert_eq!(unsafe { from_c_address(&c.raw) }, address);
        let empty = to_c_address(&Address::default());
        assert_eq!(unsafe { from_c_address(&empty.raw) }, Address::default());
    }

    #[test]
    fn statuses_map_to_errors() {
        assert_eq!(ActorError::from_status(-2), ActorError::NoConnection);
        assert_eq!(ActorError::from_status(-4), ActorError::AmbiguousRecipient);
        assert_eq!(ActorError::from_status(-9), ActorError::Status(-9));
    }
}
