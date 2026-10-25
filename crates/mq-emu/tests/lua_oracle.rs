//! mq-emu's hand-written Lua message encoding against real C++ protobuf,
//! compiled from MacroQuest's own generated `Actor.pb.cc` (the code MQ2Lua
//! uses). If these agree, Lua scripts and Rust plugins understand each other.

use lua_proto_oracle as cpp;
use mq_emu::lua::{DecodeError, LuaTable, LuaValue};

/// `text` is the expected message in protobuf text format; `value` the same
/// thing as a LuaValue.
fn check(text: &str, value: LuaValue) {
    let cpp_bytes = cpp::encode_text(text).unwrap_or_else(|| panic!("C++ couldn't parse the text {text:?}"));

    // Rust decodes what C++ (i.e. a Lua script) sends.
    assert_eq!(LuaValue::decode(&cpp_bytes), Ok(value.clone()), "decoding C++'s {text:?}");

    // C++ reads what Rust sends as the same message.
    let rust_bytes = value.encode();
    let rust_as_cpp = cpp::decode_to_text(&rust_bytes).unwrap_or_else(|| panic!("C++ rejected Rust's encoding of {value:?}"));
    assert_eq!(rust_as_cpp, cpp::decode_to_text(&cpp_bytes).unwrap(), "for {value:?}");

    // Whatever field and map order C++ writes, Rust reads it back.
    let reencoded = cpp::reencode(&rust_bytes).unwrap();
    assert_eq!(LuaValue::decode(&reencoded), Ok(value), "after a C++ round trip of {text:?}");
}

#[test]
fn scalars_match_cpp() {
    check("", LuaValue::Nil);
    check("number: 1.5", LuaValue::Number(1.5));
    check("number: -65", LuaValue::Number(-65.0));
    check("number: 0", LuaValue::Number(0.0));
    check("boolean: true", LuaValue::Boolean(true));
    check("boolean: false", LuaValue::Boolean(false));
    check(r#"str: "hello""#, LuaValue::from("hello"));
    check(r#"str: """#, LuaValue::from(""));
    check(r#"str: "caf\303\251 \360\237\220\211""#, LuaValue::from("café 🐉"));
    check("imvec2 { x: 1.5 y: -2 }", LuaValue::Vec2([1.5, -2.0]));
    check("imvec4 { x: 1 y: 2 z: 3 w: 4 }", LuaValue::Vec4([1.0, 2.0, 3.0, 4.0]));
}

#[test]
fn tables_match_cpp() {
    check("table { }", LuaValue::from(LuaTable::new()));
    check(
        r#"table { entries { key: "id" value { str: "ping" } } entries { key: "level" value { number: 65 } } }"#,
        LuaValue::from(LuaTable::new().with("id", "ping").with("level", 65)),
    );
    check(
        r#"table { arr { key: 1 value { str: "a" } } arr { key: 2 value { boolean: true } } }"#,
        LuaValue::from(["a".into(), LuaValue::Boolean(true)].into_iter().collect::<LuaTable>()),
    );
    // Mixed keys and nesting, as in MacroQuest's buffbeg.lua example.
    let mut buffs = LuaTable::new();
    buffs.push("Aegolism");
    buffs.push("Spirit of Wolf");
    check(
        r#"table {
            entries { key: "id" value { str: "announce" } }
            entries { key: "buffs" value { table { arr { key: 1 value { str: "Aegolism" } } arr { key: 2 value { str: "Spirit of Wolf" } } } } }
            entries { key: "pos" value { imvec2 { x: 1 y: 2 } } }
            arr { key: 1 value { number: 3 } }
        }"#,
        LuaValue::from({
            let mut table = LuaTable::new().with("id", "announce").with("buffs", buffs).with("pos", LuaValue::Vec2([1.0, 2.0]));
            table.push(3);
            table
        }),
    );
}

#[test]
fn large_array_indices_match_cpp() {
    // Indices >= 128 need multi-byte varints.
    let mut table = LuaTable::new();
    table.items.insert(1, LuaValue::Number(1.0));
    table.items.insert(300, LuaValue::Number(300.0));
    table.items.insert(70_000, LuaValue::Number(7.0));
    check(
        "table { arr { key: 1 value { number: 1 } } arr { key: 300 value { number: 300 } } arr { key: 70000 value { number: 7 } } }",
        LuaValue::from(table),
    );
}

#[test]
fn a_lua_nil_value_inside_a_table_matches_cpp() {
    // An entry with an empty Variant (C++ writes the value field, empty).
    check(r#"table { entries { key: "gone" value { } } }"#, LuaValue::from(LuaTable::new().with("gone", LuaValue::Nil)));
}

// Negative tests: both sides reject the same bad input.

#[test]
fn truncated_messages_are_rejected_by_both() {
    let full = cpp::encode_text(r#"table { entries { key: "id" value { str: "ping" } } }"#).unwrap();
    for cut in 1..full.len() {
        let partial = &full[..cut];
        assert!(cpp::decode_to_text(partial).is_none(), "C++ accepted a cut at {cut}");
        assert!(LuaValue::decode(partial).is_err(), "Rust accepted a cut at {cut}");
    }
}

#[test]
fn invalid_utf8_strings_are_rejected_by_both() {
    // str (field 8) holding the byte 0xff.
    let bytes = [0x42, 0x01, 0xff];
    assert!(cpp::decode_to_text(&bytes).is_none(), "C++ accepted invalid UTF-8");
    assert_eq!(LuaValue::decode(&bytes), Err(DecodeError::Malformed("string is not UTF-8")));
}

#[test]
fn the_oracle_itself_rejects_bad_text() {
    assert!(cpp::encode_text("not a field: 1").is_none());
}
