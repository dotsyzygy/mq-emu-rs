//! The string IDs the client has turned into text, for readers to poll.
//!
//! Writers (the hook, on whatever thread the client looks a string up) only
//! record IDs someone watches, so the client's constant lookups for its UI
//! cost a bit test. Each reader keeps its own cursor; a reader that falls more
//! than [`CAPACITY`] behind is told how many it missed.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Recorded IDs kept for readers.
pub const CAPACITY: usize = 4096;
/// IDs at or above this can't be watched. eqstr_us.txt's go to about 13,000.
pub const MAX_ID: u32 = 1 << 16;

pub struct Ring {
    /// Each slot: the sequence number written there (high 32 bits, plus one so
    /// an empty slot reads as 0) and the ID (low 32 bits).
    slots: [AtomicU64; CAPACITY],
    /// The next sequence number to write.
    head: AtomicU64,
    watched: [AtomicU32; (MAX_ID / 32) as usize],
}

/// What [`Ring::poll`] read.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Polled {
    pub ids: Vec<u32>,
    /// IDs written over before this reader got to them.
    pub missed: u64,
}

impl Ring {
    pub const fn new() -> Ring {
        Ring {
            slots: [const { AtomicU64::new(0) }; CAPACITY],
            head: AtomicU64::new(0),
            watched: [const { AtomicU32::new(0) }; (MAX_ID / 32) as usize],
        }
    }

    /// Starts recording `id`. False if it's too large to watch.
    pub fn watch(&self, id: u32) -> bool {
        if id >= MAX_ID {
            return false;
        }
        self.watched[(id / 32) as usize].fetch_or(1 << (id % 32), Ordering::Relaxed);
        true
    }

    pub fn is_watched(&self, id: u32) -> bool {
        id < MAX_ID && self.watched[(id / 32) as usize].load(Ordering::Relaxed) & (1 << (id % 32)) != 0
    }

    /// Records `id` if it's watched. Never blocks or allocates.
    pub fn record(&self, id: u32) {
        if !self.is_watched(id) {
            return;
        }
        let sequence = self.head.fetch_add(1, Ordering::AcqRel);
        let tag = (sequence as u32).wrapping_add(1);
        self.slots[sequence as usize % CAPACITY].store(u64::from(tag) << 32 | u64::from(id), Ordering::Release);
    }

    /// Where a new reader starts: after everything recorded so far.
    pub fn head(&self) -> u64 {
        self.head.load(Ordering::Acquire)
    }

    /// The IDs recorded since `cursor`, which moves past them, oldest first.
    /// A slot still being written by another thread ends the read early; it's
    /// read next time.
    pub fn poll(&self, cursor: &mut u64) -> Polled {
        self.poll_at_most(cursor, usize::MAX)
    }

    /// Like [`Ring::poll`], reading at most `max` IDs; the rest wait.
    pub fn poll_at_most(&self, cursor: &mut u64, max: usize) -> Polled {
        let head = self.head();
        let mut polled = Polled::default();
        if head.saturating_sub(*cursor) > CAPACITY as u64 {
            let oldest = head - CAPACITY as u64;
            polled.missed = oldest - *cursor;
            *cursor = oldest;
        }
        while *cursor < head && polled.ids.len() < max {
            let slot = self.slots[*cursor as usize % CAPACITY].load(Ordering::Acquire);
            let tag = (*cursor as u32).wrapping_add(1);
            if (slot >> 32) as u32 != tag {
                break;
            }
            polled.ids.push(slot as u32);
            *cursor += 1;
        }
        polled
    }
}

impl Default for Ring {
    fn default() -> Ring {
        Ring::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_watched_ids_are_recorded_and_each_reader_reads_them_once() {
        let ring = Box::new(Ring::new());
        let mut early = ring.head();
        ring.record(173);
        assert_eq!(ring.head(), 0, "unwatched IDs cost nothing");
        assert!(ring.watch(173) && ring.watch(425));
        ring.record(173);
        ring.record(9);
        let mut late = ring.head();
        ring.record(425);
        assert_eq!(ring.poll(&mut early), Polled { ids: vec![173, 425], missed: 0 });
        assert_eq!(ring.poll(&mut early), Polled::default(), "read once");
        assert_eq!(ring.poll(&mut late).ids, [425], "a reader starts where it joined");
    }

    #[test]
    fn a_reader_that_falls_behind_is_told_what_it_missed() {
        let ring = Box::new(Ring::new());
        ring.watch(1);
        let mut cursor = ring.head();
        for _ in 0..CAPACITY + 10 {
            ring.record(1);
        }
        let polled = ring.poll(&mut cursor);
        assert_eq!((polled.ids.len(), polled.missed), (CAPACITY, 10));
        assert_eq!(cursor, ring.head());
    }

    #[test]
    fn ids_too_large_to_watch_are_refused() {
        let ring = Box::new(Ring::new());
        assert!(!ring.watch(MAX_ID));
        assert!(!ring.is_watched(MAX_ID));
        ring.record(MAX_ID);
        assert_eq!(ring.head(), 0);
    }

    #[test]
    fn a_slot_still_being_written_ends_the_read() {
        let ring = Box::new(Ring::new());
        ring.watch(7);
        let mut cursor = ring.head();
        ring.record(7);
        // Another thread has taken sequence 1 but not stored its slot yet.
        ring.head.fetch_add(1, Ordering::AcqRel);
        assert_eq!(ring.poll(&mut cursor).ids, [7]);
        assert_eq!(cursor, 1, "waits at the unwritten slot");
        ring.slots[1].store(2 << 32 | 7, Ordering::Release);
        assert_eq!(ring.poll(&mut cursor).ids, [7]);
    }

    #[test]
    fn threads_recording_at_once_lose_nothing() {
        let ring: &'static Ring = Box::leak(Box::new(Ring::new()));
        ring.watch(5);
        ring.watch(6);
        let mut cursor = ring.head();
        let threads: Vec<_> = [5, 6].into_iter().map(|id| std::thread::spawn(move || (0..1000).for_each(|_| ring.record(id)))).collect();
        threads.into_iter().for_each(|t| t.join().unwrap());
        let polled = ring.poll(&mut cursor);
        assert_eq!(polled.ids.iter().filter(|id| **id == 5).count(), 1000);
        assert_eq!(polled.ids.iter().filter(|id| **id == 6).count(), 1000);
    }
}
