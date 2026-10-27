//! Key binds: MacroQuest's own (`/bind`, `AddMQ2KeyBind`), a plugin's among
//! them, and the game's bindings of its commands.
//!
//! MacroQuest calls a bind's function with only its name, so every bind this
//! plugin adds goes to one function, which finds the plugin's runtime through
//! a thread-local (MacroQuest calls it on the game thread, where the plugin
//! added it) and routes by name.

use std::cell::Cell;
use std::ffi::c_char;
use std::fmt;
use std::rc::Rc;

use mq_emu_sys::constants::{MAX_STRING, N_EQ_MAPPABLE_COMMANDS};
use mq_emu_sys::layouts::KeyCombo as RawKeyCombo;
use mq_emu_sys::mq2main::fMQExecuteCmd;

use crate::keys::KeyCombo;
use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::runtime::guard;
use crate::text::{from_c, to_c};

/// Why a key bind couldn't be added or set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyBindError {
    /// Bind names are printable ASCII without spaces, `=`, `[` or `]`, don't
    /// start with `~` (`/bind ~name` sets the alternate keys) or `;`, aren't
    /// one of `/bind`'s own words (`list`, `eqlist`, `clearcombo`), and are
    /// shorter than `MAX_STRING`.
    InvalidName,
    /// A key bind with that name exists (any plugin's, in any case), or a game
    /// command has it, which `/bind name` would then no longer reach.
    Taken,
    /// There's no such key bind (or game command).
    NotFound,
    /// MacroQuest has no name for the key, and binding it would crash it:
    /// it saves bindings by name.
    UnnamedKey,
    /// The game's key bindings aren't loaded.
    Unavailable,
}

impl fmt::Display for KeyBindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            KeyBindError::InvalidName => "not a usable key bind name",
            KeyBindError::Taken => "a key bind or game command already has that name",
            KeyBindError::NotFound => "no such key bind",
            KeyBindError::UnnamedKey => "MacroQuest has no name for that key, so can't bind it",
            KeyBindError::Unavailable => "the game's key bindings aren't loaded",
        })
    }
}

impl std::error::Error for KeyBindError {}

/// Which of a binding's two key combos: each command or bind has a normal and
/// an alternate one, and either sets it off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BindSlot {
    Normal,
    Alternate,
}

impl BindSlot {
    fn alternate(self) -> bool {
        self == BindSlot::Alternate
    }
}

/// A command's or bind's two key combos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KeyBinding {
    pub normal: KeyCombo,
    pub alternate: KeyCombo,
}

/// MacroQuest's key bind functions, behind pointers so tests can use a fake
/// (the real ones import MQ2Main.dll and eqlib.dll). Combos are eqlib's
/// `KeyCombo::Data`.
#[derive(Clone, Copy)]
pub(crate) struct KeyBindApi {
    pub add: fn(&str, fMQExecuteCmd) -> bool,
    pub remove: fn(&str) -> bool,
    pub get: fn(&str, bool) -> Option<[u8; 4]>,
    pub set: fn(&str, bool, [u8; 4]) -> bool,
    pub press: fn(&str, bool) -> bool,
    /// The game command's index (`FindMappableCommand`).
    pub eq_command: fn(&str) -> Option<usize>,
    /// The game command's name (`szEQMappableCommands`); None if it has none.
    pub eq_name: fn(usize) -> Option<String>,
    /// A game command's combo; None without `pKeypressHandler`.
    pub eq_get: fn(usize, bool) -> Option<[u8; 4]>,
    /// `SetEQKeyBind`; false without `pKeypressHandler`.
    pub eq_set: fn(&str, bool, [u8; 4]) -> bool,
}

/// The real functions; generic for the reason given in `host`.
#[allow(clippy::extra_unused_type_parameters)]
pub(crate) const fn real<P>() -> KeyBindApi {
    use mq_emu_sys::mq2main as mq;
    fn combo(data: &mut [u8; 4]) -> *mut RawKeyCombo {
        data.as_mut_ptr().cast()
    }
    KeyBindApi {
        add: |name, function| unsafe { mq::AddMQ2KeyBind(to_c(name).as_ptr(), function) },
        remove: |name| unsafe { mq::RemoveMQ2KeyBind(to_c(name).as_ptr()) },
        get: |name, alternate| {
            let mut data = [0; 4];
            unsafe { mq::GetMQ2KeyBind(to_c(name).as_ptr(), alternate, combo(&mut data)) }.then_some(data)
        },
        set: |name, alternate, mut data| unsafe { mq::SetMQ2KeyBind(to_c(name).as_ptr(), alternate, combo(&mut data)) },
        press: |name, hold| unsafe { mq::PressMQ2KeyBind(to_c(name).as_ptr(), hold) },
        eq_command: |name| usize::try_from(unsafe { mq::FindMappableCommand(to_c(name).as_ptr()) }).ok(),
        eq_name: |index| {
            if index >= N_EQ_MAPPABLE_COMMANDS {
                return None;
            }
            let names = (&raw const mq_emu_sys::eqlib::szEQMappableCommands).cast::<*const c_char>();
            let name = unsafe { names.add(index).read() };
            (!name.is_null()).then(|| unsafe { from_c(name) }.into_owned())
        },
        eq_get: |index, alternate| {
            let handler = unsafe { crate::host::foreign(&raw const mq_emu_sys::eqlib::pKeypressHandler) };
            if handler.is_null() || index >= N_EQ_MAPPABLE_COMMANDS {
                return None;
            }
            let handler = unsafe { &*handler };
            Some(if alternate { handler.AltKey(index) } else { handler.NormalKey(index) }.Data())
        },
        eq_set: |name, alternate, mut data| {
            let handler = unsafe { crate::host::foreign(&raw const mq_emu_sys::eqlib::pKeypressHandler) };
            !handler.is_null() && unsafe { mq::SetEQKeyBind(to_c(name).as_ptr(), alternate, combo(&mut data)) }
        },
    }
}

/// Whether MacroQuest's `/bind` and MacroQuest.ini can hold a bind by this name.
pub(crate) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() < MAX_STRING
        && name.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b'=' | b'[' | b']'))
        && !name.starts_with(['~', ';'])
        && !["list", "eqlist", "clearcombo"].iter().any(|word| word.eq_ignore_ascii_case(name))
}

/// Where this thread's key binds go: the runtime of the plugin that added them.
pub(crate) trait KeyBindTarget {
    fn key_bind(&'static self, name: &str, down: bool);
}

thread_local! {
    static TARGET: Cell<Option<&'static dyn KeyBindTarget>> = const { Cell::new(None) };
}

pub(crate) fn set_target(target: Option<&'static dyn KeyBindTarget>) {
    TARGET.with(|cell| cell.set(target));
}

/// Every key bind this plugin adds calls this.
pub(crate) unsafe extern "C" fn trampoline(name: *const c_char, down: bool) {
    let name = unsafe { from_c(name) }.into_owned();
    guard(|| {
        if let Some(target) = TARGET.with(Cell::get) {
            target.key_bind(&name, down);
        }
    });
}

pub(crate) type KeyBindHandler<P> = dyn Fn(&mut P, &Mq<P>, bool);

pub(crate) struct KeyBindSlot<P: Plugin> {
    pub name: String,
    pub handler: Rc<KeyBindHandler<P>>,
}

impl<P: Plugin> Mq<P> {
    fn key_bind_api(&self) -> &KeyBindApi {
        &self.host().key_binds
    }

    /// Adds a MacroQuest key bind, as a C++ plugin's `AddMQ2KeyBind` does:
    /// `/bind <name> <keys>` sets its keys (saved in MacroQuest.ini), and the
    /// handler runs as they go down (`true`) and up (`false`). It starts with
    /// the keys saved for that name, if any. Removed when the plugin unloads
    /// (MacroQuest itself would keep calling into the unloaded plugin).
    pub fn add_key_bind<F>(&self, name: &str, handler: F) -> Result<(), KeyBindError>
    where
        F: Fn(&mut P, &Mq<P>, bool) + 'static,
    {
        self.runtime().add_key_bind(name, Rc::new(handler))
    }

    /// Removes a key bind this plugin added. False if it hadn't. Its keys stay
    /// saved in MacroQuest.ini, for when it's added again.
    pub fn remove_key_bind(&self, name: &str) -> bool {
        self.runtime().remove_key_bind(name)
    }

    /// A MacroQuest key bind's keys: any plugin's, or MacroQuest's own
    /// (`RANGED`), by name in any case.
    pub fn key_bind(&self, name: &str) -> Option<KeyBinding> {
        let api = self.key_bind_api();
        Some(KeyBinding { normal: KeyCombo::from_raw((api.get)(name, false)?), alternate: KeyCombo::from_raw((api.get)(name, true)?) })
    }

    /// Sets one of a MacroQuest key bind's keys and saves it to MacroQuest.ini,
    /// as `/bind [~]<name> <keys>` does.
    pub fn set_key_bind(&self, name: &str, slot: BindSlot, combo: KeyCombo) -> Result<(), KeyBindError> {
        check_named(combo)?;
        let api = self.key_bind_api();
        if (api.set)(name, slot.alternate(), combo.to_raw()) { Ok(()) } else { Err(KeyBindError::NotFound) }
    }

    /// Runs a MacroQuest key bind as if its key were pressed and released.
    /// False if there's no such bind. One of this plugin's runs after the
    /// current callback.
    pub fn press_key_bind(&self, name: &str) -> bool {
        (self.key_bind_api().press)(name, false)
    }

    /// Runs a MacroQuest key bind as if its key went down, and not up.
    /// MacroQuest has no call to release it: pressing it, or its key, does.
    pub fn hold_key_bind(&self, name: &str) -> bool {
        (self.key_bind_api().press)(name, true)
    }

    /// The game's key binding for one of its commands, by the name `/bind`
    /// uses (`"forward"`, `"jump"`, `"hot1_1"`... `/bind eqlist` lists them).
    /// None if there's no such command or the bindings aren't loaded.
    pub fn eq_key_bind(&self, command: &str) -> Option<KeyBinding> {
        let api = self.key_bind_api();
        let index = (api.eq_command)(command)?;
        Some(KeyBinding {
            normal: KeyCombo::from_raw((api.eq_get)(index, false)?),
            alternate: KeyCombo::from_raw((api.eq_get)(index, true)?),
        })
    }

    /// Every game command that has a name, with its key binding, in the
    /// game's order: what `/bind eqlist` lists. Empty if the bindings aren't
    /// loaded.
    pub fn eq_key_binds(&self) -> Vec<(String, KeyBinding)> {
        let api = self.key_bind_api();
        (0..N_EQ_MAPPABLE_COMMANDS)
            .filter_map(|index| {
                let name = (api.eq_name)(index)?;
                let normal = KeyCombo::from_raw((api.eq_get)(index, false)?);
                let alternate = KeyCombo::from_raw((api.eq_get)(index, true)?);
                Some((name, KeyBinding { normal, alternate }))
            })
            .collect()
    }

    /// Sets the game's key binding for one of its commands, as `/bind`
    /// does: the game saves it with the rest of its key map.
    pub fn set_eq_key_bind(&self, command: &str, slot: BindSlot, combo: KeyCombo) -> Result<(), KeyBindError> {
        check_named(combo)?;
        let api = self.key_bind_api();
        let index = (api.eq_command)(command).ok_or(KeyBindError::NotFound)?;
        (api.eq_get)(index, false).ok_or(KeyBindError::Unavailable)?;
        if (api.eq_set)(command, slot.alternate(), combo.to_raw()) { Ok(()) } else { Err(KeyBindError::Unavailable) }
    }
}

/// MacroQuest writes a combo's key by name (`DescribeKeyCombo`), reading a
/// null name for keys it doesn't know.
fn check_named(combo: KeyCombo) -> Result<(), KeyBindError> {
    match combo.key() {
        Some(key) if key.name().is_none() => Err(KeyBindError::UnnamedKey),
        _ => Ok(()),
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! MacroQuest's key binds as a thread-local, following its rules: names
    //! are case-insensitive, adding loads the keys saved for the name, setting
    //! saves them, and nothing is removed unless asked.

    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::CString;

    use super::*;

    pub(crate) struct FakeBind {
        pub name: String,
        pub keys: [[u8; 4]; 2],
        pub function: fMQExecuteCmd,
    }

    #[derive(Default)]
    pub(crate) struct FakeKeyBinds {
        pub binds: Vec<FakeBind>,
        /// MacroQuest.ini's `[Key Binds]`, by lowercase `<name>_Nrm`/`_Alt`.
        pub saved: HashMap<String, [u8; 4]>,
        /// The game's commands and their keys, by index.
        pub eq_commands: Vec<(&'static str, [[u8; 4]; 2])>,
        /// Whether the game's bindings are loaded (`pKeypressHandler`).
        pub eq_loaded: bool,
    }

    thread_local! {
        static BINDS: RefCell<FakeKeyBinds> = RefCell::new(FakeKeyBinds {
            eq_commands: vec![("forward", [[0, 0, 0, 200], [0; 4]]), ("jump", [[0, 0, 0, 57], [0; 4]])],
            eq_loaded: true,
            ..FakeKeyBinds::default()
        });
    }

    pub(crate) fn with<R>(f: impl FnOnce(&mut FakeKeyBinds) -> R) -> R {
        BINDS.with(|binds| f(&mut binds.borrow_mut()))
    }

    fn saved_key(name: &str, alternate: bool) -> String {
        format!("{}_{}", name.to_ascii_lowercase(), if alternate { "alt" } else { "nrm" })
    }

    fn find(binds: &FakeKeyBinds, name: &str) -> Option<usize> {
        binds.binds.iter().position(|bind| bind.name.eq_ignore_ascii_case(name))
    }

    /// Calls a bind's function as MacroQuest does, outside the borrow.
    fn call(function: fMQExecuteCmd, name: &str, down: bool) {
        let name = CString::new(name).unwrap();
        unsafe { function(name.as_ptr(), down) };
    }

    /// A key going down or up, reaching every bind it's bound to.
    pub(crate) fn key(combo: KeyCombo, down: bool) {
        let hit: Vec<_> = with(|binds| {
            binds.binds.iter().filter(|bind| bind.keys.contains(&combo.to_raw())).map(|bind| (bind.function, bind.name.clone())).collect()
        });
        for (function, name) in hit {
            call(function, &name, down);
        }
    }

    pub(crate) fn has(name: &str) -> bool {
        with(|binds| find(binds, name).is_some())
    }

    pub(crate) const API: KeyBindApi = KeyBindApi {
        add: |name, function| {
            with(|binds| {
                if find(binds, name).is_some() {
                    return false;
                }
                let keys = [false, true].map(|alternate| binds.saved.get(&saved_key(name, alternate)).copied().unwrap_or_default());
                binds.binds.push(FakeBind { name: name.to_owned(), keys, function });
                true
            })
        },
        remove: |name| {
            with(|binds| match find(binds, name) {
                Some(i) => {
                    binds.binds.remove(i);
                    true
                }
                None => false,
            })
        },
        get: |name, alternate| with(|binds| find(binds, name).map(|i| binds.binds[i].keys[usize::from(alternate)])),
        set: |name, alternate, data| {
            with(|binds| {
                let Some(i) = find(binds, name) else { return false };
                binds.binds[i].keys[usize::from(alternate)] = data;
                binds.saved.insert(saved_key(name, alternate), data);
                true
            })
        },
        press: |name, hold| {
            let Some((function, name)) = with(|binds| find(binds, name).map(|i| (binds.binds[i].function, binds.binds[i].name.clone())))
            else {
                return false;
            };
            call(function, &name, true);
            if !hold {
                call(function, &name, false);
            }
            true
        },
        eq_command: |name| with(|binds| binds.eq_commands.iter().position(|(command, _)| command.eq_ignore_ascii_case(name))),
        eq_name: |index| with(|binds| binds.eq_commands.get(index).map(|(command, _)| (*command).to_owned())),

        eq_get: |index, alternate| with(|binds| binds.eq_commands.get(index).filter(|_| binds.eq_loaded).map(|(_, keys)| keys[usize::from(alternate)])),
        eq_set: |name, alternate, data| {
            with(|binds| {
                let loaded = binds.eq_loaded;
                match binds.eq_commands.iter_mut().find(|(command, _)| command.eq_ignore_ascii_case(name)) {
                    Some((_, keys)) if loaded => {
                        keys[usize::from(alternate)] = data;
                        true
                    }
                    _ => false,
                }
            })
        },
    };
}
