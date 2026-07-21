use std::sync::Arc;

use core_agent_orchestrator::{
    AggregatedResult, AgentInstanceRef, OrchestrationStatus, OrchestrationStrategy,
    OrchestratorManager, SqliteOrchestrationStore, OrchestrationStore,
};
use core_agent_subagent::AgentRole;
use uuid::Uuid;

fn create_manager() -> OrchestratorManager {
    OrchestratorManager::builder().build()
}

#[tokio::test]
async fn test_create_orchestration() {
    let manager = create_manager();
    let orch = manager
        .create(
            "analyze outage".into(),
            OrchestrationStrategy::Supervisor,
            Uuid::new_v4(),
            "system",
        )
        .await
        .unwrap();
    assert_eq!(orch.status, OrchestrationStatus::Created);
    assert_eq!(orch.strategy, OrchestrationStrategy::Supervisor);
}

#[tokio::test]
async fn test_add_worker() {
    let manager = create_manager();
    let mut orch = manager
        .create(
            "test".into(),
            OrchestrationStrategy::Parallel,
            Uuid::new_v4(),
            "system",
        )
        .await
        .unwrap();

    let worker = AgentInstanceRef {
        agent_id: Uuid::new_v4(),
        agent_name: "worker-1".into(),
        role: AgentRole::Researcher,
    };
    orch = manager.add_worker(orch.id, worker, "system").await.unwrap();
    assert_eq!(orch.worker_agents.len(), 1);
    assert_eq!(orch.worker_agents[0].agent_name, "worker-1");
}

#[tokio::test]
async fn test_sequential_strategy() {
    let manager = create_manager();
    let mut orch = manager
        .create(
            "sequential test".into(),
            OrchestrationStrategy::Sequential,
            Uuid::new_v4(),
            "system",
        )
        .await
        .unwrap();

    for i in 0..3 {
        orch = manager
            .add_worker(
                orch.id,
                AgentInstanceRef {
                    agent_id: Uuid::new_v4(),
                    agent_name: format!("worker-{i}"),
                    role: AgentRole::Executor,
                },
                "system",
            )
            .await
            .unwrap();
    }

    let completed = manager.start(orch.id, "system").await.unwrap();
    assert_eq!(completed.status, OrchestrationStatus::Completed);
    assert!(completed.result.is_some());
    let result = completed.result.unwrap();
    assert_eq!(result.details.len(), 3);
    assert!(result.confidence > 0.0);
}

#[tokio::test]
async fn test_parallel_strategy() {
    let manager = create_manager();
    let mut orch = manager
        .create(
            "parallel test".into(),
            OrchestrationStrategy::Parallel,
            Uuid::new_v4(),
            "system",
        )
        .await
        .unwrap();

    orch = manager
        .add_worker(
            orch.id,
            AgentInstanceRef {
                agent_id: Uuid::new_v4(),
                agent_name: "p1".into(),
                role: AgentRole::Researcher,
            },
            "system",
        )
        .await
        .unwrap();

    let completed = manager.start(orch.id, "system").await.unwrap();
    assert_eq!(completed.status, OrchestrationStatus::Completed);
    assert!(completed.result.unwrap().confidence > 0.0);
}

#[tokio::test]
async fn test_supervisor_strategy() {
    let manager = create_manager();
    let supervisor_id = Uuid::new_v4();

    // Use supervise convenience method
    let result = manager
        .supervise(
            "root cause analysis".into(),
            vec![
                ("Log-Agent".into(), AgentRole::Researcher),
                ("Metric-Agent".into(), AgentRole::Researcher),
                ("Trace-Agent".into(), AgentRole::Researcher),
            ],
            supervisor_id,
            "system",
        )
        .await
        .unwrap();

    assert_eq!(result.details.len(), 3);
    assert!(result.confidence > 0.0);
    assert!(result.summary.contains("Root Cause"));
}

#[tokio::test]
async fn test_result_aggregation() {
    let manager = create_manager();
    let orch = manager
        .create(
            "aggregation test".into(),
            OrchestrationStrategy::Sequential,
            Uuid::new_v4(),
            "system",
        )
        .await
        .unwrap();

    let orch = manager
        .add_worker(
            orch.id,
            AgentInstanceRef {
                agent_id: Uuid::new_v4(),
                agent_name: "w1".into(),
                role: AgentRole::Researcher,
            },
            "system",
        )
        .await
        .unwrap();

    let completed = manager.start(orch.id, "system").await.unwrap();
    assert!(completed.result.is_some());
}

#[tokio::test]
async fn test_sqlite_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test_orchestration.db");
    let store = Arc::new(SqliteOrchestrationStore::new(&db_path).unwrap());
    let manager = OrchestratorManager::builder().store(store).build();

    let orch = manager
        .create(
            "persist test".into(),
            OrchestrationStrategy::Sequential,
            Uuid::new_v4(),
            "system",
        )
        .await
        .unwrap();

    let store2 = Arc::new(SqliteOrchestrationStore::new(&db_path).unwrap());
    let found = store2.find(orch.id).await.unwrap().unwrap();
    assert_eq!(found.goal, "persist test");
}

#[tokio::test]
async fn test_supervise_rca_via_manager() {
    // This is the RCA demo integration test
    let manager = create_manager();
    let supervisor = Uuid::new_v4();

    let result = manager
        .supervise(
            "订单服务 500 错误".into(),
            vec![
                ("Log-Agent".into(), AgentRole::Researcher),
                ("Metric-Agent".into(), AgentRole::Researcher),
                ("Trace-Agent".into(), AgentRole::Researcher),
            ],
            supervisor,
            "system",
        )
        .await
        .unwrap();

    assert_eq!(result.details.len(), 3);
    assert!(result.summary.contains("Root Cause"));
    assert!(result.confidence > 0.0);

    // Verify each agent had a finding
    let agent_names: Vec<&str> = result.details.iter().map(|d| d.agent_name.as_str()).collect();
    assert!(agent_names.contains(&"Log-Agent"));
    assert!(agent_names.contains(&"Metric-Agent"));
    assert!(agent_names.contains(&"Trace-Agent"));
}

#[tokio::test]
async fn test_status_transitions() {
    let manager = create_manager();
    let orch = manager
        .create(
            "transition test".into(),
            OrchestrationStrategy::Sequential,
            Uuid::new_v4(),
            "system",
        )
        .await
        .unwrap();

    // Can't add workers after Running
    let orch = manager
        .add_worker(
            orch.id,
            AgentInstanceRef {
                agent_id: Uuid::new_v4(),
                agent_name: "w".into(),
                role: AgentRole::Executor,
            },
            "system",
        )
        .await
        .unwrap();

    let completed = manager.start(orch.id, "system").await.unwrap();
    assert_eq!(completed.status, OrchestrationStatus::Completed);
}