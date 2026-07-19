use core_agent::{EnterpriseAgent, EnterpriseAgentConfig, EnterpriseModelConfig};

/// Opt-in live-provider gate. Credentials are read only from the process
/// environment and are never persisted by the test.
#[tokio::test]
#[ignore = "requires CORE_AGENT_API_KEY and a reachable DeepSeek endpoint"]
async fn deepseek_reads_an_unknown_workspace_file_through_the_tool_loop() {
    let api_key = std::env::var("CORE_AGENT_API_KEY")
        .expect("CORE_AGENT_API_KEY is required for the ignored live-provider test");
    let workspace = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let marker = format!("LIVE_WORKSPACE_{}", uuid::Uuid::new_v4().simple());
    std::fs::write(workspace.path().join("marker.txt"), &marker).unwrap();

    let mut config = EnterpriseAgentConfig::new(data.path(), workspace.path());
    config.model = EnterpriseModelConfig {
        provider: "deepseek".into(),
        endpoint: std::env::var("CORE_AGENT_MODEL_ENDPOINT")
            .unwrap_or_else(|_| "https://api.deepseek.com".into()),
        api_key: Some(api_key),
        model: std::env::var("CORE_AGENT_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into()),
        profile: "live-e2e".into(),
    };
    let agent = EnterpriseAgent::open(config).await.unwrap();
    let run = agent
        .run(
            "Use the workspace read_file tool to read marker.txt. Return only its exact content.",
            None,
        )
        .await
        .unwrap();

    assert!(run.response.contains(&marker));
    assert!(run
        .events
        .iter()
        .any(|event| event.kind == "tool_completed"));
    assert!(run.events.last().unwrap().is_terminal());
}
