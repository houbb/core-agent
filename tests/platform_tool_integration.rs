use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use core_agent::integrations::{PlatformToolPolicy, ToolGovernanceResolver};
use core_agent::{
    FunctionTool, GovernanceRequest, PermissionDecision, PlatformManager, PlatformPolicy,
    PolicyEffect, PolicyRule, Quota, RawToolOutput, StaticToolProvider, Tenant, ToolDefinition,
    ToolError, ToolManager, ToolProviderDefinition, ToolProviderKind, ToolRegistration,
    ToolRequest,
};
use uuid::Uuid;

struct Resolver {
    tenant_id: Uuid,
}

#[async_trait]
impl ToolGovernanceResolver for Resolver {
    async fn resolve(
        &self,
        request: &ToolRequest,
        _tool: &ToolDefinition,
    ) -> Result<GovernanceRequest, String> {
        let mut governance = GovernanceRequest::new(
            self.tenant_id,
            request.subject.as_deref().unwrap_or("anonymous"),
            "tool.execute",
            "builtin.echo",
            "tool-runtime",
        );
        governance.request_id = request.id;
        governance.quota_key = Some("tool-calls".into());
        governance.units = 1;
        Ok(governance)
    }
}

#[tokio::test]
async fn platform_policy_governs_real_tool_execution_and_quota() {
    let platform = Arc::new(PlatformManager::builder().build());
    platform.start().unwrap();
    let tenant = platform
        .create_tenant(Tenant::new("acme", "Acme", "operator"))
        .await
        .unwrap();
    let mut policy = PlatformPolicy::new(tenant.id, "tools", "Tools", "operator");
    policy.rules.push(PolicyRule {
        id: Uuid::new_v4(),
        subjects: ["operator".into()].into_iter().collect(),
        actions: ["tool.execute".into()].into_iter().collect(),
        resources: ["builtin.echo".into()].into_iter().collect(),
        attributes: Default::default(),
        effect: PolicyEffect::Allow,
        priority: 100,
    });
    platform.create_policy(policy).await.unwrap();
    platform
        .create_quota(Quota::new(tenant.id, "tool-calls", 1, 60, "operator").unwrap())
        .await
        .unwrap();

    let calls = Arc::new(AtomicUsize::new(0));
    let tools = ToolManager::builder()
        .policy(Arc::new(PlatformToolPolicy::new(
            platform.clone(),
            Arc::new(Resolver {
                tenant_id: tenant.id,
            }),
        )))
        .build();
    let mut definition = ToolDefinition::new(
        "builtin",
        "echo",
        "1.0.0",
        serde_json::json!({"type":"object"}),
    );
    definition.default_permission = PermissionDecision::Allow;
    let key = definition.key.clone();
    let observed = calls.clone();
    let tool = FunctionTool::new(key.clone(), move |_, _| {
        let observed = observed.clone();
        async move {
            observed.fetch_add(1, Ordering::SeqCst);
            Ok(RawToolOutput::text("ok"))
        }
    });
    tools
        .load_provider(&StaticToolProvider::new(
            ToolProviderDefinition::new("builtin", "Builtin", ToolProviderKind::Builtin),
            vec![ToolRegistration::new(definition, Arc::new(tool))],
        ))
        .await
        .unwrap();

    let mut first = ToolRequest::new(&key, serde_json::json!({}));
    first.subject = Some("operator".into());
    assert!(tools.execute(first).await.is_ok());
    let mut second = ToolRequest::new(&key, serde_json::json!({}));
    second.subject = Some("operator".into());
    assert!(matches!(
        tools.execute(second).await,
        Err(ToolError::PolicyDenied(_))
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(platform.list_audits(tenant.id).await.unwrap().len(), 2);
}
