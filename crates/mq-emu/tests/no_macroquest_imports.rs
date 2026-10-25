//! A program that uses mq-emu without being a plugin (a tool, or a test like
//! this one) must start without the game: it may not import MQ2Main.dll,
//! eqlib.dll or imgui.dll. Code that calls MacroQuest is generic over the plugin type for
//! this reason (see src/host.rs); a plain function calling it could land in
//! the same object file as, say, the Lua codec and be linked in with it.

use object::LittleEndian;
use object::read::pe::PeFile32;

use mq_emu::{Address, LuaTable, LuaValue, Value};

fn imported_dlls() -> Vec<String> {
    let exe = std::fs::read(std::env::current_exe().unwrap()).unwrap();
    let file = PeFile32::parse(&*exe).unwrap();
    let imports = file.import_table().unwrap().unwrap();
    let mut descriptors = imports.descriptors().unwrap();
    let mut names = Vec::new();
    while let Some(descriptor) = descriptors.next().unwrap() {
        let name = imports.name(descriptor.name.get(LittleEndian)).unwrap();
        names.push(String::from_utf8_lossy(name).to_ascii_lowercase());
    }
    names
}

#[test]
fn using_mq_emu_outside_a_plugin_does_not_import_macroquest() {
    // Touch the non-generic API, so its object files are linked in.
    let table = LuaValue::from(LuaTable::new().with("id", "ping"));
    assert_eq!(LuaValue::decode(&table.encode()), Ok(table));
    assert_eq!(Address::lua_script("x").to_string(), "mailbox=lua:x");
    assert_eq!(Value::Float(1.5).to_string(), "1.50");

    let dlls = imported_dlls();
    assert!(dlls.contains(&"kernel32.dll".to_owned()), "the import table was read: {dlls:?}");
    for dll in ["mq2main.dll", "eqlib.dll", "imgui.dll"] {
        assert!(!dlls.contains(&dll.to_owned()), "imports {dll}: {dlls:?}");
    }
}
