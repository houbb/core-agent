use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use core_agent_cost::{
    Budget, BudgetScope, CostManager, CostRecord, CostSummary,
};

#[tokio::test]
async fn cost_record_and_aggregate() {
    let manager = CostManager::builder().build();
    let tenant = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let now = Utc::now();

    let record = CostRecord::new(tenant, "evt-001", "USD", 5000, "rca-agent")
        .with_agent(agent_id)
        .with_model("gpt-5");
    manager.record_cost(&record, "system").await.unwrap();

    let record2 = CostRecord::new(tenant, "evt-002", "USD", 3000, "rca-agent")
        .with_agent(agent_id)
        .with_model("gpt-5");
    manager.record_cost(&record2, "system").await.unwrap();

    let from = now - chrono::Duration::hours(1);
    let to = now + chrono::Duration::hours(1);

    let summary = manager.aggregate(tenant, from, to).await.unwrap();
    assert_eq!(summary.record_count, 2);
    assert_eq!(summary.total_amount_micros, 8000);
    assert_eq!(summary.total_input_tokens, 0);
    assert_eq!(summary.total_output_tokens, 0);
}

#[tokio::test]
async fn cost_idempotency() {
    let manager = CostManager::builder().build();
    let tenant = Uuid::new_v4();
    let record = CostRecord::new(tenant, "evt-001", "USD", 1000, "agent");
    manager.record_cost(&record, "system").await.unwrap();
    let result = manager.record_cost(&record, "system").await;
    assert!(result.is_err(), "duplicate event key should be rejected");
}

#[tokio::test]
async fn budget_enforcement() {
    let manager = CostManager::builder().build();
    let tenant = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    let budget = Budget::new(tenant, BudgetScope::Agent, agent_id.to_string(), 5000, "admin");
    manager.set_budget(&budget, "admin").await.unwrap();

    let record = CostRecord::new(tenant, "evt-001", "USD", 3000, "agent")
        .with_agent(agent_id);
    manager.record_cost(&record, "system").await.unwrap();

    let record2 = CostRecord::new(tenant, "evt-002", "USD", 3000, "agent")
        .with_agent(agent_id);
    let result = manager.record_cost(&record2, "system").await;
    assert!(result.is_err(), "should exceed budget");
}

#[tokio::test]
async fn budget_aggregation_by_agent() {
    let manager = CostManager::builder().build();
    let tenant = Uuid::new_v4();
    let agent_a = Uuid::new_v4();
    let agent_b = Uuid::new_v4();
    let now = Utc::now();

    for i in 0..3 {
        let r = CostRecord::new(tenant, &format!("a-evt-{i}"), "USD", 1000, "agent")
            .with_agent(agent_a);
        manager.record_cost(&r, "system").await.unwrap();
    }
    for i in 0..2 {
        let r = CostRecord::new(tenant, &format!("b-evt-{i}"), "USD", 2000, "agent")
            .with_agent(agent_b);
        manager.record_cost(&r, "system").await.unwrap();
    }

    let from = now - chrono::Duration::hours(1);
    let to = now + chrono::Duration::hours(1);

    let a_summary = manager.aggregate_by_agent(agent_a, from, to).await.unwrap();
    assert_eq!(a_summary.record_count, 3);
    assert_eq!(a_summary.total_amount_micros, 3000);

    let all_summary = manager.aggregate(tenant, from, to).await.unwrap();
    assert_eq!(all_summary.record_count, 5);
    assert_eq!(all_summary.total_amount_micros, 7000);
}

#[tokio::test]
async fn cost_sqlite_persistence() {
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("cost.db");
    let store = Arc::new(core_agent_cost::SqliteCostStore::new(&db_path).unwrap());
    let manager = CostManager::new(store);
    let tenant = Uuid::new_v4();

    let record = CostRecord::new(tenant, "evt-001", "USD", 5000, "agent")
        .with_tokens(100, 20, 5);
    manager.record_cost(&record, "system").await.unwrap();

    let found = manager.find(record.id).await.unwrap().unwrap();
    assert_eq!(found.id, record.id);

    // Re-open and verify persistence
    let store2 = Arc::new(core_agent_cost::SqliteCostStore::new(&db_path).unwrap());
    let manager2 = CostManager::new(store2);
    let found2 = manager2.find(record.id).await.unwrap().unwrap();
    assert_eq!(found2.id, record.id);
    assert_eq!(found2.amount_micros, 600); // (100 + 20) * 5
}

#[tokio::test]
async fn cost_budget_persistence() {
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("cost.db");
    let store = Arc::new(core_agent_cost::SqliteCostStore::new(&db_path).unwrap());
    let manager = CostManager::new(store);
    let tenant = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    let budget = Budget::new(tenant, BudgetScope::Agent, agent_id.to_string(), 10000, "admin");
    manager.set_budget(&budget, "admin").await.unwrap();

    let found = manager.find_budget(BudgetScope::Agent, &agent_id.to_string()).await.unwrap().unwrap();
    assert_eq!(found.monthly_limit_micros, 10000);

    // Re-open
    let store2 = Arc::new(core_agent_cost::SqliteCostStore::new(&db_path).unwrap());
    let manager2 = CostManager::new(store2);
    let found2 = manager2.find_budget(BudgetScope::Agent, &agent_id.to_string()).await.unwrap().unwrap();
    assert_eq!(found2.monthly_limit_micros, 10000);
}