use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use core_agent_platform::{
    AuditDecision, AuditRecord, GovernanceRequest, PlatformManager, PlatformObserver,
    PlatformPolicy, PlatformStore, PolicyEffect, PolicyRule, Quota, SqlitePlatformStore, Tenant,
    TenantState,
};
use rusqlite::{Connection, OptionalExtension};
use tempfile::tempdir;
use uuid::Uuid;

fn allow_policy(tenant_id: Uuid) -> PlatformPolicy {
    let mut policy = PlatformPolicy::new(tenant_id, "tool-policy", "Tool Policy", "operator");
    policy.rules.push(PolicyRule {
        id: Uuid::new_v4(),
        subjects: BTreeSet::from(["operator".into()]),
        actions: BTreeSet::from(["tool.execute".into()]),
        resources: BTreeSet::from(["builtin.echo".into()]),
        attributes: BTreeMap::new(),
        effect: PolicyEffect::Allow,
        priority: 100,
    });
    policy
}

async fn running_manager() -> (PlatformManager, Tenant) {
    let manager = PlatformManager::builder().build();
    manager.start().unwrap();
    let tenant = manager
        .create_tenant(Tenant::new("acme", "Acme", "operator"))
        .await
        .unwrap();
    (manager, tenant)
}

#[tokio::test]
async fn unmatched_request_is_denied_and_audited_by_default() {
    let (manager, tenant) = running_manager().await;
    let request = GovernanceRequest::new(
        tenant.id,
        "operator",
        "tool.execute",
        "builtin.echo",
        "operator",
    );
    let request_id = request.request_id;

    let decision = manager.govern(request).await.unwrap();

    assert!(!decision.allowed);
    assert!(decision.policy_id.is_none());
    let audits = manager.list_audits(tenant.id).await.unwrap();
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].request_id, request_id);
    assert_eq!(audits[0].decision, AuditDecision::Denied);
}

#[tokio::test]
async fn allow_rule_and_quota_commit_once_for_an_idempotent_request() {
    let (manager, tenant) = running_manager().await;
    manager
        .create_policy(allow_policy(tenant.id))
        .await
        .unwrap();
    let quota = manager
        .create_quota(Quota::new(tenant.id, "tool-calls", 2, 60, "operator").unwrap())
        .await
        .unwrap();
    let mut request = GovernanceRequest::new(
        tenant.id,
        "operator",
        "tool.execute",
        "builtin.echo",
        "operator",
    );
    request.quota_key = Some("tool-calls".into());
    request.units = 1;

    let first = manager.govern(request.clone()).await.unwrap();
    let replay = manager.govern(request).await.unwrap();

    assert!(first.allowed);
    assert_eq!(first, replay);
    let stored = manager
        .list_quotas(tenant.id)
        .await
        .unwrap()
        .into_iter()
        .find(|item| item.id == quota.id)
        .unwrap();
    assert_eq!(stored.consumed, 1);
    assert_eq!(stored.ledger.len(), 1);
    assert_eq!(manager.list_audits(tenant.id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn quota_excess_is_denied_without_mutating_consumption() {
    let (manager, tenant) = running_manager().await;
    manager
        .create_policy(allow_policy(tenant.id))
        .await
        .unwrap();
    manager
        .create_quota(Quota::new(tenant.id, "tool-calls", 1, 60, "operator").unwrap())
        .await
        .unwrap();
    for expected in [true, false] {
        let mut request = GovernanceRequest::new(
            tenant.id,
            "operator",
            "tool.execute",
            "builtin.echo",
            "operator",
        );
        request.quota_key = Some("tool-calls".into());
        request.units = 1;
        assert_eq!(manager.govern(request).await.unwrap().allowed, expected);
    }

    let quota = manager.list_quotas(tenant.id).await.unwrap().remove(0);
    assert_eq!(quota.consumed, 1);
    let audits = manager.list_audits(tenant.id).await.unwrap();
    assert_eq!(audits.len(), 2);
    assert!(audits
        .iter()
        .any(|audit| audit.decision == AuditDecision::QuotaExceeded));
}

#[tokio::test]
async fn suspended_and_cross_tenant_scopes_fail_closed() {
    let manager = PlatformManager::builder().build();
    manager.start().unwrap();
    let first = manager
        .create_tenant(Tenant::new("first", "First", "operator"))
        .await
        .unwrap();
    let second = manager
        .create_tenant(Tenant::new("second", "Second", "operator"))
        .await
        .unwrap();
    let organization = manager
        .create_organization(core_agent_platform::PlatformOrganization::new(
            second.id,
            "engineering",
            "Engineering",
            "operator",
        ))
        .await
        .unwrap();
    let mut cross_tenant = GovernanceRequest::new(
        first.id,
        "operator",
        "tool.execute",
        "builtin.echo",
        "operator",
    );
    cross_tenant.organization_id = Some(organization.id);
    assert!(manager.govern(cross_tenant).await.is_err());

    manager
        .set_tenant_state(first.id, TenantState::Suspended, "operator")
        .await
        .unwrap();
    assert!(manager
        .govern(GovernanceRequest::new(
            first.id,
            "operator",
            "tool.execute",
            "builtin.echo",
            "operator",
        ))
        .await
        .is_err());
}

struct PanickingObserver;

impl PlatformObserver for PanickingObserver {
    fn on_audit(&self, _audit: &AuditRecord) {
        panic!("observer failure")
    }
}

#[tokio::test]
async fn observer_panic_does_not_change_governance_result() {
    let manager = PlatformManager::builder()
        .observer(Arc::new(PanickingObserver))
        .build();
    manager.start().unwrap();
    let tenant = manager
        .create_tenant(Tenant::new("acme", "Acme", "operator"))
        .await
        .unwrap();
    let decision = manager
        .govern(GovernanceRequest::new(
            tenant.id,
            "operator",
            "tool.execute",
            "builtin.echo",
            "operator",
        ))
        .await
        .unwrap();
    assert!(!decision.allowed);
}

#[tokio::test]
async fn sqlite_has_five_audited_tables_recovers_and_detects_tampering() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("platform.db");
    let store = Arc::new(SqlitePlatformStore::new(&path).unwrap());
    let manager = PlatformManager::builder().store(store).build();
    manager.start().unwrap();
    let tenant = manager
        .create_tenant(Tenant::new("acme", "Acme", "operator"))
        .await
        .unwrap();
    manager
        .govern(GovernanceRequest::new(
            tenant.id,
            "operator",
            "tool.execute",
            "builtin.echo",
            "operator",
        ))
        .await
        .unwrap();
    drop(manager);

    let reopened = SqlitePlatformStore::new(&path).unwrap();
    assert_eq!(reopened.list_audits(tenant.id).await.unwrap().len(), 1);
    let connection = Connection::open(&path).unwrap();
    for table in ["tenant", "organization", "policy", "audit", "quota"] {
        let columns = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for required in [
            "id",
            "create_time",
            "update_time",
            "create_user",
            "update_user",
        ] {
            assert!(columns.iter().any(|column| column == required));
        }
    }
    assert_eq!(
        connection
            .query_row("PRAGMA foreign_key_list(tenant)", [], |_| Ok(()))
            .optional()
            .unwrap(),
        None
    );
    connection
        .execute(
            "UPDATE tenant SET state='SUSPENDED' WHERE id=?1",
            [tenant.id.to_string()],
        )
        .unwrap();
    drop(connection);
    assert!(reopened.find_tenant(tenant.id).await.is_err());
}
