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
        max_context_tokens: 128_000,
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

#[tokio::test]
#[ignore = "requires CORE_AGENT_API_KEY and a reachable DeepSeek endpoint"]
async fn deepseek_receives_file_and_folder_mentions_without_a_read_tool_call() {
    let api_key = std::env::var("CORE_AGENT_API_KEY")
        .expect("CORE_AGENT_API_KEY is required for the ignored live-provider test");
    let workspace = tempfile::tempdir().unwrap();
    let data = tempfile::tempdir().unwrap();
    let file_marker = format!("MENTION_FILE_{}", uuid::Uuid::new_v4().simple());
    let folder_marker = format!("MENTION_FOLDER_{}", uuid::Uuid::new_v4().simple());
    std::fs::create_dir(workspace.path().join("docs")).unwrap();
    std::fs::write(workspace.path().join("marker.txt"), &file_marker).unwrap();
    std::fs::write(workspace.path().join("docs/nested.txt"), &folder_marker).unwrap();

    let mut config = EnterpriseAgentConfig::new(data.path(), workspace.path());
    config.model = EnterpriseModelConfig {
        provider: "deepseek".into(),
        endpoint: std::env::var("CORE_AGENT_MODEL_ENDPOINT")
            .unwrap_or_else(|_| "https://api.deepseek.com".into()),
        api_key: Some(api_key),
        model: std::env::var("CORE_AGENT_MODEL").unwrap_or_else(|_| "deepseek-v4-flash".into()),
        profile: "live-mention-e2e".into(),
        max_context_tokens: 128_000,
    };
    let agent = EnterpriseAgent::open(config).await.unwrap();
    let run = agent
        .run(
            "Read the explicit @marker.txt and @docs context. Return the two marker values only.",
            None,
        )
        .await
        .unwrap();

    assert!(run.response.contains(&file_marker));
    assert!(run.response.contains(&folder_marker));
    assert!(run
        .events
        .iter()
        .any(|event| event.kind == "context_mentions_resolved"));
}
