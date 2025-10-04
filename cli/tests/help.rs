use assert_cmd::prelude::*;
use std::process::Command;

#[test]
fn shows_help() {
    let mut cmd = Command::cargo_bin("skelz").unwrap();
    cmd.arg("--help");
    cmd.assert().success();
}
