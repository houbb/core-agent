use std::sync::Arc;

use core_agent_autonomous::{
    AutonomyLevel, AutonomousManager, AutonomousQuery, InMemoryAutonomousStore,
};
use uuid::Uuid;

#[tokio::test]
async fn autonomous_e2e_full_lifecycle() {
    let store = Arc::new(InMemoryAutonomousStore::default());
    let manager = AutonomousManager::new(store);

    let agent_id = Uuid::new_v4();

    // Create a goal
    let goal = manager
        .create_goal(
            agent_id,
            "Keep system SLA above 99.9%",
            5,
            AutonomyLevel::L2AutoExecuteLowRisk,
            "system",
        )
        .await
        .unwrap();
    assert_eq!(goal.priority, 5);
    assert!(goal.active);

    // Start autonomous loop
    let loop_state = manager
        .start_loop(agent_id, AutonomyLevel::L2AutoExecuteLowRisk, "system")
        .await
        .unwrap();
    assert_eq!(loop_state.status.as_str(), "OBSERVING");

    // Advance through all cycles
    let state = manager.advance_cycle(agent_id, "system").await.unwrap();
    assert_eq!(state.current_cycle, 1);
    assert_eq!(state.status.as_str(), "ANALYZING");

    // Multiple cycles
    for _ in 0..5 {
        manager.advance_cycle(agent_id, "system").await.unwrap();
    }
    let final_state = manager.find_loop(agent_id).await.unwrap().unwrap();
    assert_eq!(final_state.current_cycle, 6);

    // Pause
    let paused = manager.pause_loop(agent_id, "system").await.unwrap();
    assert_eq!(paused.status.as_str(), "IDLE");

    // Snapshot
    let snap = manager.snapshot(agent_id).await.unwrap();
    assert_eq!(snap.active_goals, 1);
    assert_eq!(snap.total_cycles, 6);
}

#[tokio::test]
async fn autonomous_e2e_multi_level_goals() {
    let store = Arc::new(InMemoryAutonomousStore::default());
    let manager = AutonomousManager::new(store);

    let agent_id = Uuid::new_v4();

    // Goals at different autonomy levels
    manager
        .create_goal(
            agent_id,
            "Suggest improvements",
            1,
            AutonomyLevel::L0Suggest,
            "system",
        )
        .await
        .unwrap();
    manager
        .create_goal(
            agent_id,
            "Auto analyze logs",
            3,
            AutonomyLevel::L1AutoAnalyze,
            "system",
        )
        .await
        .unwrap();
    manager
        .create_goal(
            agent_id,
            "Auto fix low risk issues",
            5,
            AutonomyLevel::L2AutoExecuteLowRisk,
            "system",
        )
        .await
        .unwrap();

    let goals = manager
        .list_goals(&AutonomousQuery {
            agent_id: Some(agent_id),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(goals.len(), 3);
}