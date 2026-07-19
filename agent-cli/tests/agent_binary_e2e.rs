use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn agent_init_binary_creates_project_configuration() {
    let directory = tempdir().unwrap();
    Command::cargo_bin("agent")
        .unwrap()
        .args(["--workspace", directory.path().to_str().unwrap(), "init"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));
    assert!(directory.path().join(".agent/config.yaml").is_file());

    Command::cargo_bin("agent")
        .unwrap()
        .args(["--workspace", directory.path().to_str().unwrap(), "init"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}
