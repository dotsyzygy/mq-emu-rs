//! The few runtime operations that call into MacroQuest, behind function
//! pointers so tests can substitute fakes. Code reachable from tests must go
//! through [`Host`] rather than calling MQ2Main.dll directly; otherwise the
//! test binary would need MQ2Main.dll to start.

use std::time::Instant;

use mq_emu_sys::constants::MAX_STRING;
use mq_emu_sys::shim::MainInterface;

use crate::text::to_c;

#[derive(Clone, Copy)]
pub(crate) struct Host {
    pub write_chat: fn(&str, i32),
    pub main_interface: fn() -> *mut MainInterface,
    pub game_state: fn() -> i32,
    pub now: fn() -> Instant,
}

/// The real MacroQuest host. Only referenced by `Runtime::new`, which only
/// plugins (through `plugin!`) call.
pub(crate) const REAL: Host = Host {
    write_chat: real_write_chat,
    main_interface: real_main_interface,
    game_state: real_game_state,
    now: Instant::now,
};

fn real_write_chat(text: &str, color: i32) {
    let text = to_c(truncate(text, MAX_STRING - 1));
    unsafe { mq_emu_sys::mq2main::WriteChatColor(text.as_ptr(), color, 0) }
}

fn real_main_interface() -> *mut MainInterface {
    unsafe { mq_emu_sys::shim::mqemu_main_interface() }
}

fn real_game_state() -> i32 {
    unsafe { mq_emu_sys::mq2main::GetGameState() }
}

/// Cuts `text` to at most `max_bytes`, on a character boundary. MacroQuest
/// copies chat into `MAX_STRING` buffers.
pub(crate) fn truncate(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_char_boundaries() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdef", 3), "abc");
        // 'é' is two bytes; cutting through it backs off to before it.
        assert_eq!(truncate("caf\u{e9}", 4), "caf");
        assert_eq!(truncate("", 0), "");
    }
}
