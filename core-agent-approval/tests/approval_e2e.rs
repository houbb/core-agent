use std::sync::Arc;

use uuid::Uuid;

use core_agent_approval::{
    ApprovalManager, ApprovalRequest, ApprovalState, ApprovalType, RiskLevel, RiskRule,
};

#[tokio::test]
async fn approval_full_lifecycle() {
    let manager = ApprovalManager::builder().build();
    let tenant = Uuid::new_v4();

    // Create a risk rule for production
    let rule = RiskRule::new(tenant, "delete.*", "production/*", RiskLevel::Critical, "admin");
    manager.upsert_risk_rule(&rule, "admin").await.unwrap();

    // Evaluate risk
    let rules = manager.list_risk_rules(tenant).await.unwrap();
    let risk = manager.evaluate_risk("delete.pod", "production/app", &rules);
    assert_eq!(risk, RiskLevel::Critical);

    // Create approval request
    let req = ApprovalRequest::new(
        tenant,
        ApprovalType::Tool,
        "operator",
        "delete.pod",
        "production/app",
        risk,
    );
    let created = manager.request(req, "operator").await.unwrap();
    assert_eq!(created.state, ApprovalState::Pending);

    // Approve
    let approved = manager.approve(created.id, Uuid::new_v4(), "Approved for maintenance", "manager").await.unwrap();
    assert_eq!(approved.state, ApprovalState::Approved);

    // Execute
    let executed = manager.execute(approved.id, "operator").await.unwrap();
    assert_eq!(executed.state, ApprovalState::Executed);
}

#[tokio::test]
async fn approval_rejection_flow() {
    let manager = ApprovalManager::builder().build();
    let tenant = Uuid::new_v4();
    let req = ApprovalRequest::new(
        tenant,
        ApprovalType::Data,
        "analyst",
        "db.query",
        "production/users",
        RiskLevel::High,
    );
    let created = manager.request(req, "analyst").await.unwrap();
    let rejected = manager.reject(created.id, Uuid::new_v4(), "Access denied", "security").await.unwrap();
    assert_eq!(rejected.state, ApprovalState::Rejected);
}

#[tokio::test]
async fn approval_list_pending() {
    let manager = ApprovalManager::builder().build();
    let tenant = Uuid::new_v4();
    let other_tenant = Uuid::new_v4();

    for i in 0..3 {
        let req = ApprovalRequest::new(
            tenant,
            ApprovalType::Tool,
            "operator",
            &format!("action{i}"),
            "resource",
            RiskLevel::Low,
        );
        manager.request(req, "operator").await.unwrap();
    }
    let req = ApprovalRequest::new(
        other_tenant,
        ApprovalType::Tool,
        "operator",
        "other",
        "resource",
        RiskLevel::Low,
    );
    manager.request(req, "operator").await.unwrap();

    let pending = manager.list_pending(tenant).await.unwrap();
    assert_eq!(pending.len(), 3);

    let by_requester = manager.list_by_requester(tenant, "operator").await.unwrap();
    assert_eq!(by_requester.len(), 3);
}

#[tokio::test]
async fn approval_risk_engine() {
    let manager = ApprovalManager::builder().build();
    let tenant = Uuid::new_v4();
    let rules = vec![
        RiskRule::new(tenant, "read.*", "production/*", RiskLevel::Medium, "admin"),
        RiskRule::new(tenant, "delete.*", "production/*", RiskLevel::Critical, "admin"),
        RiskRule::new(tenant, "read.*", "staging/*", RiskLevel::Low, "admin"),
    ];

    assert_eq!(manager.evaluate_risk("read.logs", "production/app", &rules), RiskLevel::Medium);
    assert_eq!(manager.evaluate_risk("delete.pod", "production/app", &rules), RiskLevel::Critical);
    assert_eq!(manager.evaluate_risk("read.logs", "staging/app", &rules), RiskLevel::Low);
    assert_eq!(manager.evaluate_risk("unknown", "development/app", &rules), RiskLevel::Low);
}

#[tokio::test]
async fn approval_sqlite_persistence() {
    let dir = tempfile::TempDir::new().unwrap();
    let db_path = dir.path().join("approval.db");
    let store = Arc::new(core_agent_approval::SqliteApprovalStore::new(&db_path).unwrap());
    let manager = ApprovalManager::new(store);
    let tenant = Uuid::new_v4();

    let req = ApprovalRequest::new(
        tenant,
        ApprovalType::Workflow,
        "designer",
        "deploy",
        "production/workflow",
        RiskLevel::High,
    );
    let created = manager.request(req, "designer").await.unwrap();
    let found = manager.find(created.id).await.unwrap().unwrap();
    assert_eq!(found.id, created.id);
    assert_eq!(found.state, ApprovalState::Pending);

    // Re-open and verify persistence
    let store2 = Arc::new(core_agent_approval::SqliteApprovalStore::new(&db_path).unwrap());
    let manager2 = ApprovalManager::new(store2);
    let found2 = manager2.find(created.id).await.unwrap().unwrap();
    assert_eq!(found2.id, created.id);
    assert_eq!(found2.requester, "designer");
}