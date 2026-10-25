//! The smallest possible plugin. Built as a DLL by `cargo test`, so the
//! `plugin!` macro's expansion is compiled and linked for real;
//! `tests/plugin_exports.rs` then checks its exports.

use mq_emu::{Mq, Plugin};

struct Minimal;

impl Plugin for Minimal {
    fn new(mq: &Mq<Self>) -> Self {
        mq.chat("minimal plugin loaded");
        Minimal
    }
}

mq_emu::plugin!(Minimal);
