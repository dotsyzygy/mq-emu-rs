//! Your friends list (`${Friends}`).

use crate::game_fns::text;
use crate::mq::Mq;
use crate::plugin::Plugin;

impl<P: Plugin> Mq<P> {
    /// Your friends list, in the friends window's order (`${Friends.Friend[n]}`,
    /// from 1). `None` without the chat service that keeps it.
    pub fn friends(&self) -> Option<Vec<String>> {
        let api = self.host().game_fns;
        let count = unsafe { (api.mqemu_friend_count)() };
        let count = usize::try_from(count).ok()?;
        Some((0..count).filter_map(|i| text(|out, cap| unsafe { (api.mqemu_friend_name)(i as i32, out, cap) })).collect())
    }

    /// Whether `name` is on your friends list, ignoring case (`${Friends.Friend[name]}`).
    pub fn is_friend(&self, name: &str) -> bool {
        self.friends().is_some_and(|friends| friends.iter().any(|friend| friend.eq_ignore_ascii_case(name)))
    }
}
