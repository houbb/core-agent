use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use core_agent::{
    ContentPart, EnterpriseAgent, EnterpriseAgentConfig, EnterpriseApprovalDecision,
    EnterpriseApprovalHandler, EnterpriseApprovalRequest, EnterpriseModelConfig, FinishReason,
    InMemoryModelCatalog, ModelCapability, ModelCatalog, ModelError, ModelManager,
    ModelManagerBuilder, ModelProfile, ModelProvider, ModelRequest, ModelResponse, ModelRole,
    ModelUsage, ProviderDefinition, ToolCallRequest,
};

struct AllowOnce;

#[async_trait]
impl EnterpriseApprovalHandler for AllowOnce {
    async fn decide(&self, _request: &EnterpriseApprovalRequest) -> EnterpriseApprovalDecision {
        EnterpriseApprovalDecision::AllowOnce
    }
}

struct CapabilityProvider {
    expect_recalled_memory: bool,
}

#[async_trait]
impl ModelProvider for CapabilityProvider {
    fn key(&self) -> &str {
        "p034"
    }

    async fn invoke(
        &self,
        request: &ModelRequest,
        profile: &ModelProfile,
    ) -> Result<ModelResponse, ModelError> {
        let prompt = format!("{:?}", request.messages);
        assert!(prompt.contains("P034_AGENTS_MARKER"));
        assert!(prompt.contains("demo: Inspect P034 capabilities"));
        assert!(request.tools.iter().any(|tool| tool.name == "find_files"));
        assert!(request.tools.iter().any(|tool| tool.name == "search_files"));
        assert!(request.tools.iter().any(|tool| tool.name == "apply_patch"));
        assert!(request.tools.iter().any(|tool| tool.name == "run_command"));
        assert!(request
            .tools
            .iter()
            .any(|tool| tool.name == "start_command"));
        assert!(request
            .tools
            .iter()
            .any(|tool| tool.name == "delegate_task"));
        assert!(request
            .tools
            .iter()
            .any(|tool| tool.name == "remember_memory"));

        if self.expect_recalled_memory {
            assert!(prompt.contains("Always run cargo fmt before Rust review"));
            return Ok(response(
                request,
                profile,
                "Persistent memory recalled.",
                vec![],
            ));
        }

        let tool_results = request
            .messages
            .iter()
            .filter(|message| message.role == ModelRole::Tool)
            .count();
        let (text, tool_calls) = match tool_results {
            0 => (
                "Loading the relevant skill.",
                vec![ToolCallRequest {
                    id: "skill-1".into(),
                    name: "load_skill".into(),
                    arguments: serde_json::json!({"name": "demo"}),
                }],
            ),
            1 => {
                assert!(prompt.contains("P034_SKILL_BODY"));
                (
                    "Searching the workspace.",
                    vec![ToolCallRequest {
                        id: "search-1".into(),
                        name: "search_files".into(),
                        arguments: serde_json::json!({
                            "query": "P034_CAPABILITY_MARKER",
                            "file_pattern": "**/*.rs"
                        }),
                    }],
                )
            }
            2 => {
                assert!(prompt.contains("src/lib.rs"));
                (
                    "Remembering the explicit project rule.",
                    vec![ToolCallRequest {
                        id: "memory-1".into(),
                        name: "remember_memory".into(),
                        arguments: serde_json::json!({
                            "title": "Rust formatting rule",
                            "body": "Always run cargo fmt before Rust review",
                            "scope": "project",
                            "type": "rule",
                            "importance": "high",
                            "tags": ["rust", "formatting"]
                        }),
                    }],
                )
            }
            _ => ("P034 capability loop completed.", vec![]),
        };
        Ok(response(request, profile, text, tool_calls))
    }
}

fn response(
    request: &ModelRequest,
    profile: &ModelProfile,
    text: &str,
    tool_calls: Vec<ToolCallRequest>,
) -> ModelResponse {
    ModelResponse {
        request_id: request.id,
        provider: "p034".into(),
        model: profile.model.clone(),
        profile: profile.key.clone(),
        content: vec![ContentPart::text(text)],
        finish_reason: if tool_calls.is_empty() {
            FinishReason::Stop
        } else {
            FinishReason::ToolCall
        },
        tool_calls,
        usage: ModelUsage::default(),
        metadata: BTreeMap::new(),
        raw_response: None,
    }
}

async fn model(expect_recalled_memory: bool) -> Arc<ModelManager> {
    let catalog = Arc::new(InMemoryModelCatalog::default());
    catalog
        .upsert_provider(&ProviderDefinition::new("p034", "P034"))
        .await
        .unwrap();
    catalog
        .upsert_profile(
            &ModelProfile::new("default", "p034", "test-model")
                .with_capability(ModelCapability::Chat),
        )
        .await
        .unwrap();
    Arc::new(
        ModelManagerBuilder::new(catalog)
            .add_provider(Arc::new(CapabilityProvider {
                expect_recalled_memory,
            }))
            .build()
            .unwrap(),
    )
}

fn config(data: &std::path::Path, workspace: &std::path::Path) -> EnterpriseAgentConfig {
    let mut config = EnterpriseAgentConfig::new(data, workspace);
    config.model = EnterpriseModelConfig {
        provider: "p034".into(),
        endpoint: "http://127.0.0.1:1/v1".into(),
        api_key: None,
        model: "test-model".into(),
        profile: "default".into(),
        max_context_tokens: 128_000,
    };
    config
}

#[tokio::test]
async fn guidance_skills_search_and_persistent_memory_share_the_enterprise_tool_loop() {
    let workspace = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(workspace.path().join("src")).unwrap();
    std::fs::create_dir_all(workspace.path().join(".agents/skills/demo")).unwrap();
    std::fs::write(
        workspace.path().join("AGENTS.md"),
        "Follow P034_AGENTS_MARKER for this project.",
    )
    .unwrap();
    std::fs::write(
        workspace.path().join(".agents/skills/demo/SKILL.md"),
        "---\nname: demo\ndescription: Inspect P034 capabilities\n---\nP034_SKILL_BODY",
    )
    .unwrap();
    std::fs::write(
        workspace.path().join("src/lib.rs"),
        "// P034_CAPABILITY_MARKER\n",
    )
    .unwrap();

    let agent =
        EnterpriseAgent::with_model(config(data.path(), workspace.path()), model(false).await)
            .await
            .unwrap();
    let run = agent
        .run_with_approval(
            "Inspect P034 capabilities and remember the Rust formatting rule",
            None,
            &AllowOnce,
        )
        .await
        .unwrap();
    assert_eq!(run.response, "P034 capability loop completed.");
    assert!(run
        .events
        .iter()
        .any(|event| event.kind == "guidance_loaded"));
    assert_eq!(
        run.events
            .iter()
            .filter(|event| event.kind == "tool_completed")
            .count(),
        3
    );
    drop(agent);

    let reopened =
        EnterpriseAgent::with_model(config(data.path(), workspace.path()), model(true).await)
            .await
            .unwrap();
    let recalled = reopened
        .run("What is the Rust formatting rule?", None)
        .await
        .unwrap();
    assert_eq!(recalled.response, "Persistent memory recalled.");
    assert!(recalled
        .events
        .iter()
        .any(|event| event.kind == "memory_recalled"));
}
