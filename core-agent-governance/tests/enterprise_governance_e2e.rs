use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use core_agent_governance::{
    CostRecord, EnterpriseError, EnterpriseGovernanceManager, EnterprisePrincipal, GovernanceAsset,
    GovernanceAssetState, IdentityProviderKind,
};
use core_agent_platform::{PlatformManager, PlatformPolicy, PolicyEffect, PolicyRule, Tenant};
use uuid::Uuid;

async fn governed_manager() -> (Arc<PlatformManager>, Uuid) {
    let platform = Arc::new(PlatformManager::builder().build());
    platform.start().unwrap();
    let tenant = platform
        .create_tenant(Tenant::new("enterprise", "Enterprise", "admin"))
        .await
        .unwrap();
    let mut policy = PlatformPolicy::new(tenant.id, "enterprise", "Enterprise", "admin");
    policy.rules.push(PolicyRule {
        id: Uuid::new_v4(),
        subjects: BTreeSet::from(["admin".into(), "alice".into(), "bob".into()]),
        actions: BTreeSet::from(["*".into()]),
        resources: BTreeSet::from(["*".into()]),
        attributes: BTreeMap::new(),
        effect: PolicyEffect::Allow,
        priority: 100,
    });
    platform.create_policy(policy).await.unwrap();
    (platform, tenant.id)
}

async fn bind_team(manager: &EnterpriseGovernanceManager, tenant_id: Uuid) {
    manager
        .bind_principal(EnterprisePrincipal::new(
            tenant_id,
            "alice",
            IdentityProviderKind::Oidc,
            "Alice",
            "admin",
        ))
        .await
        .unwrap();
    manager
        .bind_principal(EnterprisePrincipal::new(
            tenant_id,
            "bob",
            IdentityProviderKind::Saml,
            "Bob",
            "admin",
        ))
        .await
        .unwrap();
}

#[tokio::test]
async fn asset_governance_requires_independent_approval_before_production() {
    let (platform, tenant_id) = governed_manager().await;
    let manager = EnterpriseGovernanceManager::new(platform.clone());
    bind_team(&manager, tenant_id).await;

    let asset = manager
        .register_asset(GovernanceAsset::new(
            tenant_id,
            core_agent_governance::AiAssetType::Agent,
            "support-agent",
            "Support Agent",
            "1.0.0",
            "alice",
        ))
        .await
        .unwrap();
    assert_eq!(
        manager.submit_asset(asset.id, "alice").await.unwrap().state,
        GovernanceAssetState::Reviewed
    );
    assert!(matches!(
        manager.approve_asset(asset.id, "self", "alice").await,
        Err(EnterpriseError::Denied(_))
    ));
    assert_eq!(
        manager
            .approve_asset(asset.id, "Risk review complete", "bob")
            .await
            .unwrap()
            .state,
        GovernanceAssetState::Approved
    );
    assert_eq!(
        manager
            .transition_asset(asset.id, GovernanceAssetState::Production, "bob")
            .await
            .unwrap()
            .state,
        GovernanceAssetState::Production
    );
    assert_eq!(
        manager
            .transition_asset(asset.id, GovernanceAssetState::Suspended, "bob")
            .await
            .unwrap()
            .state,
        GovernanceAssetState::Suspended
    );
    assert!(platform.list_audits(tenant_id).await.unwrap().len() >= 8);
}

#[tokio::test]
async fn cost_ledger_is_idempotent_and_uses_integer_totals() {
    let (platform, tenant_id) = governed_manager().await;
    let manager = EnterpriseGovernanceManager::new(platform);
    bind_team(&manager, tenant_id).await;
    let mut cost = CostRecord::new(tenant_id, "generation-001", "USD", 1_250_000, "alice");
    cost.input_tokens = 100;
    cost.output_tokens = 40;

    manager.record_cost(cost.clone()).await.unwrap();
    assert!(matches!(
        manager.record_cost(cost).await,
        Err(EnterpriseError::Conflict(_))
    ));
    let snapshot = manager.snapshot(tenant_id).unwrap();
    assert_eq!(snapshot.cost_micros_by_currency["USD"], 1_250_000);
    assert_eq!((snapshot.input_tokens, snapshot.output_tokens), (100, 40));
}

#[tokio::test]
async fn missing_platform_policy_denies_and_audits_enterprise_writes() {
    let platform = Arc::new(PlatformManager::builder().build());
    platform.start().unwrap();
    let tenant = platform
        .create_tenant(Tenant::new("denied", "Denied", "admin"))
        .await
        .unwrap();
    let manager = EnterpriseGovernanceManager::new(platform.clone());

    assert!(matches!(
        manager
            .bind_principal(EnterprisePrincipal::new(
                tenant.id,
                "alice",
                IdentityProviderKind::Oidc,
                "Alice",
                "admin",
            ))
            .await,
        Err(EnterpriseError::Denied(_))
    ));
    let audits = platform.list_audits(tenant.id).await.unwrap();
    assert_eq!(audits.len(), 1);
    assert!(!audits[0]
        .decision
        .eq(&core_agent_platform::AuditDecision::Allowed));
}
