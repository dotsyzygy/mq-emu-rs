use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=oracle.cpp");
    println!("cargo::rerun-if-env-changed=MQ_ROOT");

    let mq_root = PathBuf::from(env::var("MQ_ROOT").expect("MQ_ROOT must point at a built MacroQuest checkout"));
    let lua = mq_root.join("src/plugins/lua");
    let vcpkg = mq_root.join("contrib/vcpkg/installed/x86-windows-static");
    for needed in [lua.join("Actor.pb.cc"), vcpkg.join("lib/libprotobuf.lib")] {
        assert!(needed.is_file(), "{} not found; build MacroQuest (incl. MQ2Lua) first", needed.display());
        println!("cargo::rerun-if-changed={}", needed.display());
    }

    cc::Build::new()
        .cpp(true)
        .files([lua.join("Actor.pb.cc"), PathBuf::from("oracle.cpp")])
        .include(&lua)
        .include(vcpkg.join("include"))
        .flag("/std:c++17")
        .flag("/EHsc")
        .warnings(false)
        .compile("lua_proto_oracle");

    println!("cargo::rustc-link-search=native={}", vcpkg.join("lib").display());
    println!("cargo::rustc-link-lib=static=libprotobuf");
}
