use std::collections::BTreeMap;
use std::sync::Arc;

use agent_cli::{AgentClient, AgentRequest, EmbeddedAgentClient};
use async_trait::async_trait;
use core_agent::{
    ContentPart, EnterpriseAgent, EnterpriseAgentConfig, FinishReason, InMemoryModelCatalog,
    ModelCapability, ModelCatalog, ModelError, ModelManagerBuilder, ModelProfile, ModelProvider,
    ModelRequest, ModelResponse, ModelUsage, ProviderDefinition,
};
use futures_util::StreamExt;

struct TerminalModel;

#[async_trait]
impl ModelProvider for TerminalModel {
    fn key(&self) -> &str {
        "terminal-test"
    }

    async fn invoke(
        &self,
        request: &ModelRequest,
        profile: &ModelProfile,
    ) -> Result<ModelResponse, ModelError> {
        Ok(ModelResponse {
            request_id: request.id,
            provider: self.key().into(),
            model: profile.model.clone(),
            profile: profile.key.clone(),
            content: vec![ContentPart::text("Terminal embedded Runtime is ready.")],
            tool_calls: Vec::new(),
            usage: ModelUsage::default(),
            finish_reason: FinishReason::Stop,
            metadata: BTreeMap::new(),
            raw_response: None,
        })
    }
}

#[tokio::test]
async fn terminal_adapter_uses_enterprise_agent_without_a_server() {
    let workspace = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let catalog = Arc::new(InMemoryModelCatalog::default());
    catalog
        .upsert_provider(&ProviderDefinition::new("terminal-test", "Terminal test"))
        .await
        .unwrap();
    catalog
        .upsert_profile(
            &ModelProfile::new("default", "terminal-test", "test-model")
                .with_capability(ModelCapability::Chat),
        )
        .await
        .unwrap();
    let models = Arc::new(
        ModelManagerBuilder::new(catalog)
            .add_provider(Arc::new(TerminalModel))
            .build()
            .unwrap(),
    );
    let agent = Arc::new(
        EnterpriseAgent::with_model(
            EnterpriseAgentConfig::new(data.path(), workspace.path()),
            models,
        )
        .await
        .unwrap(),
    );
    let canonical = workspace.path().canonicalize().unwrap();
    let client = EmbeddedAgentClient::from_runtime(agent, canonical.to_string_lossy());

    let submission = client
        .send(AgentRequest {
            session_id: None,
            message: "verify the terminal entry".into(),
            workspace: canonical.to_string_lossy().into_owned(),
        })
        .await
        .unwrap();
    let events = client
        .stream(submission.session_id)
        .await
        .unwrap()
        .collect::<Vec<_>>()
        .await;

    assert!(events.iter().all(Result::is_ok));
    assert!(events
        .last()
        .and_then(|event| event.as_ref().ok())
        .is_some_and(|event| event.is_terminal()));
    assert_eq!(client.sessions().await.unwrap().len(), 1);
}
