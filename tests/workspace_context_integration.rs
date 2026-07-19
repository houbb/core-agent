use std::fs;

use core_agent::integrations::{environment_context, workspace_context};
use core_agent::{WorkspaceManager, WorkspaceOpenRequest, WorkspaceState};
use tempfile::tempdir;

#[tokio::test]
async fn workspace_runtime_populates_existing_context_contracts() {
    let directory = tempdir().unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname='adapter'",
    )
    .unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(directory.path().join("src/lib.rs"), "pub fn adapter() {}").unwrap();

    let manager = WorkspaceManager::builder().build();
    let workspace = manager
        .open(WorkspaceOpenRequest::local("adapter", directory.path()).unwrap())
        .await
        .unwrap();
    assert_eq!(workspace.state, WorkspaceState::Ready);

    let workspace_context = workspace_context(&workspace);
    assert!(workspace_context.enabled);
    assert!(workspace_context.root_path.is_some());
    assert_eq!(
        workspace_context.content["resource_count"],
        workspace.resources.len()
    );
    assert_eq!(workspace_context.content["projects"][0]["kind"], "RUST");

    let environment_context = environment_context(&workspace);
    assert_eq!(
        environment_context.os.as_deref(),
        Some(std::env::consts::OS)
    );
    assert_eq!(environment_context.extra["runtimes"][0], "rust");
}
