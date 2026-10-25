//! The MIT license requires its notice to ship with the code, so each published
//! crate carries a copy of the repository's LICENSE. Keep the copies in sync.

use std::path::Path;

#[test]
fn crate_license_matches_repository_license() {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ours = std::fs::read_to_string(crate_dir.join("LICENSE")).expect("crate is missing its LICENSE copy");

    // Absent when testing an unpacked .crate outside the repository.
    let Ok(root) = std::fs::read_to_string(crate_dir.join("../../LICENSE")) else {
        return;
    };
    assert_eq!(ours, root, "crates/mq-emu/LICENSE is out of date; copy the root LICENSE over it");
}

#[test]
fn license_is_mit() {
    let license = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("LICENSE")).unwrap();
    assert!(license.starts_with("MIT License"), "LICENSE is not the MIT license");
    assert_eq!(env!("CARGO_PKG_LICENSE"), "MIT", "Cargo.toml license field disagrees with LICENSE");
}
