//! `taskman --licenses` prints LICENSE-THIRD-PARTY from the binary itself: an installed copy has
//! no archive beside it, so the notices of the code in it must come from the binary.

mod common;

use std::process::Stdio;

#[test]
fn licenses_prints_the_third_party_notices() {
    let dir = common::scratch_dir("licenses");
    let out = common::taskman(&dir).arg("--licenses").stdin(Stdio::null()).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let file = concat!(env!("CARGO_MANIFEST_DIR"), "/LICENSE-THIRD-PARTY");
    let notices = std::fs::read_to_string(file).unwrap();
    assert!(notices.contains("\nUsed by:\n"), "the notices file lists no crates");
    assert_eq!(String::from_utf8(out.stdout).unwrap(), notices);

    let help = common::taskman(&dir).arg("--help").stdin(Stdio::null()).output().unwrap();
    assert!(String::from_utf8_lossy(&help.stdout).contains("taskman --licenses"));
}
