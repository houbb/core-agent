use std::time::Duration;

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
#[ignore = "requires a reachable provider configured in ~/core-agent/core-agent-config.yaml"]
fn terminal_uses_global_config_mentions_without_project_initialization() {
    let workspace = tempfile::tempdir().unwrap();
    let marker = format!("GLOBAL_TERMINAL_{}", uuid::Uuid::new_v4().simple());
    std::fs::write(workspace.path().join("terminal-marker.txt"), &marker).unwrap();

    Command::cargo_bin("agent")
        .unwrap()
        .timeout(Duration::from_secs(180))
        .args([
            "--workspace",
            workspace.path().to_str().unwrap(),
            "run",
            "Read @terminal-marker.txt and return only its exact marker value.",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(&marker));

    assert!(!workspace.path().join(".agent/config.yaml").exists());
}
