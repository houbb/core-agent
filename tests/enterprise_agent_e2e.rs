use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use core_agent::{
    ContentPart, EnterpriseAgent, EnterpriseAgentConfig, EnterpriseModelConfig, FinishReason,
    FunctionTool, InMemoryModelCatalog, ModelCapability, ModelCatalog, ModelError, ModelManager,
    ModelManagerBuilder, ModelProfile, ModelProvider, ModelRequest, ModelResponse, ModelRole,
    ModelUsage, PermissionDecision, PermissionMode, ProviderDefinition, RawToolOutput,
    StaticToolProvider, ToolContent, ToolDefinition, ToolProviderDefinition, ToolProviderKind,
    ToolRegistration,
};

struct AllowOnce;

#[async_trait]
impl core_agent::EnterpriseApprovalHandler for AllowOnce {
    async fn decide(
        &self,
        _request: &core_agent::EnterpriseApprovalRequest,
    ) -> core_agent::EnterpriseApprovalDecision {
        core_agent::EnterpriseApprovalDecision::AllowOnce
    }
}

struct DeterministicProvider;

#[async_trait]
impl ModelProvider for DeterministicProvider {
    fn key(&self) -> &str {
        "deterministic"
    }

    async fn invoke(
        &self,
        request: &ModelRequest,
        profile: &ModelProfile,
    ) -> Result<ModelResponse, ModelError> {
        assert!(request
            .messages
            .iter()
            .any(|message| message.content == vec![ContentPart::text("inspect workspace")]));
        assert!(request.tools.iter().any(|tool| tool.name == "list_files"));
        assert!(request.tools.iter().any(|tool| tool.name == "read_file"));
        let has_tool_result = request
            .messages
            .iter()
            .any(|message| message.role == ModelRole::Tool);
        Ok(ModelResponse {
            request_id: request.id,
            provider: "deterministic".into(),
            model: profile.model.clone(),
            profile: profile.key.clone(),
            content: vec![ContentPart::text("Workspace inspection completed.")],
            tool_calls: (!has_tool_result)
                .then(|| core_agent::ToolCallRequest {
                    id: "call-1".into(),
                    name: "echo".into(),
                    arguments: serde_json::json!({"message": "checked"}),
                })
                .into_iter()
                .collect(),
            usage: ModelUsage {
                prompt_tokens: 12,
                completion_tokens: 4,
                total_tokens: 16,
                ..Default::default()
            },
            finish_reason: if has_tool_result {
                FinishReason::Stop
            } else {
                FinishReason::ToolCall
            },
            metadata: BTreeMap::new(),
            raw_response: None,
        })
    }
}

struct WorkspaceWriteProvider;

#[async_trait]
impl ModelProvider for WorkspaceWriteProvider {
    fn key(&self) -> &str {
        "workspace-writer"
    }

    async fn invoke(
        &self,
        request: &ModelRequest,
        profile: &ModelProfile,
    ) -> Result<ModelResponse, ModelError> {
        let has_tool_result = request
            .messages
            .iter()
            .any(|message| message.role == ModelRole::Tool);
        Ok(ModelResponse {
            request_id: request.id,
            provider: self.key().into(),
            model: profile.model.clone(),
            profile: profile.key.clone(),
            content: vec![ContentPart::text(if has_tool_result {
                "Workspace edit completed."
            } else {
                "Creating the requested workspace file."
            })],
            tool_calls: (!has_tool_result)
                .then(|| core_agent::ToolCallRequest {
                    id: "write-call-1".into(),
                    name: "write_file".into(),
                    arguments: serde_json::json!({
                        "path": "agent-created.txt",
                        "content": "created through governed tool loop\n"
                    }),
                })
                .into_iter()
                .collect(),
            usage: ModelUsage::default(),
            finish_reason: if has_tool_result {
                FinishReason::Stop
            } else {
                FinishReason::ToolCall
            },
            metadata: BTreeMap::new(),
            raw_response: None,
        })
    }
}

struct InteractionProvider {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl ModelProvider for InteractionProvider {
    fn key(&self) -> &str {
        "interaction"
    }

    async fn invoke(
        &self,
        request: &ModelRequest,
        profile: &ModelProfile,
    ) -> Result<ModelResponse, ModelError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let prompt = format!("{:?}", request.messages);
        assert!(prompt.contains("FILE_CONTEXT_MARKER"));
        assert!(prompt.contains("FOLDER_CONTEXT_MARKER"));
        assert!(!request.tools.iter().any(|tool| tool.name == "write_file"));
        assert!(request.tools.iter().any(|tool| tool.name == "read_file"));
        Ok(ModelResponse {
            request_id: request.id,
            provider: self.key().into(),
            model: profile.model.clone(),
            profile: profile.key.clone(),
            content: vec![ContentPart::text("Unified interaction completed.")],
            tool_calls: Vec::new(),
            usage: ModelUsage::default(),
            finish_reason: FinishReason::Stop,
            metadata: BTreeMap::new(),
            raw_response: None,
        })
    }
}

async fn deterministic_model() -> Arc<ModelManager> {
    let catalog = Arc::new(InMemoryModelCatalog::default());
    catalog
        .upsert_provider(&ProviderDefinition::new("deterministic", "Deterministic"))
        .await
        .unwrap();
    catalog
        .upsert_profile(
            &ModelProfile::new("default", "deterministic", "test-model")
                .with_capability(ModelCapability::Chat),
        )
        .await
        .unwrap();
    Arc::new(
        ModelManagerBuilder::new(catalog)
            .add_provider(Arc::new(DeterministicProvider))
            .build()
            .unwrap(),
    )
}

async fn workspace_write_model() -> Arc<ModelManager> {
    let catalog = Arc::new(InMemoryModelCatalog::default());
    catalog
        .upsert_provider(&ProviderDefinition::new(
            "workspace-writer",
            "Workspace Writer",
        ))
        .await
        .unwrap();
    catalog
        .upsert_profile(
            &ModelProfile::new("default", "workspace-writer", "test-model")
                .with_capability(ModelCapability::Chat),
        )
        .await
        .unwrap();
    Arc::new(
        ModelManagerBuilder::new(catalog)
            .add_provider(Arc::new(WorkspaceWriteProvider))
            .build()
            .unwrap(),
    )
}

async fn interaction_model(calls: Arc<AtomicUsize>) -> Arc<ModelManager> {
    let catalog = Arc::new(InMemoryModelCatalog::default());
    catalog
        .upsert_provider(&ProviderDefinition::new("interaction", "Interaction"))
        .await
        .unwrap();
    catalog
        .upsert_profile(
            &ModelProfile::new("default", "interaction", "test-model")
                .with_capability(ModelCapability::Chat),
        )
        .await
        .unwrap();
    Arc::new(
        ModelManagerBuilder::new(catalog)
            .add_provider(Arc::new(InteractionProvider { calls }))
            .build()
            .unwrap(),
    )
}

#[tokio::test]
async fn terminal_and_desktop_composition_runs_runtimes_in_one_process() {
    let workspace = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::write(workspace.path().join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::write(workspace.path().join("README.md"), "# Demo\n").unwrap();

    let mut config = EnterpriseAgentConfig::new(data.path(), workspace.path());
    config.model = EnterpriseModelConfig {
        provider: "deterministic".into(),
        endpoint: "http://127.0.0.1:1/v1".into(),
        api_key: None,
        model: "test-model".into(),
        profile: "default".into(),
    };
    let agent = EnterpriseAgent::with_model(config, deterministic_model().await)
        .await
        .unwrap();

    let provider = ToolProviderDefinition::new("builtin", "Builtin", ToolProviderKind::Builtin);
    let mut definition = ToolDefinition::new(
        "builtin",
        "echo",
        "1.0.0",
        serde_json::json!({
            "type": "object",
            "required": ["message"],
            "properties": {"message": {"type": "string"}},
            "additionalProperties": false
        }),
    );
    definition.default_permission = PermissionDecision::Allow;
    let tool = Arc::new(FunctionTool::new(
        definition.key.clone(),
        |request, _| async move {
            Ok(RawToolOutput::text(
                request.parameters["message"].as_str().unwrap(),
            ))
        },
    ));
    agent
        .tools()
        .load_provider(&StaticToolProvider::new(
            provider,
            vec![ToolRegistration::new(definition, tool)],
        ))
        .await
        .unwrap();

    let run = agent.run("inspect workspace", None).await.unwrap();
    assert_eq!(run.response, "Workspace inspection completed.");
    assert!(run.events.iter().any(|event| event.kind == "context_built"));
    assert!(run
        .events
        .iter()
        .any(|event| event.kind == "model_completed"));
    assert!(run
        .events
        .iter()
        .any(|event| event.kind == "tool_completed"));
    assert!(run.events.last().unwrap().is_terminal());

    let conversations = agent
        .sessions()
        .list_conversations(&run.session_id.to_string())
        .await
        .unwrap();
    let messages = agent
        .sessions()
        .list_messages(&conversations[0].id, 0, 20)
        .await
        .unwrap();
    assert_eq!(messages.total, 3);
    assert!(messages
        .items
        .iter()
        .all(|message| message.status == "DONE"));

    let snapshot = agent.workspace_snapshot().await.unwrap();
    assert!(snapshot
        .resources
        .iter()
        .any(|resource| resource.name == "README.md"));
    let read = agent
        .tools()
        .execute(core_agent::ToolRequest::new(
            "workspace/read_file@1.0.0",
            serde_json::json!({"path": "README.md"}),
        ))
        .await
        .unwrap();
    assert_eq!(read.content, vec![ToolContent::Text("# Demo\n".into())]);
    std::fs::write(workspace.path().join(".env.local"), "SECRET=hidden").unwrap();
    let denied = agent
        .tools()
        .execute(core_agent::ToolRequest::new(
            "workspace/read_file@1.0.0",
            serde_json::json!({"path": ".env.local"}),
        ))
        .await
        .unwrap();
    assert_eq!(denied.status, core_agent::ToolLifecycleStatus::Failed);
    assert_eq!(denied.error.unwrap().kind, "PERMISSION_DENIED");
    assert!(agent.cancel(run.session_id).await.unwrap());
    assert_eq!(agent.status(run.session_id).await.unwrap().state, "PAUSED");
    assert!(agent
        .resume(run.session_id)
        .await
        .unwrap()
        .last()
        .unwrap()
        .is_terminal());
    assert_eq!(agent.status(run.session_id).await.unwrap().state, "RUNNING");
}

#[tokio::test]
async fn denied_tool_call_is_a_terminal_agent_failure() {
    let workspace = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let mut config = EnterpriseAgentConfig::new(data.path(), workspace.path());
    config.model = EnterpriseModelConfig {
        provider: "deterministic".into(),
        endpoint: "http://127.0.0.1:1/v1".into(),
        api_key: None,
        model: "test-model".into(),
        profile: "default".into(),
    };
    let agent = EnterpriseAgent::with_model(config, deterministic_model().await)
        .await
        .unwrap();
    let provider = ToolProviderDefinition::new("builtin", "Builtin", ToolProviderKind::Builtin);
    let definition = ToolDefinition::new(
        "builtin",
        "echo",
        "1.0.0",
        serde_json::json!({
            "type": "object",
            "required": ["message"],
            "properties": {"message": {"type": "string"}},
            "additionalProperties": false
        }),
    );
    let executions = Arc::new(AtomicUsize::new(0));
    let execution_counter = executions.clone();
    let tool = Arc::new(FunctionTool::new(definition.key.clone(), move |_, _| {
        let execution_counter = execution_counter.clone();
        async move {
            execution_counter.fetch_add(1, Ordering::SeqCst);
            Ok(RawToolOutput::text("approved"))
        }
    }));
    agent
        .tools()
        .load_provider(&StaticToolProvider::new(
            provider,
            vec![ToolRegistration::new(definition, tool)],
        ))
        .await
        .unwrap();

    let error = agent.run("inspect workspace", None).await.unwrap_err();
    assert!(error.to_string().to_ascii_lowercase().contains("approval"));
    assert_eq!(executions.load(Ordering::SeqCst), 0);
    let session = agent
        .list_sessions()
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let events = agent.events(session.session_id).await;
    assert!(events.last().unwrap().is_terminal());
    assert_eq!(events.last().unwrap().kind, "execution_failed");

    let approved = agent
        .run_with_approval("inspect workspace", None, &AllowOnce)
        .await
        .unwrap();
    assert!(approved
        .events
        .iter()
        .any(|event| event.kind == "approval_required"));
    assert!(approved
        .events
        .iter()
        .any(|event| event.kind == "approval_decided"));
    assert!(approved
        .events
        .iter()
        .any(|event| event.kind == "tool_completed"));
    assert_eq!(executions.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn workspace_edit_requires_a_person_and_then_completes_end_to_end() {
    let workspace = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let mut config = EnterpriseAgentConfig::new(data.path(), workspace.path());
    config.model = EnterpriseModelConfig {
        provider: "workspace-writer".into(),
        endpoint: "http://127.0.0.1:1/v1".into(),
        api_key: None,
        model: "test-model".into(),
        profile: "default".into(),
    };
    let agent = EnterpriseAgent::with_model(config, workspace_write_model().await)
        .await
        .unwrap();

    let read_only = agent
        .run("/plan create a marker file", None)
        .await
        .unwrap_err();
    assert!(read_only.to_string().contains("read-only command boundary"));
    assert!(!workspace.path().join("agent-created.txt").exists());

    let denied = agent.run("create a marker file", None).await.unwrap_err();
    assert!(denied.to_string().contains("approval denied"));
    assert!(!workspace.path().join("agent-created.txt").exists());

    let approved = agent
        .run_with_approval("create a marker file", None, &AllowOnce)
        .await
        .unwrap();
    assert_eq!(approved.response, "Workspace edit completed.");
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("agent-created.txt")).unwrap(),
        "created through governed tool loop\n"
    );
    assert!(approved
        .events
        .iter()
        .any(|event| event.kind == "approval_required"));
    assert!(approved
        .events
        .iter()
        .any(|event| event.kind == "tool_completed"));
    assert!(approved
        .events
        .iter()
        .any(|event| event.kind == "checkpoint_created"));

    let undo = agent
        .execute_command("/undo", Some(approved.session_id))
        .await
        .unwrap()
        .unwrap();
    assert!(undo.response.starts_with("Undid checkpoint"));
    assert!(!workspace.path().join("agent-created.txt").exists());
    let redo = agent
        .execute_command("/redo", Some(approved.session_id))
        .await
        .unwrap()
        .unwrap();
    assert!(redo.response.starts_with("Redid checkpoint"));
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("agent-created.txt")).unwrap(),
        "created through governed tool loop\n"
    );
    agent
        .execute_command("/undo", Some(approved.session_id))
        .await
        .unwrap();
    std::fs::write(workspace.path().join("agent-created.txt"), "manual edit").unwrap();
    let conflict = agent
        .execute_command("/redo", Some(approved.session_id))
        .await
        .unwrap_err();
    assert!(conflict.to_string().contains("modified outside"));
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("agent-created.txt")).unwrap(),
        "manual edit"
    );

    let auto_workspace = tempfile::tempdir().unwrap();
    let auto_data = tempfile::tempdir().unwrap();
    let mut auto_config = EnterpriseAgentConfig::new(auto_data.path(), auto_workspace.path());
    auto_config.model = EnterpriseModelConfig {
        provider: "workspace-writer".into(),
        endpoint: "http://127.0.0.1:1/v1".into(),
        api_key: None,
        model: "test-model".into(),
        profile: "default".into(),
    };
    auto_config.permission_mode = PermissionMode::Auto;
    let auto_agent = EnterpriseAgent::with_model(auto_config, workspace_write_model().await)
        .await
        .unwrap();
    let automatic = auto_agent.run("create a marker file", None).await.unwrap();
    assert_eq!(automatic.response, "Workspace edit completed.");
    assert!(auto_workspace.path().join("agent-created.txt").is_file());
    assert!(!automatic
        .events
        .iter()
        .any(|event| event.kind == "approval_required"));
}

#[tokio::test]
async fn shared_mentions_and_slash_commands_are_end_to_end_and_zero_model_when_local() {
    let workspace = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join("context-folder")).unwrap();
    std::fs::write(workspace.path().join("context.txt"), "FILE_CONTEXT_MARKER").unwrap();
    std::fs::write(
        workspace.path().join("context-folder/nested.txt"),
        "FOLDER_CONTEXT_MARKER",
    )
    .unwrap();
    std::fs::write(workspace.path().join(".env"), "PRIVATE_VALUE").unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut config = EnterpriseAgentConfig::new(data.path(), workspace.path());
    config.model = EnterpriseModelConfig {
        provider: "interaction".into(),
        endpoint: "http://127.0.0.1:1/v1".into(),
        api_key: None,
        model: "test-model".into(),
        profile: "default".into(),
    };
    let agent = EnterpriseAgent::with_model(config, interaction_model(calls.clone()).await)
        .await
        .unwrap();

    let tools = agent
        .execute_command("/tools", None)
        .await
        .unwrap()
        .unwrap();
    assert!(tools.response.contains("read_file"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let run = agent
        .run("/plan Analyze @context.txt and @context-folder", None)
        .await
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(run
        .events
        .iter()
        .any(|event| event.kind == "context_mentions_resolved"));
    let conversations = agent
        .sessions()
        .list_conversations(&run.session_id.to_string())
        .await
        .unwrap();
    let messages = agent
        .sessions()
        .list_messages(&conversations[0].id, 0, 20)
        .await
        .unwrap();
    let persisted = format!("{:?}", messages.items);
    assert!(persisted.contains("/plan Analyze @context.txt"));
    assert!(!persisted.contains("FILE_CONTEXT_MARKER"));
    assert!(!persisted.contains("FOLDER_CONTEXT_MARKER"));

    let sessions_before = agent.list_sessions().await.unwrap().len();
    assert!(agent.run("Read @.env", None).await.is_err());
    assert_eq!(agent.list_sessions().await.unwrap().len(), sessions_before);
}
