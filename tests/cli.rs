use assert_cmd::Command;
use predicates::prelude::*;

fn csb() -> Command {
    Command::cargo_bin("csb").unwrap()
}

#[test]
fn cli_should_print_version() {
    csb()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(format!(
            "csb {}",
            env!("CARGO_PKG_VERSION")
        )));
}

#[test]
fn cli_should_print_help() {
    csb()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Sandboxed Claude Code"));
}

#[test]
fn cli_should_print_help_for_build_subcommand() {
    csb()
        .args(["build", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--force"));
}

#[test]
fn cli_should_print_help_for_doctor_subcommand() {
    csb()
        .args(["help", "doctor"])
        .assert()
        .success()
        .stdout(predicate::str::contains("diagnostics"));
}

// A bad flag on a real subcommand must fail loudly. It previously fell through to
// launching a full Claude session with the typo forwarded as an argument.
#[test]
fn cli_should_fail_on_unknown_flag_for_doctor() {
    csb()
        .args(["doctor", "--bogus"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--bogus"));
}

#[test]
fn cli_should_fail_on_misspelled_build_flag() {
    csb()
        .args(["build", "--forc"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--forc"));
}
