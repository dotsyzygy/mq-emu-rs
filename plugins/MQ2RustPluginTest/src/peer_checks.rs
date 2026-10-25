//! Selftest checks across characters. Every other character running this
//! plugin (found by asking them all) is pinged and shown a line; where the
//! rustpeer Lua script runs, Rust and Lua are checked across characters too.

use std::time::{Duration, Instant};

use mq_emu::{Address, Task};

use crate::RustTest;
use crate::checks::{self, Check, Report};
use crate::lua_checks;

/// How long to wait for the first answer to "who".
const DISCOVERY_TIME: Duration = Duration::from_millis(1500);
/// Once someone has answered, how long without a new answer ends the wait.
const DISCOVERY_QUIET: Duration = Duration::from_millis(250);
const TIMEOUT: Duration = Duration::from_secs(5);

/// `lua_here`: whether the rustpeer script runs on this character.
pub async fn run(task: &Task<RustTest>, report: &mut Report, lua_here: bool) {
    let Some(me) = task.with(|mq| mq.me().map(|me| me.name())) else {
        task.with(|mq| report.print(mq, "other characters", Check::Skip("needs to be in game".into())));
        return;
    };

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
