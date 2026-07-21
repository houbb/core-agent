use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{ApprovalRequest, ApprovalState, RiskLevel, RiskRule};
use crate::error::ApprovalResult;

#[async_trait]
pub trait ApprovalStore: Send + Sync {
    async fn create_request(&self, request: &ApprovalRequest, actor: &str) -> ApprovalResult<()>;
    async fn update_state(
        &self,
        id: Uuid,
        state: ApprovalState,
        expected_version: u64,
        actor: &str,
    ) -> ApprovalResult<()>;
    async fn add_decision(&self, request_id: Uuid, decision: &super::domain::ApprovalDecision, actor: &str) -> ApprovalResult<()>;
    async fn find_request(&self, id: Uuid) -> ApprovalResult<Option<ApprovalRequest>>;
    async fn list_pending(&self, tenant_id: Uuid) -> ApprovalResult<Vec<ApprovalRequest>>;
    async fn list_by_requester(&self, tenant_id: Uuid, requester: &str) -> ApprovalResult<Vec<ApprovalRequest>>;
    async fn expire_pending(&self) -> ApprovalResult<u64>;
    async fn list_risk_rules(&self, tenant_id: Uuid) -> ApprovalResult<Vec<RiskRule>>;
    async fn upsert_risk_rule(&self, rule: &RiskRule, actor: &str) -> ApprovalResult<()>;
}

pub trait RiskEngine: Send + Sync {
    fn evaluate(&self, action: &str, resource: &str, risk_rules: &[RiskRule]) -> RiskLevel;
}

pub trait ApprovalNotifier: Send + Sync {
    fn notify_pending(&self, request: &ApprovalRequest);
    fn notify_decided(&self, request: &ApprovalRequest, decision: &super::domain::ApprovalDecision);
}