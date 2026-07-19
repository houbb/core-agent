use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use core_agent_ecosystem::{
    EcosystemError, EcosystemManager, MarketplacePackage, PackageDependency, PackageKind,
    PackageState, PublicationDecision, Publisher,
};
use core_agent_platform::{PlatformManager, PlatformPolicy, PolicyEffect, PolicyRule, Tenant};
use uuid::Uuid;

async fn manager() -> (EcosystemManager, Arc<PlatformManager>, Uuid) {
    let platform = Arc::new(PlatformManager::builder().build());
    platform.start().unwrap();
    let tenant = platform
        .create_tenant(Tenant::new("ecosystem", "Ecosystem", "admin"))
        .await
        .unwrap();
    let mut policy = PlatformPolicy::new(tenant.id, "ecosystem", "Ecosystem", "admin");
    policy.rules.push(PolicyRule {
        id: Uuid::new_v4(),
        subjects: BTreeSet::from(["alice".into(), "bob".into(), "consumer".into()]),
        actions: BTreeSet::from(["*".into()]),
        resources: BTreeSet::from(["*".into()]),
        attributes: BTreeMap::new(),
        effect: PolicyEffect::Allow,
        priority: 100,
    });
    platform.create_policy(policy).await.unwrap();
    (EcosystemManager::new(platform.clone()), platform, tenant.id)
}

async fn publisher(manager: &EcosystemManager, tenant_id: Uuid) -> Publisher {
    manager
        .register_publisher(Publisher::new(tenant_id, "acme", "Acme", "alice", "alice"))
        .await
        .unwrap()
}

async fn publish(manager: &EcosystemManager, package: MarketplacePackage) -> MarketplacePackage {
    let package = manager.create_package(package).await.unwrap();
    manager.submit(package.id, "alice").await.unwrap();
    manager
        .review(
            package.id,
            PublicationDecision::Approved,
            "Verified metadata and checksum",
            "bob",
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn independent_review_lists_packages_and_resolves_dependency_order() {
    let (manager, platform, tenant_id) = manager().await;
    let publisher = publisher(&manager, tenant_id).await;
    let capability = publish(
        &manager,
        MarketplacePackage::new(
            tenant_id,
            publisher.id,
            PackageKind::Capability,
            "prometheus",
            "Prometheus",
            "1.0.0",
            "alice",
        ),
    )
    .await;
    let mut agent = MarketplacePackage::new(
        tenant_id,
        publisher.id,
        PackageKind::Agent,
        "rca-agent",
        "RCA Agent",
        "1.0.0",
        "alice",
    );
    agent.dependencies.push(PackageDependency {
        key: capability.key.clone(),
        version: capability.package_version.clone(),
    });
    agent.required_capabilities.insert("metrics.query".into());
    let agent = publish(&manager, agent).await;

    let plan = manager
        .resolve_install(tenant_id, "rca-agent", "1.0.0", "consumer")
        .await
        .unwrap();
    assert_eq!(
        plan.packages
            .iter()
            .map(|item| item.key.as_str())
            .collect::<Vec<_>>(),
        vec!["prometheus", "rca-agent"]
    );
    assert!(plan.required_capabilities.contains("metrics.query"));
    let rated = manager
        .rate(agent.id, 5, "Useful", "consumer")
        .await
        .unwrap();
    assert_eq!(rated.average_rating(), Some(5.0));
    assert!(matches!(
        manager.rate(agent.id, 4, "Again", "consumer").await,
        Err(EcosystemError::Conflict(_))
    ));
    assert!(platform.list_audits(tenant_id).await.unwrap().len() >= 10);
}

#[tokio::test]
async fn self_review_and_unlisted_dependencies_fail_closed() {
    let (manager, _, tenant_id) = manager().await;
    let publisher = publisher(&manager, tenant_id).await;
    let package = manager
        .create_package(MarketplacePackage::new(
            tenant_id,
            publisher.id,
            PackageKind::Agent,
            "agent",
            "Agent",
            "1.0.0",
            "alice",
        ))
        .await
        .unwrap();
    manager.submit(package.id, "alice").await.unwrap();
    assert!(matches!(
        manager
            .review(package.id, PublicationDecision::Approved, "self", "alice")
            .await,
        Err(EcosystemError::Denied(_))
    ));

    let mut missing = MarketplacePackage::new(
        tenant_id,
        publisher.id,
        PackageKind::Template,
        "template",
        "Template",
        "1.0.0",
        "alice",
    );
    missing.dependencies.push(PackageDependency {
        key: "missing".into(),
        version: "1.0.0".into(),
    });
    let missing = manager.create_package(missing).await.unwrap();
    assert!(matches!(
        manager.submit(missing.id, "alice").await,
        Err(EcosystemError::NotFound(_))
    ));
    assert_eq!(manager.packages(tenant_id, true).unwrap().len(), 0);
}

#[tokio::test]
async fn default_deny_blocks_marketplace_mutation() {
    let platform = Arc::new(PlatformManager::builder().build());
    platform.start().unwrap();
    let tenant = platform
        .create_tenant(Tenant::new("closed", "Closed", "admin"))
        .await
        .unwrap();
    let manager = EcosystemManager::new(platform.clone());
    assert!(matches!(
        manager
            .register_publisher(Publisher::new(tenant.id, "acme", "Acme", "alice", "alice"))
            .await,
        Err(EcosystemError::Denied(_))
    ));
    assert_eq!(platform.list_audits(tenant.id).await.unwrap().len(), 1);
}

#[test]
fn package_rejects_self_dependency_before_catalog_mutation() {
    let mut package = MarketplacePackage::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        PackageKind::Agent,
        "agent",
        "Agent",
        "1.0.0",
        "alice",
    );
    package.dependencies.push(PackageDependency {
        key: "agent".into(),
        version: "1.0.0".into(),
    });
    let platform = Arc::new(PlatformManager::builder().build());
    let manager = EcosystemManager::new(platform);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    assert!(matches!(
        runtime.block_on(manager.create_package(package)),
        Err(EcosystemError::Validation(_))
    ));
    assert_ne!(PackageState::Listed, PackageState::Draft);
}
