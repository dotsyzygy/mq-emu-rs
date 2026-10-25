//! Selftest checks across characters. Every other character running this
//! plugin (found by asking them all) is pinged and shown a line; where the
//! rustpeer Lua script runs, Rust and Lua are checked across characters too.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mq_emu::{Address, Task};

use crate::RustTest;
use crate::checks::{self, Check, Report};
use crate::lua_checks;

/// How long to wait for the first answer to "who".
const DISCOVERY_TIME: Duration = Duration::from_millis(1500);
/// Once someone has answered, how long without a new answer ends the wait.
const DISCOVERY_QUIET: Duration = Duration::from_millis(250);
const TIMEOUT: Duration = Duration::from_secs(5);
/// Pings per latency measurement.
const LATENCY_PINGS: usize = 50;
/// Longest random pause before each ping. Back-to-back pings lock onto the
/// other client's frame (each is sent just after its reply left, so it waits
/// almost a whole frame there); pausing a random time samples every phase,
/// as real messages do.
const MAX_JITTER_MS: u64 = 40;

/// Round trips of back-to-back pings: milliseconds, and this client's
/// pulses that passed during each.
struct Latency {
    millis: Vec<f64>,
    pulses: Vec<u64>,
}

impl Latency {
    fn summary(&self) -> String {
        let mut millis = self.millis.clone();
        millis.sort_by(f64::total_cmp);
        let mut pulses = self.pulses.clone();
        pulses.sort_unstable();
        let at = |p: f64| ((millis.len() - 1) as f64 * p).round() as usize;
        format!(
            "{} pings: min {:.1} / median {:.1} / p90 {:.1} / max {:.1} ms; {}-{} pulses (median {})",
            millis.len(),
            millis[0],
            millis[at(0.5)],
            millis[at(0.9)],
            millis[millis.len() - 1],
            pulses[0],
            pulses[pulses.len() - 1],
            pulses[at(0.5)],
        )
    }
}

/// Sends `LATENCY_PINGS` pings one after another, each answered "pong from `name`".
async fn measure(task: &Task<RustTest>, name: &str) -> Result<Latency, String> {
    let to = Address::character(name);
    let expected = format!("pong from {name}");
    let mut latency = Latency { millis: Vec::new(), pulses: Vec::new() };
    // A small xorshift generator; the pauses only need to differ, not be secure.
    let clock = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64;
    let mut seed = (clock ^ 0x9e37_79b9_7f4a_7c15) | 1;
    for _ in 0..LATENCY_PINGS {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        task.delay(Duration::from_millis(seed % (MAX_JITTER_MS + 1))).await;
        let (started, pulses_before) = (Instant::now(), task.with(|mq| mq.stats().pulses));
        let reply = task.request_timeout(&to, "ping", TIMEOUT).await.map_err(|e| e.to_string())?;
        latency.millis.push(started.elapsed().as_secs_f64() * 1000.0);
        latency.pulses.push(task.with(|mq| mq.stats().pulses) - pulses_before);
        if reply.text() != expected {
            return Err(format!("got {:?}", reply.text()));
        }
    }
    Ok(latency)
}

/// Prints a latency measurement to `name`, with both clients' frame times.
async fn report_latency(task: &Task<RustTest>, report: &mut Report, name: &str, label: &str) {
    let latency = measure(task, name).await;
    let theirs = task.request_timeout(&Address::character(name), "frame", TIMEOUT).await;
    task.with_plugin(|plugin, mq| {
        let ours = plugin.frame_ms().map_or_else(|| "?".to_owned(), |ms| format!("{ms:.1}"));
        let theirs = theirs.map_or_else(|_| "?".to_owned(), |reply| reply.text().into_owned());
        match &latency {
            Ok(latency) => report.print(
                mq,
                &format!("{label}: {}; frame {ours} ms here, {theirs} ms there", latency.summary()),
                Check::Pass,
            ),
            Err(e) => report.print(mq, &format!("{label}: latency"), Check::Fail(e.clone())),
        }
    });
}

/// `lua_here`: whether the rustpeer script runs on this character.
pub async fn run(task: &Task<RustTest>, report: &mut Report, lua_here: bool) {
    let Some(me) = task.with(|mq| mq.me().map(|me| me.name())) else {
        task.with(|mq| report.print(mq, "other characters", Check::Skip("needs to be in game".into())));
        return;
    };

    // A baseline: this client to itself, through the same post office.
    report_latency(task, report, &me, "yourself").await;

    // Ask every character running the plugin who they are; they answer "here".
    task.with_plugin(|plugin, _| plugin.peers = Some(Default::default()));
    let asked = task.with(|mq| mq.send(&Address::default(), "who"));
    if asked.is_ok() {
        // Answers take milliseconds; stop once they've stopped coming.
        let started = Instant::now();
        let (mut seen, mut last_new) = (0, started);
        loop {
            let now = Instant::now();
            let count = task.with_plugin(|plugin, _| plugin.peers.as_ref().map_or(0, |peers| peers.len()));
            if count > seen {
                (seen, last_new) = (count, now);
            }
            let done = if seen == 0 { now - started >= DISCOVERY_TIME } else { now - last_new >= DISCOVERY_QUIET };
            if done {
                break;
            }
            task.next_pulse().await;
        }
    }
    let peers = task.with_plugin(|plugin, _| plugin.peers.take()).unwrap_or_default();
    let found = task.with(|mq| {
        let check = match &asked {
            Err(e) => Check::Fail(format!("couldn't ask: {e}")),
            Ok(()) if peers.is_empty() => Check::Skip("none found; load the plugin on another character to check messaging between them".into()),
            Ok(()) => Check::Pass,
        };
        let names = peers.iter().cloned().collect::<Vec<_>>().join(", ");
        report.print(mq, &format!("other characters running the plugin: {names}"), check);
        !peers.is_empty()
    });
    if !found {
        return;
    }

    for peer in peers {
        let them = Address::character(peer.clone());

        let started = Instant::now();
        let pong = task.request_timeout(&them, "ping", TIMEOUT).await;
        let took = started.elapsed().as_secs_f64() * 1000.0;
        task.with(|mq| {
            report.print(mq, &format!("{peer}: answers a ping ({took:.1} ms round trip)"), match &pong {
                Ok(reply) => checks::same_text(reply.text(), format!("pong from {peer}")),
                Err(e) => Check::Fail(e.to_string()),
            })
        });

        report_latency(task, report, &peer, &peer).await;

        let shown = task.request_timeout(&them, format!("show:selftest line from {me}"), TIMEOUT).await;
        task.with(|mq| {
            report.print(mq, &format!("{peer}: shows a line you sent"), match &shown {
                Ok(reply) => checks::same_text(reply.text(), "shown".to_owned()),
                Err(e) => Check::Fail(e.to_string()),
            })
        });

        let echo = lua_checks::ask(task, &lua_checks::peer(&peer), lua_checks::request("echo").with("value", "probe")).await;
        task.with(|mq| {
            report.print(mq, &format!("{peer}: Rust here -> Lua there"), match &echo {
                Ok(value) => Check::that(value.as_str() == Some("probe"), || format!("got {value:?}")),
                Err(e) => Check::Skip(format!("run /lua run {} on {peer} ({e})", lua_checks::PEER_SCRIPT)),
            })
        });

        if lua_here {
            let token = lua_checks::token();
            let last = lua_checks::lua_calls_rust(task, &lua_checks::peer(&me), &peer, token).await;
            task.with(|mq| {
                report.print(mq, &format!("{peer}: Lua here -> Rust there, and the reply back"), match &last {
                    Ok(value) => Check::that(*value == lua_checks::expected_hello_back(token), || format!("Lua got {value:?}")),
                    Err(e) => Check::Fail(e.clone()),
                })
            });
        }
    }
}
