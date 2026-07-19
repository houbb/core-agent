use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use core_agent::integrations::{ExtensionToolResolver, ToolExtensionHost};
use core_agent::{
    CapabilityInvocation, CapabilityManifest, ExtensionManager, ExtensionManifest,
    ExtensionProvider, ExtensionProviderKind, FunctionTool, InstallExtensionRequest,
    PermissionDecision, ProviderManifest, RawToolOutput, StaticToolProvider, ToolDefinition,
    ToolManager, ToolProviderDefinition, ToolProviderKind, ToolRegistration, ToolRequest,
};
use sha2::{Digest, Sha256};

struct Resolver;

#[async_trait]
impl ExtensionToolResolver for Resolver {
    async fn resolve(
        &self,
        _provider: &ExtensionProvider,
        invocation: &CapabilityInvocation,
    ) -> Result<ToolRequest, String> {
        Ok(ToolRequest::new(
            "builtin/extension-echo@1.0.0",
            invocation.input.clone(),
        ))
    }
}

#[tokio::test]
async fn extension_capability_resolves_through_real_tool_runtime() {
    let calls = Arc::new(AtomicUsize::new(0));
    let tools = Arc::new(ToolManager::builder().build());
    let provider = ToolProviderDefinition::new("builtin", "Builtin", ToolProviderKind::Builtin);
    let mut definition = ToolDefinition::new(
        "builtin",
        "extension-echo",
        "1.0.0",
        serde_json::json!({
            "type": "object",
            "properties": {"message": {"type": "string"}},
            "required": ["message"],
            "additionalProperties": false
        }),
    );
    definition.default_permission = PermissionDecision::Allow;
    let observed = calls.clone();
    let tool = Arc::new(FunctionTool::new(definition.key.clone(), move |_, _| {
        let observed = observed.clone();
        async move {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(RawToolOutput::text("extension tool completed"))
        }
    }));
    tools
        .load_provider(&StaticToolProvider::new(
            provider,
            vec![ToolRegistration::new(definition, tool)],
        ))
        .await
        .unwrap();

    let directory = tempfile::tempdir().unwrap();
    let artifact = directory.path().join("extension.bin");
    std::fs::write(&artifact, b"tool extension").unwrap();
    let checksum = format!("{:x}", Sha256::digest(b"tool extension"));
    let extensions = ExtensionManager::builder()
        .host(Arc::new(ToolExtensionHost::new(tools, Arc::new(Resolver))))
        .build();
    let extension = extensions
        .install(InstallExtensionRequest {
            manifest: ExtensionManifest {
                key: "tool-bridge".into(),
                name: "Tool Bridge".into(),
                version: "1.0.0".into(),
                description: String::new(),
                entrypoint: "tool-bridge".into(),
                requested_permissions: BTreeSet::new(),
                capabilities: vec![CapabilityManifest {
                    key: "echo.text".into(),
                    version: "1.0.0".into(),
                    name: "Echo Text".into(),
                    permissions: BTreeSet::new(),
                    metadata: BTreeMap::new(),
                }],
                providers: vec![ProviderManifest {
                    key: "tool-provider".into(),
                    kind: ExtensionProviderKind::Local,
                    capabilities: ["echo.text".into()].into_iter().collect(),
                    priority: 0,
                    config: serde_json::Value::Null,
                    metadata: BTreeMap::new(),
                }],
                metadata: BTreeMap::new(),
            },
            source_uri: format!("file:{}", artifact.display()),
            checksum,
            actor: "operator".into(),
        })
        .await
        .unwrap();
    extensions.load(extension.id, "operator").await.unwrap();
    extensions.enable(extension.id, "operator").await.unwrap();
    let result = extensions
        .execute(CapabilityInvocation::new(
            "echo.text",
            serde_json::json!({"message": "hello"}),
            "operator",
        ))
        .await
        .unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        result.summary,
        "Tool builtin/extension-echo@1.0.0 provided echo.text"
    );
}
