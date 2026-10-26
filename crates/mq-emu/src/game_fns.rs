//! Calling the game's own functions (`mq_emu_sys::game`) through
//! [`Host::game_fns`](crate::host::Host), reading the game's arrays, and a
//! fake of the functions for tests.

use std::ffi::c_char;

use crate::text::from_c_array;

/// Runs a shim function that writes NUL-terminated text into a buffer and
/// says whether there was any.
pub(crate) fn text(f: impl FnOnce(*mut c_char, usize) -> bool) -> Option<String> {
    let mut buffer = [0 as c_char; 512];
    f(buffer.as_mut_ptr(), buffer.len()).then(|| from_c_array(&buffer))
}

/// The most elements [`game_array`] reads, so a damaged length can't send a
/// walk through all of memory. Far above any real list it's used on.
pub(crate) const MAX_ARRAY_LEN: usize = 4096;

/// The `len` elements at `data` (an `ArrayClass`'s `m_array` and
/// `m_length`): empty if `data` is null or `len` isn't positive, and at most
/// [`MAX_ARRAY_LEN`].
///
/// # Safety
/// Unless null, `data` must point to `len` (or [`MAX_ARRAY_LEN`], if fewer)
/// live elements for `'a`.
pub(crate) unsafe fn game_array<'a, T>(data: *const T, len: i32) -> &'a [T] {
    let len = usize::try_from(len).unwrap_or(0).min(MAX_ARRAY_LEN);
    if data.is_null() || len == 0 { &[] } else { unsafe { std::slice::from_raw_parts(data, len) } }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_arrays_are_bounded_and_null_safe() {
        let values = [1, 2, 3];
        assert_eq!(unsafe { game_array(values.as_ptr(), 2) }, [1, 2]);
        assert!(unsafe { game_array(values.as_ptr(), 0) }.is_empty());
        assert!(unsafe { game_array(values.as_ptr(), -1) }.is_empty());
        assert!(unsafe { game_array(std::ptr::null::<i32>(), 3) }.is_empty());
        let many = vec![0u8; MAX_ARRAY_LEN + 10];
        assert_eq!(unsafe { game_array(many.as_ptr(), i32::MAX) }.len(), MAX_ARRAY_LEN);
    }

    #[test]
    fn text_is_none_unless_the_function_says_there_is_some() {
        assert_eq!(text(|_, _| false), None);
        assert_eq!(text(|out, _| unsafe { *out = b'A' as c_char; true }), Some("A".to_owned()));
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! Stand-ins for the shim's game functions, answering from thread-locals
    //! a test fills in.

    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ffi::c_char;

    use mq_emu_sys::game::{Api, mqemu_cached_buff};
    use mq_emu_sys::layouts::{CAltAbilityData, PlayerClient};

    /// A skill as the fake game knows it.
    #[derive(Debug, Clone, Default)]
    pub(crate) struct FakeSkill {
        pub name: String,
        /// With modifiers, trained, cap.
        pub values: [i32; 3],
        pub flags: u32,
        /// Reuse left and total, in ms.
        pub timer: (i32, i32),
    }

    #[derive(Default)]
    pub(crate) struct FakeGame {
        /// Cached buffs by spawn.
        pub cached_buffs: HashMap<usize, Vec<mqemu_cached_buff>>,
        /// Alternate abilities by index.
        pub alt_abilities: HashMap<i32, *mut CAltAbilityData>,
        /// Ready and reuse seconds, by ability.
        pub alt_ability_ready: HashMap<usize, (bool, i32)>,
        /// The string table, by (id, kind).
        pub strings: HashMap<(i32, i32), String>,
        pub skills: HashMap<i32, FakeSkill>,
        pub xtarget_roles: HashMap<u32, String>,
    }

    thread_local! {
        pub(crate) static GAME: RefCell<FakeGame> = RefCell::new(FakeGame::default());
    }

    /// Changes the fake game for this thread.
    pub(crate) fn with<R>(f: impl FnOnce(&mut FakeGame) -> R) -> R {
        GAME.with(|game| f(&mut game.borrow_mut()))
    }

    fn write(text: Option<&str>, out: *mut c_char, cap: usize) -> bool {
        let Some(text) = text else { return false };
        let len = text.len().min(cap - 1);
        unsafe {
            std::ptr::copy_nonoverlapping(text.as_ptr().cast(), out, len);
            *out.add(len) = 0;
        }
        true
    }

    unsafe extern "C" fn cached_buffs(spawn: *mut PlayerClient, out: *mut mqemu_cached_buff, cap: usize) -> usize {
        with(|game| {
            let buffs = game.cached_buffs.get(&(spawn as usize)).cloned().unwrap_or_default();
            for (i, buff) in buffs.iter().take(cap).enumerate() {
                unsafe { *out.add(i) = *buff };
            }
            buffs.len()
        })
    }

    unsafe extern "C" fn alt_ability(index: i32) -> *mut CAltAbilityData {
        with(|game| game.alt_abilities.get(&index).copied().unwrap_or(std::ptr::null_mut()))
    }

    unsafe extern "C" fn alt_ability_ready(ability: *mut CAltAbilityData, reuse_seconds: *mut i32) -> bool {
        let (ready, reuse) = with(|game| game.alt_ability_ready.get(&(ability as usize)).copied().unwrap_or((false, 0)));
        unsafe { *reuse_seconds = reuse };
        ready
    }

    unsafe extern "C" fn db_string(id: i32, kind: i32, out: *mut c_char, cap: usize) -> bool {
        with(|game| write(game.strings.get(&(id, kind)).map(String::as_str), out, cap))
    }

    unsafe extern "C" fn skill_value(skill: i32, which: i32) -> i32 {
        with(|game| game.skills.get(&skill).map_or(0, |s| s.values[which as usize]))
    }

    unsafe extern "C" fn skill_flags(skill: i32) -> u32 {
        with(|game| game.skills.get(&skill).map_or(0, |s| s.flags))
    }

    unsafe extern "C" fn skill_timer(skill: i32, total: *mut i32) -> i32 {
        let (left, all) = with(|game| game.skills.get(&skill).map_or((0, 0), |s| s.timer));
        unsafe { *total = all };
        left
    }

    unsafe extern "C" fn skill_name(skill: i32, out: *mut c_char, cap: usize) -> bool {
        with(|game| write(game.skills.get(&skill).map(|s| s.name.as_str()), out, cap))
    }

    unsafe extern "C" fn xtarget_role_name(role: u32, out: *mut c_char, cap: usize) -> bool {
        with(|game| write(game.xtarget_roles.get(&role).map(String::as_str), out, cap))
    }

    pub(crate) static API: Api = Api {
        mqemu_cached_buffs: cached_buffs,
        mqemu_alt_ability: alt_ability,
        mqemu_alt_ability_ready: alt_ability_ready,
        mqemu_db_string: db_string,
        mqemu_skill_value: skill_value,
        mqemu_skill_flags: skill_flags,
        mqemu_skill_timer: skill_timer,
        mqemu_skill_name: skill_name,
        mqemu_xtarget_role_name: xtarget_role_name,
    };
}
