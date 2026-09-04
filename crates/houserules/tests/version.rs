//! Integration test for the `houserules --version` flag: the crate's first
//! behavior (HR-054).

use std::process::Command;

/// `houserules --version` prints `houserules <CARGO_PKG_VERSION>` to stdout
/// and exits 0, matching clap's standard `--version` output.
#[test]
fn version_flag_prints_binary_name_and_crate_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_houserules"))
        .arg("--version")
        .output()
        .expect("failed to run the houserules binary");

    let stdout = String::from_utf8(output.stdout).expect("stdout was not valid UTF-8");
    let expected = format!("houserules {}\n", env!("CARGO_PKG_VERSION"));

    assert_eq!(stdout, expected);
    assert!(output.status.success());
}
