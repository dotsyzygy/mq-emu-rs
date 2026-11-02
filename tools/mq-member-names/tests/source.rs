//! Runs the generator against MacroQuest's real source.

use mq_member_names::{default_root, generate, output_file, scan};

fn root() -> std::path::PathBuf {
    default_root().expect("MQ_ROOT must be set to run the generator tests")
}

#[test]
fn committed_names_are_up_to_date() {
    let generated = generate(&root()).unwrap();
    let committed = std::fs::read_to_string(output_file()).unwrap();
    assert!(generated == committed, "{} is out of date with MacroQuest's source; run `cargo run -p mq-member-names`", output_file().display());
}

#[test]
fn generation_is_deterministic() {
    assert_eq!(generate(&root()).unwrap(), generate(&root()).unwrap());
}

#[test]
fn finds_members_methods_and_second_names() {
    let names = scan(&root()).unwrap();
    assert!(names.members.len() > 1000, "{} members", names.members.len());
    for name in ["PctHPs", "Buff", "Duration", "State", "ScreenMode", "SongCap"] {
        assert!(names.members.contains(name), "{name} missing");
    }
    assert!(names.methods.contains("Remove"), "{:?}", names.methods);
}

#[test]
fn missing_source_is_an_error() {
    let err = scan(std::path::Path::new("C:/definitely/not/here")).unwrap_err();
    assert!(err.to_string().contains("src/main/datatypes"), "{err}");
}
