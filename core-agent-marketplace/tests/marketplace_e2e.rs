use std::sync::Arc;

use core_agent_marketplace::{
    AssetType, InMemoryMarketplaceStore, MarketplaceManager, MarketplaceQuery,
};
use serde_json::Value;
use uuid::Uuid;

#[tokio::test]
async fn marketplace_e2e_publish_install() {
    let store = Arc::new(InMemoryMarketplaceStore::default());
    let manager = MarketplaceManager::new(store);

    // Publish a skill
    let pkg = manager
        .publish(
            AssetType::Skill,
            "redis-diagnosis",
            "Redis Diagnosis",
            "1.0.0",
            "core-agent",
            "Diagnose Redis issues",
            serde_json::json!({"steps": ["check slowlog", "check latency"]}),
        )
        .await
        .unwrap();
    assert_eq!(pkg.state.as_str(), "PUBLISHED");
    assert_eq!(pkg.downloads, 0);

    // Install (download)
    let installed = manager.install(pkg.id, "user-1").await.unwrap();
    assert_eq!(installed.downloads, 1);

    // Find by key and version
    let found = manager
        .find_by_key("redis-diagnosis", "1.0.0")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.id, pkg.id);

    // List published skills
    let skills = manager
        .list(&MarketplaceQuery {
            asset_type: Some(AssetType::Skill),
            state: None,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(skills.len(), 1);
}

#[tokio::test]
async fn marketplace_e2e_multiple_assets() {
    let store = Arc::new(InMemoryMarketplaceStore::default());
    let manager = MarketplaceManager::new(store);

    // Publish different asset types
    manager
        .publish(
            AssetType::Agent,
            "db-agent",
            "Database Agent",
            "1.0",
            "core-agent",
            "Database expert",
            Value::Null,
        )
        .await
        .unwrap();
    manager
        .publish(
            AssetType::Skill,
            "redis-skill",
            "Redis Skill",
            "1.0",
            "core-agent",
            "Redis expert",
            Value::Null,
        )
        .await
        .unwrap();
    manager
        .publish(
            AssetType::Plugin,
            "http-plugin",
            "HTTP Plugin",
            "1.0",
            "core-agent",
            "HTTP client",
            Value::Null,
        )
        .await
        .unwrap();

    let snap = manager.snapshot().await.unwrap();
    assert_eq!(snap.total_packages, 3);
    assert_eq!(*snap.by_type.get("AGENT").unwrap(), 1);
    assert_eq!(*snap.by_type.get("SKILL").unwrap(), 1);
    assert_eq!(*snap.by_type.get("PLUGIN").unwrap(), 1);
}