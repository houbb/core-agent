use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use core_agent_config::{
    project_storage_key, AgentConfigPatch, ConfigLayer, ConfigManager, ConfigModelPatch,
    ConfigProvider, ConfigRequest, ConfigResult, ConfigSourceInfo, EnvironmentConfigProvider,
    EnvironmentSecretResolver, ProjectFileConfigProvider, UserFileConfigProvider,
};

struct RemoteStrategy;

#[async_trait]
impl ConfigProvider for RemoteStrategy {
    fn key(&self) -> &str {
        "remote-test"
    }

    fn priority(&self) -> u16 {
        150
    }

    async fn load(&self, _request: &ConfigRequest) -> ConfigResult<Option<ConfigLayer>> {
        Ok(Some(ConfigLayer {
            source: ConfigSourceInfo {
                provider: self.key().into(),
                priority: self.priority(),
                location: Some("memory://remote-test".into()),
            },
            patch: AgentConfigPatch {
                model: Some(ConfigModelPatch {
                    name: Some("remote-model".into()),
                    ..ConfigModelPatch::default()
                }),
                ..AgentConfigPatch::default()
            },
        }))
    }
}

#[tokio::test]
async fn provider_strategies_merge_by_explicit_priority_and_resolve_secrets() {
    let user = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    std::fs::create_dir(workspace.path().join(".agent")).unwrap();
    std::fs::write(
        user.path().join("core-agent-config.yaml"),
        "model:\n  provider: deepseek\n  endpoint: https://api.deepseek.com\n  name: global-model\n  apiKeyRef: env:TEST_CONFIG_KEY\npermissions:\n  mode: strict\n",
    )
    .unwrap();
    std::fs::write(
        workspace.path().join(".agent/config.json"),
        r#"{"model":{"name":"project-model"},"permissions":{"mode":"auto"}}"#,
    )
    .unwrap();
    let environment = BTreeMap::from([
        ("CORE_AGENT_MODEL".into(), "environment-model".into()),
        ("CORE_AGENT_MODEL_MAX_CONTEXT_TOKENS".into(), "64000".into()),
    ]);
    let secrets = BTreeMap::from([("TEST_CONFIG_KEY".into(), "private-value".into())]);
    let manager = ConfigManager::builder()
        .provider(Arc::new(UserFileConfigProvider::new(user.path())))
        .provider(Arc::new(RemoteStrategy))
        .provider(Arc::new(ProjectFileConfigProvider))
        .provider(Arc::new(EnvironmentConfigProvider::new(environment)))
        .secret_resolver(Arc::new(EnvironmentSecretResolver::new(secrets)))
        .build()
        .unwrap();

    let resolved = manager
        .resolve(&ConfigRequest::new(workspace.path()))
        .await
        .unwrap();

    assert_eq!(resolved.config.model.name, "environment-model");
    assert_eq!(resolved.config.model.max_context_tokens, 64_000);
    assert_eq!(resolved.config.permissions.mode, "auto");
    assert_eq!(
        resolved.config.model.api_key.as_deref(),
        Some("private-value")
    );
    assert_eq!(
        resolved
            .sources
            .iter()
            .map(|source| source.provider.as_str())
            .collect::<Vec<_>>(),
        vec!["user-file", "remote-test", "project-file", "environment"]
    );
    let debug = format!("{resolved:?}");
    let redacted = resolved.redacted().to_string();
    assert!(!debug.contains("private-value"));
    assert!(!redacted.contains("private-value"));
    assert!(redacted.contains("apiKeyConfigured"));
}

#[tokio::test]
async fn ambiguous_user_formats_and_symlinked_config_fail_closed() {
    let user = tempfile::tempdir().unwrap();
    std::fs::write(user.path().join("core-agent-config.yaml"), "version: 1\n").unwrap();
    std::fs::write(
        user.path().join("core-agent-config.json"),
        "{\"version\":1}",
    )
    .unwrap();
    let manager = ConfigManager::builder()
        .provider(Arc::new(UserFileConfigProvider::new(user.path())))
        .build()
        .unwrap();
    assert!(manager.resolve(&ConfigRequest::global()).await.is_err());

    std::fs::remove_file(user.path().join("core-agent-config.json")).unwrap();
    let target = user.path().join("target.yaml");
    std::fs::rename(user.path().join("core-agent-config.yaml"), &target).unwrap();
    let link = user.path().join("core-agent-config.yaml");
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&target, &link);
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&target, &link);
    if linked.is_ok() {
        assert!(manager.resolve(&ConfigRequest::global()).await.is_err());
    }
}

#[test]
fn project_storage_keys_are_stable_and_do_not_expose_paths() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let one = project_storage_key(first.path()).unwrap();
    assert_eq!(one, project_storage_key(first.path()).unwrap());
    assert_ne!(one, project_storage_key(second.path()).unwrap());
    assert_eq!(one.len(), 64);
    assert!(!one.contains(&first.path().to_string_lossy().to_string()));
}

#[tokio::test]
async fn version_two_selects_one_of_multiple_unique_models_with_token_limits() {
    let user = tempfile::tempdir().unwrap();
    std::fs::write(
        user.path().join("core-agent-config.yaml"),
        "version: 2\nactiveModel: second\nmodels:\n  - name: first\n    baseURL: https://first.example/v1\n    maxContextTokens: 64000\n  - name: second\n    baseURL: https://second.example/v1\n    maxContextTokens: 128000\ncontext:\n  compression:\n    strategy: extractive-summary\n    triggerPercent: 75\n    keepRecentMessages: 12\n",
    )
    .unwrap();
    let manager = ConfigManager::builder()
        .provider(Arc::new(UserFileConfigProvider::new(user.path())))
        .build()
        .unwrap();

    let resolved = manager.resolve(&ConfigRequest::global()).await.unwrap();
    assert_eq!(resolved.config.models.len(), 2);
    assert_eq!(resolved.config.active_model, "second");
    assert_eq!(resolved.config.model.name, "second");
    assert_eq!(resolved.config.model.max_context_tokens, 128_000);
    assert_eq!(
        resolved.config.context.compression.strategy,
        "extractive-summary"
    );

    std::fs::write(
        user.path().join("core-agent-config.yaml"),
        "version: 2\nactiveModel: same\nmodels:\n  - name: same\n    baseURL: https://one.example/v1\n  - name: SAME\n    baseURL: https://two.example/v1\n",
    )
    .unwrap();
    let error = manager.resolve(&ConfigRequest::global()).await.unwrap_err();
    assert!(error.to_string().contains("unique"));
}
