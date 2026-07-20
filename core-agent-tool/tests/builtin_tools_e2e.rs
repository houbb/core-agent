use std::collections::BTreeMap;
use std::sync::Arc;

use core_agent_tool::{
    BuiltinToolProvider, PermissionDecision, SqliteToolStore, ToolCapability, ToolCatalog,
    ToolManager, ToolPermissionRule, ToolPermissionStore, ToolProviderDefinition, ToolProviderKind,
    ToolRequest, ToolResult, ToolLifecycleStatus,
};
use tempfile::tempdir;

fn setup_manager() -> (ToolManager, Arc<SqliteToolStore>) {
    let store = Arc::new(SqliteToolStore::new(":memory:").unwrap());
    let manager = ToolManager::builder()
        .catalog(store.clone())
        .permission(store.clone())
        .lifecycle(store.clone())
        .build();
    (manager, store)
}

async fn allow_all(store: &SqliteToolStore, manager: &ToolManager) {
    // Allow all tools
    for key in manager.list().await.unwrap() {
        let rule = ToolPermissionRule::for_tool(&key.key, PermissionDecision::Allow);
        store.upsert_permission(&rule).await.unwrap();
    }
}

#[tokio::test]
async fn builtin_provider_registers_all_44_tools() {
    let (manager, _store) = setup_manager();
    let count = manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    assert_eq!(count, 44, "Should register exactly 44 builtin tools");

    let tools = manager.list().await.unwrap();
    assert_eq!(tools.len(), 44, "Catalog should have 44 tools");

    // Verify provider was stored
    let provider = _store.find_provider("builtin").await.unwrap().unwrap();
    assert_eq!(provider.kind, ToolProviderKind::Builtin);
    assert!(provider.enabled);
}

#[tokio::test]
async fn tools_have_correct_capabilities() {
    let (manager, _store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();

    // By category
    let file_tools = manager.find_by_capability(
        &ToolCapability::new("file").unwrap(), true
    ).await.unwrap();
    assert_eq!(file_tools.len(), 11, "Should have 11 file tools");

    let shell_tools = manager.find_by_capability(
        &ToolCapability::new("shell").unwrap(), true
    ).await.unwrap();
    assert_eq!(shell_tools.len(), 3, "Should have 3 shell tools");

    let git_tools = manager.find_by_capability(
        &ToolCapability::new("git").unwrap(), true
    ).await.unwrap();
    assert_eq!(git_tools.len(), 7, "Should have 7 git tools");

    let ask_tools = manager.find_by_capability(
        &ToolCapability::new("ask").unwrap(), true
    ).await.unwrap();
    assert_eq!(ask_tools.len(), 3, "Should have 3 ask tools");

    let todo_tools = manager.find_by_capability(
        &ToolCapability::new("todo").unwrap(), true
    ).await.unwrap();
    assert_eq!(todo_tools.len(), 3, "Should have 3 todo tools");

    let agent_tools = manager.find_by_capability(
        &ToolCapability::new("agent").unwrap(), true
    ).await.unwrap();
    assert_eq!(agent_tools.len(), 3, "Should have 3 agent tools");

    let plan_tools = manager.find_by_capability(
        &ToolCapability::new("plan").unwrap(), true
    ).await.unwrap();
    assert_eq!(plan_tools.len(), 3, "Should have 3 plan tools");

    let cron_tools = manager.find_by_capability(
        &ToolCapability::new("cron").unwrap(), true
    ).await.unwrap();
    assert_eq!(cron_tools.len(), 3, "Should have 3 cron tools");

    let lsp_tools = manager.find_by_capability(
        &ToolCapability::new("lsp").unwrap(), true
    ).await.unwrap();
    assert_eq!(lsp_tools.len(), 6, "Should have 6 lsp tools");

    // By specific capability
    let read_tools = manager.find_by_capability(
        &ToolCapability::new("file.read").unwrap(), false
    ).await.unwrap();
    assert_eq!(read_tools.len(), 1, "Should have 1 file.read tool");
}

#[tokio::test]
async fn file_read_tool_works_through_manager() {
    let (manager, store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    allow_all(&store, &manager).await;

    let dir = tempdir().unwrap();
    let path = dir.path().join("test_read.txt");
    tokio::fs::write(&path, "hello from e2e test").await.unwrap();

    let result = manager.execute(ToolRequest::new(
        "builtin/file.read@1.0.0",
        serde_json::json!({"path": path.to_string_lossy()}),
    )).await.unwrap();

    assert_eq!(result.status, ToolLifecycleStatus::Success);
    let text = extract_text(&result);
    assert_eq!(text, "hello from e2e test");
}

#[tokio::test]
async fn file_write_tool_works_through_manager() {
    let (manager, store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    allow_all(&store, &manager).await;

    let dir = tempdir().unwrap();
    let path = dir.path().join("test_write.txt");

    let result = manager.execute(ToolRequest::new(
        "builtin/file.write@1.0.0",
        serde_json::json!({"path": path.to_string_lossy(), "content": "written by e2e"}),
    )).await.unwrap();

    assert_eq!(result.status, ToolLifecycleStatus::Success);
    let content = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(content, "written by e2e");
}

#[tokio::test]
async fn file_edit_tool_works_through_manager() {
    let (manager, store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    allow_all(&store, &manager).await;

    let dir = tempdir().unwrap();
    let path = dir.path().join("test_edit.txt");
    tokio::fs::write(&path, "Hello {name}").await.unwrap();

    let result = manager.execute(ToolRequest::new(
        "builtin/file.edit@1.0.0",
        serde_json::json!({
            "path": path.to_string_lossy(),
            "old_string": "{name}",
            "new_string": "E2E"
        }),
    )).await.unwrap();

    assert_eq!(result.status, ToolLifecycleStatus::Success);
    let content = tokio::fs::read_to_string(&path).await.unwrap();
    assert_eq!(content, "Hello E2E");
}

#[tokio::test]
async fn file_patch_tool_works_through_manager() {
    let (manager, store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    allow_all(&store, &manager).await;

    let dir = tempdir().unwrap();
    let path1 = dir.path().join("patch1.txt");
    let path2 = dir.path().join("patch2.txt");
    tokio::fs::write(&path1, "File One").await.unwrap();
    tokio::fs::write(&path2, "File Two").await.unwrap();

    let result = manager.execute(ToolRequest::new(
        "builtin/file.patch@1.0.0",
        serde_json::json!({
            "patches": [
                {"path": path1.to_string_lossy(), "old_string": "One", "new_string": "1"},
                {"path": path2.to_string_lossy(), "old_string": "Two", "new_string": "2"}
            ]
        }),
    )).await.unwrap();

    assert_eq!(result.status, ToolLifecycleStatus::Success);
    assert_eq!(tokio::fs::read_to_string(&path1).await.unwrap(), "File 1");
    assert_eq!(tokio::fs::read_to_string(&path2).await.unwrap(), "File 2");
}

#[tokio::test]
async fn file_glob_tool_works_through_manager() {
    let (manager, store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    allow_all(&store, &manager).await;

    let dir = tempdir().unwrap();
    tokio::fs::write(dir.path().join("a.rs"), "").await.unwrap();
    tokio::fs::write(dir.path().join("b.rs"), "").await.unwrap();

    let result = manager.execute(ToolRequest::new(
        "builtin/file.glob@1.0.0",
        serde_json::json!({"pattern": format!("{}/*.rs", dir.path().to_string_lossy())}),
    )).await.unwrap();

    assert_eq!(result.status, ToolLifecycleStatus::Success);
    let text = extract_text(&result);
    assert!(text.contains("2 file(s)"));
}

#[tokio::test]
async fn file_grep_tool_works_through_manager() {
    let (manager, store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    allow_all(&store, &manager).await;

    let dir = tempdir().unwrap();
    tokio::fs::write(dir.path().join("search.txt"), "hello world\nfind me").await.unwrap();

    let result = manager.execute(ToolRequest::new(
        "builtin/file.grep@1.0.0",
        serde_json::json!({"pattern": "find", "path": dir.path().to_string_lossy()}),
    )).await.unwrap();

    assert_eq!(result.status, ToolLifecycleStatus::Success);
    let text = extract_text(&result);
    assert!(text.contains("find me"));
}

#[tokio::test]
async fn file_delete_tool_works_through_manager() {
    let (manager, store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    allow_all(&store, &manager).await;

    let dir = tempdir().unwrap();
    let path = dir.path().join("delete_me.txt");
    tokio::fs::write(&path, "delete me").await.unwrap();

    let result = manager.execute(ToolRequest::new(
        "builtin/file.delete@1.0.0",
        serde_json::json!({"path": path.to_string_lossy()}),
    )).await.unwrap();

    assert_eq!(result.status, ToolLifecycleStatus::Success);
    assert!(!path.exists());
}

#[tokio::test]
async fn file_list_tool_works_through_manager() {
    let (manager, store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    allow_all(&store, &manager).await;

    let dir = tempdir().unwrap();
    tokio::fs::write(dir.path().join("list_test.txt"), "data").await.unwrap();

    let result = manager.execute(ToolRequest::new(
        "builtin/file.list@1.0.0",
        serde_json::json!({"path": dir.path().to_string_lossy()}),
    )).await.unwrap();

    assert_eq!(result.status, ToolLifecycleStatus::Success);
    let text = extract_text(&result);
    assert!(text.contains("list_test.txt"));
}

#[tokio::test]
async fn shell_exec_tool_works_through_manager() {
    let (manager, store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    allow_all(&store, &manager).await;

    let result = manager.execute(ToolRequest::new(
        "builtin/shell.exec@1.0.0",
        serde_json::json!({"command": "echo e2e_test_pass"}),
    )).await.unwrap();

    assert_eq!(result.status, ToolLifecycleStatus::Success);
    let text = extract_text(&result);
    assert!(text.contains("e2e_test_pass"));
}

#[tokio::test]
async fn git_status_tool_works_through_manager() {
    let (manager, store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    allow_all(&store, &manager).await;

    let dir = tempdir().unwrap();
    std::process::Command::new("git")
        .args(["init", "--initial-branch=main"])
        .current_dir(dir.path())
        .output().unwrap();

    let result = manager.execute(ToolRequest::new(
        "builtin/git.status@1.0.0",
        serde_json::json!({"path": dir.path().to_string_lossy()}),
    )).await.unwrap();

    assert_eq!(result.status, ToolLifecycleStatus::Success);
    let text = extract_text(&result);
    assert!(text.contains("main"));
}

#[tokio::test]
async fn ask_tools_work_through_manager() {
    let (manager, store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();
    allow_all(&store, &manager).await;

    // ask.user
    let result = manager.execute(ToolRequest::new(
        "builtin/ask.user@1.0.0",
        serde_json::json!({"question": "How are you?"}),
    )).await.unwrap();
    assert_eq!(result.status, ToolLifecycleStatus::Success);
    assert!(extract_text(&result).contains("How are you?"));

    // ask.confirm
    let result = manager.execute(ToolRequest::new(
        "builtin/ask.confirm@1.0.0",
        serde_json::json!({"message": "Proceed?"}),
    )).await.unwrap();
    assert_eq!(result.status, ToolLifecycleStatus::Success);
    assert!(extract_text(&result).contains("Proceed?"));

    // ask.select
    let result = manager.execute(ToolRequest::new(
        "builtin/ask.select@1.0.0",
        serde_json::json!({"question": "Pick", "options": ["A", "B"]}),
    )).await.unwrap();
    assert_eq!(result.status, ToolLifecycleStatus::Success);
    assert!(extract_text(&result).contains("A"));
}

#[tokio::test]
async fn permission_denied_if_not_configured() {
    let (manager, _store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();

    // shell.exec has Ask by default, and no permission rule configured
    let result = manager.execute(ToolRequest::new(
        "builtin/shell.exec@1.0.0",
        serde_json::json!({"command": "echo test"}),
    )).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(err_str.contains("APPROVAL_REQUIRED") || err_str.contains("approval"));
}

#[tokio::test]
async fn capability_based_discovery_from_agent_perspective() {
    let (manager, _store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();

    // Agent asks: "Find tools that can read files"
    let read_tools = manager.find_by_capability(
        &ToolCapability::new("file.read").unwrap(), false
    ).await.unwrap();
    assert_eq!(read_tools.len(), 1);
    assert_eq!(read_tools[0].name, "file.read");

    // Agent asks: "Find tools that can search"
    let search_tools = manager.find_by_capability(
        &ToolCapability::new("search").unwrap(), false
    ).await.unwrap();
    assert_eq!(search_tools.len(), 1);
    assert_eq!(search_tools[0].name, "file.grep");

    // Agent asks: "Find all file tools"
    let all_file_tools = manager.find_by_capability(
        &ToolCapability::new("file").unwrap(), true
    ).await.unwrap();
    assert!(all_file_tools.len() >= 11);
}

#[tokio::test]
async fn tool_definition_schema_validation() {
    let (manager, _store) = setup_manager();
    manager.load_provider(&BuiltinToolProvider::new()).await.unwrap();

    for tool in manager.list().await.unwrap() {
        // All tools must have valid JSON Schema
        assert!(tool.input_schema.is_object(), "Tool {} has invalid schema", tool.name);
        // All tools must have at least one capability
        assert!(!tool.capabilities.is_empty(), "Tool {} has no capabilities", tool.name);
        // All tools must have a description
        assert!(!tool.description.is_empty(), "Tool {} has no description", tool.name);
        // All tools must have a valid timeout
        assert!(tool.timeout_ms > 0, "Tool {} has zero timeout", tool.name);
        // All tools must have a category
        assert!(!tool.category.is_empty(), "Tool {} has no category", tool.name);
    }
}

fn extract_text(result: &ToolResult) -> String {
    match &result.content[0] {
        core_agent_tool::ToolContent::Text(t) => t.clone(),
        _ => panic!("expected text content"),
    }
}