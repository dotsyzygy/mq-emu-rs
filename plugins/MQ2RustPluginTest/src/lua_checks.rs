//! Selftest checks against a real Lua script (lua/rustpeer.lua) through
//! MacroQuest's actors: Rust -> Lua, Lua's reading of Rust's values, and
//! Lua -> Rust with a reply back to Lua.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mq_emu::lua::LUA_PLUGIN;
use mq_emu::{Address, LuaTable, LuaValue, Task};

use crate::RustTest;
use crate::checks::{Check, Report};

/// The script, as run with `/lua run rustpeer`.
pub const PEER_SCRIPT: &str = "rustpeer";
const TIMEOUT: Duration = Duration::from_secs(5);

/// The Lua script on this character.
pub fn peer(me: &str) -> Address {
    Address::lua_script(PEER_SCRIPT).on_character(me)
}

/// A request table: `{ id = id, ... }`.
pub fn request(id: &str) -> LuaTable {
    LuaTable::new().with("id", id)
}

/// Every kind of value Lua can carry, to send through Lua and back.
fn everything() -> LuaValue {
    let mut list = LuaTable::new();
    for n in [10, 20, 30] {
        list.push(n);
    }
    LuaTable::new()
        .with("name", "café 🐉")
        .with("level", 65)
        .with("fraction", -12.5)
        .with("flag", true)
        .with("off", false)
        .with("list", list)
        .with("nested", LuaTable::new().with("deeper", "ok"))
        .with("pos", LuaValue::Vec2([1.5, -2.0]))
        .with("color", LuaValue::Vec4([0.25, 0.5, 0.75, 1.0]))
        .into()
}

/// Sends `value` to the script and decodes its reply.
pub async fn ask(task: &Task<RustTest>, to: &Address, value: impl Into<LuaValue>) -> Result<LuaValue, String> {
    let reply = task.request_timeout(to, value.into().encode(), TIMEOUT).await.map_err(|e| e.to_string())?;
    reply.lua().map_err(|e| e.to_string())
}

fn field<'a>(value: &'a LuaValue, key: &str) -> Option<&'a LuaValue> {
    value.get(key)
}

/// A token that differs every run, so an old reply can't pass for a new one.
pub fn token() -> f64 {
    f64::from(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos() % 1_000_000)
}

/// Has the script on `script` message MQ2RustPluginTest on `character`, and
/// returns what the script's response callback then received.
pub async fn lua_calls_rust(task: &Task<RustTest>, script: &Address, character: &str, token: f64) -> Result<LuaValue, String> {
    ask(task, script, request("call_rust").with("character", character).with("token", token)).await?;
    // The callback runs on a later pulse; poll for it.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let last = ask(task, script, request("last_reply")).await?;
        if field(&last, "id").and_then(LuaValue::as_str) != Some("none") || Instant::now() >= deadline {
            return Ok(last);
        }
        task.delay(Duration::from_millis(100)).await;
    }
}

/// What the script's callback holds after Rust answered its `hello`.
pub fn expected_hello_back(token: f64) -> LuaValue {
    LuaTable::new().with("status", 0).with("content", LuaTable::new().with("id", "hello_back").with("token", token)).into()
}

/// The Lua checks on this character. True if the script is running here.
pub async fn run(task: &Task<RustTest>, report: &mut Report) -> bool {
    let Some(me) = task.with(|mq| mq.me().map(|me| me.name())) else {
        task.with(|mq| report.print(mq, "Lua", Check::Skip("needs to be in game".into())));
        return false;
    };
    let script = peer(&me);

    // Is the script running? Without it, the rest can't run.
    let probe = ask(task, &script, request("echo").with("value", "probe")).await;
    if let Err(e) = &probe {
        task.with(|mq| {
            report.print(mq, "Lua", Check::Skip(format!("run /lua run {PEER_SCRIPT} for the Lua checks ({e})")))
        });
        return false;
    }
    task.with(|mq| {
        report.print(mq, "a Lua script answers a request", match &probe {
            Ok(value) => Check::that(value.as_str() == Some("probe"), || format!("got {value:?}")),
            Err(e) => Check::Fail(e.clone()),
        })
    });

    // Every kind of value, decoded by Lua and encoded again.
    let sent = everything();
    let echoed = ask(task, &script, request("echo").with("value", sent.clone())).await;
    task.with(|mq| {
        report.print(mq, "every Lua value type survives a trip through Lua", match &echoed {
            Ok(value) => Check::that(*value == sent, || format!("sent {sent:?}, got back {value:?}")),
            Err(e) => Check::Fail(e.clone()),
        })
    });

    // Lua sees real Lua types, not just bytes it passes along.
    let inspected = ask(task, &script, request("inspect").with("value", sent)).await;
    task.with(|mq| {
        let expected = LuaValue::from(
            LuaTable::new()
                .with("name_type", "string")
                .with("list_len", 3)
                .with("list_sum", 60)
                .with("flag_type", "boolean")
                .with("nested", "ok"),
        );
        report.print(mq, "Lua reads Rust's table as Lua types", match &inspected {
            Ok(value) => Check::that(*value == expected, || format!("Lua saw {value:?}")),
            Err(e) => Check::Fail(e.clone()),
        })
    });

    // Lua -> Rust: the script messages our "lua" mailbox and keeps our reply.
    let token = token();
    task.with_plugin(|plugin, _| plugin.lua_hello = None);
    let last = lua_calls_rust(task, &script, &me, token).await;
    let hello = task.with_plugin(|plugin, _| plugin.lua_hello.take());
    task.with(|mq| {
        report.print(mq, "a Lua script's message reaches a Rust mailbox", match &hello {
            Some((value, sender)) => Check::that(
                field(value, "from").and_then(LuaValue::as_str) == Some("lua")
                    && field(value, "token").and_then(LuaValue::as_number) == Some(token)
                    // MQ2Lua's name depends on how it was loaded ("lua" or "Lua").
                    && sender.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(&format!("{LUA_PLUGIN}:{PEER_SCRIPT}"))),
                || format!("got {value:?} from mailbox {sender:?}"),
            ),
            None => Check::Fail(match &last {
                Err(e) => e.clone(),
                Ok(_) => "nothing arrived from Lua".into(),
            }),
        });
        report.print(mq, "Rust's reply reaches the Lua script's callback", match &last {
            Ok(value) => Check::that(*value == expected_hello_back(token), || format!("Lua got {value:?}")),
            Err(e) => Check::Fail(e.clone()),
        })
    });

    // Lua reads the plugin's TLO (mq.TLO.RustTest(...)()), with types intact,
    // its datatype's members, method and text, inherited members (from spawn
    // and string) and the member it adds to spawn.
    let bumps_before = task.with_plugin(|plugin, _| plugin.counter_bumps);
    let tlo = ask(task, &script, request("tlo")).await;
    task.with_plugin(|plugin, mq| {
        let expected = LuaValue::from(
            LuaTable::new()
                .with("counter", "#5")
                .with("counter_label", "counter 5")
                .with("counter_plus", 8)
                .with("me_is_me", true)
                .with("bot_role", "tank")
                .with("bot_is_me", true)
                .with("word_length", 5)
                .with("tag_is_mine", true)
                .with("name", mq.plugin_name())
                .with("answer", 42)
                .with("half", 2.5)
                .with("yes", true)
                .with("nope_is_nil", true),
        );
        report.print(mq, "Lua reads the plugin's TLO and datatypes, and its member of spawn", match &tlo {
            Ok(value) => Check::that(*value == expected, || format!("Lua read {value:?}")),
            Err(e) => Check::Fail(e.clone()),
        });
        let bumps = plugin.counter_bumps - bumps_before;
        report.print(mq, "Lua calls the datatype's method once", Check::that(tlo.is_ok() && bumps == 1, || format!("ran {bumps} times")));
    });

    // Negative: requests Lua can't make sense of are refused, not dropped.
    let unknown = ask(task, &script, request("no_such_request")).await;
    let garbage = task.request_timeout(&script, [0xffu8], TIMEOUT).await.map_err(|e| e.to_string());
    task.with(|mq| {
        report.print(mq, "Lua refuses an unknown request", match &unknown {
            Ok(value) => Check::that(
                field(value, "error").and_then(LuaValue::as_str) == Some("unknown request")
                    && field(value, "id").and_then(LuaValue::as_str) == Some("no_such_request"),
                || format!("got {value:?}"),
            ),
            Err(e) => Check::Fail(e.clone()),
        });
        report.print(mq, "Lua survives bytes that aren't a Lua value", match &garbage {
            Ok(reply) => match reply.lua() {
                Ok(value) => Check::that(field(&value, "id").and_then(LuaValue::as_str) == Some("nil"), || {
                    format!("got {value:?}")
                }),
                Err(e) => Check::Fail(e.to_string()),
            },
            Err(e) => Check::Fail(e.clone()),
        });
    });
    true
}
