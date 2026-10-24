//! Runs the generator against the real eqlib.pdb.

use mq_emu_bindgen::{default_pdb, generate, model, output_file, parse_roots, roots_file};

fn roots() -> Vec<String> {
    parse_roots(&std::fs::read_to_string(roots_file()).unwrap())
}

fn pdb() -> std::path::PathBuf {
    default_pdb().expect("MQ_ROOT must be set to run the generator tests")
}

#[test]
fn committed_layouts_are_up_to_date() {
    let generated = generate(&pdb(), &roots()).unwrap();
    let committed = std::fs::read_to_string(output_file()).unwrap();
    assert!(
        generated == committed,
        "{} is out of date with layouts.txt / eqlib.pdb; run `cargo run -p mq-emu-bindgen`",
        output_file().display()
    );
}

#[test]
fn generation_is_deterministic() {
    assert_eq!(generate(&pdb(), &roots()).unwrap(), generate(&pdb(), &roots()).unwrap());
}

#[test]
fn by_value_types_are_generated_and_pointer_only_types_are_opaque() {
    let pdb = pdb();
    let model = model::build(&pdb.to_string_lossy(), &["PlayerClient"]).unwrap();
    let names: Vec<&str> = model.records.values().map(|r| r.cpp_name.as_str()).collect();
    // PlayerClient contains CastingData (LaunchSpellData) by value...
    assert!(names.contains(&"LaunchSpellData"), "{names:?}");
    // ...but only points at other spawns' owners, groups, etc.
    assert!(!model.opaque.is_empty());
    for id in model.opaque.keys() {
        assert!(!model.records.contains_key(id), "type {id} is both generated and opaque");
    }
}

#[test]
fn unknown_root_is_an_error() {
    let err = generate(&pdb(), &["NoSuchClassAnywhere".to_owned()]).unwrap_err();
    assert!(err.to_string().contains("`NoSuchClassAnywhere` not found"), "{err}");
}

#[test]
fn enum_root_is_an_error() {
    let err = generate(&pdb(), &["EActorType".to_owned()]).unwrap_err();
    assert!(err.to_string().contains("is not a class, struct or union"), "{err}");
}

#[test]
fn missing_pdb_is_an_error() {
    let err = generate(std::path::Path::new("C:/definitely/not/here/eqlib.pdb"), &roots()).unwrap_err();
    assert!(err.to_string().contains("eqlib.pdb"), "{err}");
}
