use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use core_agent_audit::{
    AuditEvent, AuditEventType, AuditManager, AuditQuery, AuditSeverity, AuditStore, SqliteAuditStore,
};
use core_agent_approval::{
    ApprovalManager, ApprovalRequest, ApprovalState, ApprovalType, RiskLevel, RiskRule, SqliteApprovalStore,
};
use core_agent_cost::{
    Budget, BudgetScope, CostManager, CostRecord, CostStore, SqliteCostStore,
};

/// Integration test verifying that all three P4 governance modules
/// work together in a shared enterprise scenario.
#[tokio::test]
async fn audit_approval_cost_share_enterprise_tenant() {
    let dir = tempfile::TempDir::new().unwrap();

    // Initialize all three stores with SQLite persistence
    let audit_store = Arc::new(SqliteAuditStore::new(dir.path().join("audit.db")).unwrap());
    let approval_store = Arc::new(SqliteApprovalStore::new(dir.path().join("approval.db")).unwrap());
    let cost_store = Arc::new(SqliteCostStore::new(dir.path().join("cost.db")).unwrap());

    let audit = AuditManager::new(audit_store);
    let approval = ApprovalManager::new(approval_store);
    let cost = CostManager::new(cost_store);

    let tenant = Uuid::new_v4();
    let agent_id = Uuid::new_v4();
    let now = Utc::now();

    // 1. Record an audit event for agent creation
    let audit_event = AuditEvent::new(tenant, "admin", AuditEventType::AgentCreated, "agent.create", "rca-agent")
        .with_severity(AuditSeverity::Info)
        .with_result("success");
    audit.record(&audit_event, "admin").await.unwrap();

    // 2. Set a budget for the agent
    let budget = Budget::new(tenant, BudgetScope::Agent, agent_id.to_string(), 100_000, "admin");
    cost.set_budget(&budget, "admin").await.unwrap();

    // 3. Record a cost event
    let cost_record = CostRecord::new(tenant, "cost-evt-001", "USD", 5000, "rca-agent")
        .with_agent(agent_id)
        .with_model("gpt-5");
    cost.record_cost(&cost_record, "system").await.unwrap();

    // 4. Create an approval request for a high-risk action
    let risk_rule = RiskRule::new(tenant, "delete.*", "production/*", RiskLevel::Critical, "admin");
    approval.upsert_risk_rule(&risk_rule, "admin").await.unwrap();

    let rules = approval.list_risk_rules(tenant).await.unwrap();
    let risk = approval.evaluate_risk("delete.pod", "production/app", &rules);
    assert_eq!(risk, RiskLevel::Critical);

    let req = ApprovalRequest::new(
        tenant, ApprovalType::Tool, "operator", "delete.pod", "production/app", risk,
    );
    let created = approval.request(req, "operator").await.unwrap();
    assert_eq!(created.state, ApprovalState::Pending);

    // 5. Approve and execute
    let approved = approval.approve(created.id, Uuid::new_v4(), "Approved", "manager").await.unwrap();
    assert_eq!(approved.state, ApprovalState::Approved);

    let executed = approval.execute(approved.id, "operator").await.unwrap();
    assert_eq!(executed.state, ApprovalState::Executed);

    // 6. Record audit for the approval action
    let audit_event2 = AuditEvent::new(tenant, "manager", AuditEventType::Approval, "approval.approve", "delete.pod")
        .with_severity(AuditSeverity::Info)
        .with_result("approved");
    audit.record(&audit_event2, "manager").await.unwrap();

    // 7. Verify audit trail
    let audit_query = AuditQuery {
        tenant_id: Some(tenant),
        event_type: Some(AuditEventType::Approval),
        limit: 100,
        ..Default::default()
    };
    let approval_events = audit.list(&audit_query).await.unwrap();
    assert_eq!(approval_events.len(), 1);
    assert_eq!(approval_events[0].actor, "manager");

    // 8. Verify cost aggregation
    let from = now - chrono::Duration::hours(1);
    let to = now + chrono::Duration::hours(1);
    let summary = cost.aggregate(tenant, from, to).await.unwrap();
    assert_eq!(summary.record_count, 1);
    assert_eq!(summary.total_amount_micros, 5000);

    // 9. Verify budget usage updated
    let found_budget = cost.find_budget(BudgetScope::Agent, &agent_id.to_string()).await.unwrap().unwrap();
    assert_eq!(found_budget.monthly_used_micros, 5000);

    // 10. Audit snapshot
    let snap = audit.snapshot(tenant).await.unwrap();
    assert_eq!(snap.total_events, 2);
}

#[tokio::test]
async fn governance_denies_budget_exceeded() {
    let dir = tempfile::TempDir::new().unwrap();
    let cost_store = Arc::new(SqliteCostStore::new(dir.path().join("cost.db")).unwrap());
    let cost = CostManager::new(cost_store);
    let tenant = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    // Set a very small budget
    let budget = Budget::new(tenant, BudgetScope::Agent, agent_id.to_string(), 1000, "admin");
    cost.set_budget(&budget, "admin").await.unwrap();

    // Record a cost within budget
    let r1 = CostRecord::new(tenant, "evt-1", "USD", 500, "agent").with_agent(agent_id);
    cost.record_cost(&r1, "system").await.unwrap();

    // Record another that exceeds budget
    let r2 = CostRecord::new(tenant, "evt-2", "USD", 600, "agent").with_agent(agent_id);
    let result = cost.record_cost(&r2, "system").await;
    assert!(result.is_err(), "should exceed budget");
}

#[tokio::test]
async fn audit_approval_rejection_chain() {
    let dir = tempfile::TempDir::new().unwrap();
    let audit_store = Arc::new(SqliteAuditStore::new(dir.path().join("audit.db")).unwrap());
    let approval_store = Arc::new(SqliteApprovalStore::new(dir.path().join("approval.db")).unwrap());

    let audit = AuditManager::new(audit_store);
    let approval = ApprovalManager::new(approval_store);
    let tenant = Uuid::new_v4();

    // Create approval request
    let req = ApprovalRequest::new(tenant, ApprovalType::Data, "analyst", "db.query", "production/users", RiskLevel::High);
    let created = approval.request(req, "analyst").await.unwrap();

    // Record audit before rejection
    let audit_event = AuditEvent::new(tenant, "analyst", AuditEventType::PermissionDeny, "approval.request", "production/users")
        .with_severity(AuditSeverity::Warning)
        .with_result("pending_review");
    audit.record(&audit_event, "analyst").await.unwrap();

    // Reject
    let rejected = approval.reject(created.id, Uuid::new_v4(), "No access for this role", "security").await.unwrap();
    assert_eq!(rejected.state, ApprovalState::Rejected);

    // Record audit after rejection
    let audit_event2 = AuditEvent::new(tenant, "security", AuditEventType::PermissionDeny, "approval.reject", "production/users")
        .with_severity(AuditSeverity::Warning)
        .with_result("rejected");
    audit.record(&audit_event2, "security").await.unwrap();

    // Verify audit trail
    let snap = audit.snapshot(tenant).await.unwrap();
    assert_eq!(snap.total_events, 2);
    assert_eq!(*snap.by_event_type.get("PERMISSION_DENY").unwrap(), 2);
}