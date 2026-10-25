//! Runtime statistics: collected silently, shown only when asked for, plus
//! rate-limited warnings when something is actually wrong.

use std::time::{Duration, Instant};

/// Counters kept by the runtime for one plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    /// `on_pulse` calls.
    pub pulses: u64,
    /// Total time spent in `on_pulse` (including queued events it caused).
    pub pulse_time: Duration,
    /// The slowest single pulse.
    pub slowest_pulse: Duration,
    /// Events that arrived while a callback was running and were delivered
    /// right after it (see the crate docs on re-entrant events).
    pub queued: u64,
    /// Re-entrant events that were dropped: queue full, the per-callback cap
    /// was hit, or the event can't be queued (spawn and ground item events).
    pub dropped: u64,
    /// Times the per-callback delivery cap was hit (a likely feedback loop).
    pub cap_hits: u64,
    /// Panics caught in plugin callbacks.
    pub panics: u64,
}

impl Stats {
    pub const fn new() -> Stats {
        Stats {
            pulses: 0,
            pulse_time: Duration::ZERO,
            slowest_pulse: Duration::ZERO,
            queued: 0,
            dropped: 0,
            cap_hits: 0,
            panics: 0,
        }
    }

    pub fn average_pulse(&self) -> Duration {
        if self.pulses == 0 {
            Duration::ZERO
        } else {
            self.pulse_time / u32::try_from(self.pulses).unwrap_or(u32::MAX)
        }
    }

    /// A short human-readable summary, one entry per chat line.
    pub fn summary(&self) -> [String; 2] {
        [
            format!(
                "pulses: {} | avg {:.3} ms | slowest {:.3} ms",
                self.pulses,
                millis(self.average_pulse()),
                millis(self.slowest_pulse)
            ),
            format!(
                "re-entrant events: {} queued, {} dropped (cap hits: {}) | panics: {}",
                self.queued, self.dropped, self.cap_hits, self.panics
            ),
        ]
    }
}

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Default threshold for a "slow" pulse: one frame at 60 FPS.
pub const DEFAULT_SLOW_PULSE: Duration = Duration::from_millis(16);
/// Minimum time between two warnings.
pub const WARNING_INTERVAL: Duration = Duration::from_secs(30);

/// Collects problems and turns them into at most one warning line per
/// [`WARNING_INTERVAL`], summarizing everything since the previous warning.
#[derive(Debug, Clone)]
pub(crate) struct Warnings {
    pub slow_threshold: Option<Duration>,
    interval: Duration,
    last_report: Option<Instant>,
    slow_pulses: u64,
    worst_pulse: Duration,
    dropped: u64,
    panics: u64,
}

impl Warnings {
    pub const fn new() -> Warnings {
        Warnings {
            slow_threshold: Some(DEFAULT_SLOW_PULSE),
            interval: WARNING_INTERVAL,
            last_report: None,
            slow_pulses: 0,
            worst_pulse: Duration::ZERO,
            dropped: 0,
            panics: 0,
        }
    }

    pub fn record_pulse(&mut self, took: Duration) {
        if self.slow_threshold.is_some_and(|limit| took > limit) {
            self.slow_pulses += 1;
            self.worst_pulse = self.worst_pulse.max(took);
        }
    }

    pub fn record_dropped(&mut self, count: u64) {
        self.dropped += count;
    }

    pub fn record_panic(&mut self) {
        self.panics += 1;
    }

    /// Returns a warning if anything is pending and the interval has passed.
    pub fn poll(&mut self, now: Instant) -> Option<String> {
        if self.slow_pulses == 0 && self.dropped == 0 && self.panics == 0 {
            return None;
        }
        if self.last_report.is_some_and(|last| now.saturating_duration_since(last) < self.interval) {
            return None;
        }

        let mut parts = Vec::new();
        if self.slow_pulses > 0 {
            parts.push(format!(
                "{} slow pulse{} (worst {:.1} ms)",
                self.slow_pulses,
                if self.slow_pulses == 1 { "" } else { "s" },
                millis(self.worst_pulse)
            ));
        }
        if self.dropped > 0 {
            parts.push(format!("{} re-entrant event{} dropped", self.dropped, if self.dropped == 1 { "" } else { "s" }));
        }
        if self.panics > 0 {
            parts.push(format!("{} panic{}", self.panics, if self.panics == 1 { "" } else { "s" }));
        }

        self.last_report = Some(now);
        self.slow_pulses = 0;
        self.worst_pulse = Duration::ZERO;
        self.dropped = 0;
        self.panics = 0;
        Some(parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn average_handles_no_pulses() {
        assert_eq!(Stats::new().average_pulse(), Duration::ZERO);
        let stats = Stats { pulses: 4, pulse_time: 10 * MS, ..Stats::new() };
        assert_eq!(stats.average_pulse(), Duration::from_micros(2500));
    }

    #[test]
    fn summary_is_two_lines() {
        let stats = Stats { pulses: 2, pulse_time: 3 * MS, slowest_pulse: 2 * MS, queued: 5, dropped: 1, cap_hits: 1, panics: 0 };
        let [first, second] = stats.summary();
        assert_eq!(first, "pulses: 2 | avg 1.500 ms | slowest 2.000 ms");
        assert_eq!(second, "re-entrant events: 5 queued, 1 dropped (cap hits: 1) | panics: 0");
    }

    #[test]
    fn nothing_to_report_is_silent() {
        let mut w = Warnings::new();
        w.record_pulse(MS);
        assert_eq!(w.poll(Instant::now()), None);
    }

    #[test]
    fn first_problem_is_reported_immediately_and_summarized() {
        let mut w = Warnings::new();
        w.record_pulse(20 * MS);
        w.record_pulse(41 * MS);
        w.record_pulse(5 * MS);
        w.record_dropped(3);
        assert_eq!(w.poll(Instant::now()).unwrap(), "2 slow pulses (worst 41.0 ms), 3 re-entrant events dropped");
    }

    #[test]
    fn warnings_are_rate_limited() {
        let start = Instant::now();
        let mut w = Warnings::new();
        w.record_pulse(20 * MS);
        assert!(w.poll(start).is_some());

        // More problems within the interval stay pending...
        w.record_pulse(30 * MS);
        w.record_panic();
        assert_eq!(w.poll(start + Duration::from_secs(10)), None);
        assert_eq!(w.poll(start + Duration::from_secs(29)), None);
        // ...and are summarized once it has passed.
        assert_eq!(w.poll(start + Duration::from_secs(30)).unwrap(), "1 slow pulse (worst 30.0 ms), 1 panic");
        assert_eq!(w.poll(start + Duration::from_secs(90)), None);
    }

    #[test]
    fn disabled_threshold_ignores_slow_pulses() {
        let mut w = Warnings::new();
        w.slow_threshold = None;
        w.record_pulse(Duration::from_secs(1));
        assert_eq!(w.poll(Instant::now()), None);
    }
}
