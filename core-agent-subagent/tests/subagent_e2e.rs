use std::sync::Arc;

use core_agent_subagent::{
    AgentRole, InMemorySubAgentStore, InstanceType, SubAgentManager, SubAgentObserver,
    SubAgentStatus, SubAgentStore,
};
use serde_json::json;

fn create_manager() -> SubAgentManager {
    SubAgentManager::builder().build()
}

#[tokio::test]
async fn test_create_and_find() {
    let manager = create_manager();
    let instance = manager
        .create(
            "log-agent".into(),
            InstanceType::Worker,
            AgentRole::Researcher,
            None,
            None,
            json!({}),
            "system",
        )
        .await
        .unwrap();
    assert_eq!(instance.name, "log-agent");
    assert_eq!(instance.status, SubAgentStatus::Initialized);

    let found = manager.find(instance.id).await.unwrap().unwrap();
    assert_eq!(found.id, instance.id);
    assert_eq!(found.name, "log-agent");
}

#[tokio::test]
async fn test_lifecycle_transitions() {
    let manager = create_manager();
    let instance = manager
        .create(
            "worker".into(),
            InstanceType::Worker,
            AgentRole::Executor,
            None,
            None,
            json!({}),
            "system",
        )
        .await
        .unwrap();

    let started = manager.start(instance.id, "system").await.unwrap();
    assert_eq!(started.status, SubAgentStatus::Running);

    let stopped = manager.stop(instance.id, "system").await.unwrap();
    assert_eq!(stopped.status, SubAgentStatus::Waiting);

    let destroyed = manager.destroy(instance.id, "system").await.unwrap();
    assert_eq!(destroyed.status, SubAgentStatus::Destroyed);
}

#[tokio::test]
async fn test_invalid_transition() {
    let manager = create_manager();
    let instance = manager
        .create(
            "worker".into(),
            InstanceType::Worker,
            AgentRole::Executor,
            None,
            None,
            json!({}),
            "system",
        )
        .await
        .unwrap();

    // Created -> Completed is invalid (must go through proper lifecycle)
    // We need to use the lifecycle directly; via manager, any -> Destroyed is allowed
    // So test that destroying a Running instance works, but starting a Completed one fails
    manager.start(instance.id, "system").await.unwrap();
    manager.stop(instance.id, "system").await.unwrap();
    // Now at Waiting, can go to Running
    manager.start(instance.id, "system").await.unwrap();
    // Now at Running -> can go to Failed
    // Use the manager's stop to go to Waiting first
    manager.stop(instance.id, "system").await.unwrap();
    // Now at Waiting, can destroy
    manager.destroy(instance.id, "system").await.unwrap();
    // Starting a destroyed instance is invalid
    let result = manager.start(instance.id, "system").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_parent_child_relationship() {
    let manager = create_manager();
    let parent = manager
        .create(
            "supervisor".into(),
            InstanceType::Manager,
            AgentRole::Monitor,
            None,
            None,
            json!({}),
            "system",
        )
        .await
        .unwrap();

    let _child = manager
        .create(
            "worker".into(),
            InstanceType::Worker,
            AgentRole::Executor,
            Some(parent.id),
            Some(parent.id),
            json!({}),
            "system",
        )
        .await
        .unwrap();

    let children = manager.list_by_parent(parent.id).await.unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].name, "worker");
}

#[tokio::test]
async fn test_supervisor_relationship() {
    let manager = create_manager();
    let supervisor = manager
        .create(
            "supervisor".into(),
            InstanceType::Manager,
            AgentRole::Monitor,
            None,
            None,
            json!({}),
            "system",
        )
        .await
        .unwrap();

    let _w1 = manager
        .create(
            "w1".into(), InstanceType::Worker, AgentRole::Researcher,
            Some(supervisor.id), Some(supervisor.id), json!({}), "system",
        )
        .await
        .unwrap();
    let _w2 = manager
        .create(
            "w2".into(), InstanceType::Worker, AgentRole::Researcher,
            Some(supervisor.id), Some(supervisor.id), json!({}), "system",
        )
        .await
        .unwrap();

    let workers = manager.list_by_supervisor(supervisor.id).await.unwrap();
    assert_eq!(workers.len(), 2);
}

#[tokio::test]
async fn test_status_filter() {
    let manager = create_manager();
    let i1 = manager
        .create("a".into(), InstanceType::Worker, AgentRole::Executor, None, None, json!({}), "system")
        .await
        .unwrap();
    let i2 = manager
        .create("b".into(), InstanceType::Worker, AgentRole::Executor, None, None, json!({}), "system")
        .await
        .unwrap();

    manager.start(i1.id, "system").await.unwrap();
    manager.start(i2.id, "system").await.unwrap();

    let running = manager.list_by_status(SubAgentStatus::Running).await.unwrap();
    assert_eq!(running.len(), 2);

    manager.stop(i1.id, "system").await.unwrap();
    let waiting = manager.list_by_status(SubAgentStatus::Waiting).await.unwrap();
    assert_eq!(waiting.len(), 1);
}

#[tokio::test]
async fn test_observer_notification() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingObserver(AtomicUsize);
    impl SubAgentObserver for CountingObserver {
        fn on_observation(&self, _observation: &core_agent_subagent::SubAgentObservation) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    let observer = Arc::new(CountingObserver(AtomicUsize::new(0)));
    let manager = SubAgentManager::builder()
        .observer(observer.clone())
        .build();

    let instance = manager
        .create("test".into(), InstanceType::Worker, AgentRole::Executor, None, None, json!({}), "system")
        .await
        .unwrap();

    assert!(observer.0.load(Ordering::SeqCst) >= 1);
}

#[tokio::test]
async fn test_sqlite_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_subagent.db");
    let store = Arc::new(
        core_agent_subagent::SqliteSubAgentStore::new(&db_path).unwrap(),
    );
    let manager = SubAgentManager::builder().store(store.clone()).build();

    let instance = manager
        .create(
            "persistent-agent".into(),
            InstanceType::Worker,
            AgentRole::Researcher,
            None,
            None,
            json!({"key": "value"}),
            "system",
        )
        .await
        .unwrap();

    // Re-open with a new store, import SubAgentStore trait
    let store2 = Arc::new(
        core_agent_subagent::SqliteSubAgentStore::new(&db_path).unwrap(),
    );
    use core_agent_subagent::SubAgentStore as _;
    let found = store2.find(instance.id).await.unwrap().unwrap();
    assert_eq!(found.name, "persistent-agent");
    assert_eq!(found.role, AgentRole::Researcher);
    assert_eq!(found.config, json!({"key": "value"}));
}