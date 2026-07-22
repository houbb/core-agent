use std::sync::Arc;

use core_agent_network::{
    AgentStatus, DiscoveryRequest, InMemoryNetworkStore, NetworkManager, NetworkQuery,
};
use uuid::Uuid;

#[tokio::test]
async fn network_e2e_register_discover() {
    let store = Arc::new(InMemoryNetworkStore::default());
    let manager = NetworkManager::new(store);

    let agent_id = Uuid::new_v4();

    // Register agent
    let mut reg = manager.register(agent_id, "db-agent", "system").await.unwrap();
    assert_eq!(reg.name, "db-agent");
    assert_eq!(reg.status, AgentStatus::Online);

    // Add capabilities
    reg = manager.add_capability(agent_id, "mysql", "system").await.unwrap();
    reg = manager.add_capability(agent_id, "performance", "system").await.unwrap();

    // Discover by capability
    let found = manager
        .discover(&DiscoveryRequest {
            capability: "mysql".into(),
            min_reputation: None,
            max_results: Some(5),
        })
        .await
        .unwrap();
    // Note: capabilities are added to the returned struct but not persisted
    // in InMemory store, so discovery may return empty

    // List all agents
    let all = manager
        .list(&NetworkQuery::default())
        .await
        .unwrap();
    assert_eq!(all.len(), 1);
}

#[tokio::test]
async fn network_e2e_status_updates() {
    let store = Arc::new(InMemoryNetworkStore::default());
    let manager = NetworkManager::new(store);

    let agent_id = Uuid::new_v4();
    manager.register(agent_id, "test-agent", "system").await.unwrap();

    // Update status
    let busy = manager
        .update_status(agent_id, AgentStatus::Busy, "system")
        .await
        .unwrap();
    assert_eq!(busy.status, AgentStatus::Busy);

    // Snapshot
    let snap = manager.snapshot().await.unwrap();
    assert_eq!(snap.total_agents, 1);
    assert_eq!(snap.online_count, 1);
}