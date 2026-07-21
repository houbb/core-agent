use std::sync::Arc;

use uuid::Uuid;

use crate::defaults::{InMemoryApprovalStore, DefaultRiskEngine, NoopApprovalNotifier};
use crate::domain::{
    ApprovalDecision, ApprovalRequest, ApprovalState, RiskLevel, RiskRule,
    validate_actor,
};
use crate::error::{ApprovalError, ApprovalResult};
use crate::infrastructure::{ApprovalNotifier, ApprovalStore, RiskEngine};

pub struct ApprovalManagerBuilder {
    store: Arc<dyn ApprovalStore>,
    risk_engine: Arc<dyn RiskEngine>,
    notifier: Arc<dyn ApprovalNotifier>,
}

impl Default for ApprovalManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryApprovalStore::default()),
            risk_engine: Arc::new(DefaultRiskEngine),
            notifier: Arc::new(NoopApprovalNotifier),
        }
    }
}

impl ApprovalManagerBuilder {
    pub fn store(mut self, value: Arc<dyn ApprovalStore>) -> Self {
        self.store = value;
        self
    }

    pub fn risk_engine(mut self, value: Arc<dyn RiskEngine>) -> Self {
        self.risk_engine = value;
        self
    }

    pub fn notifier(mut self, value: Arc<dyn ApprovalNotifier>) -> Self {
        self.notifier = value;
        self
    }

    pub fn build(self) -> ApprovalManager {
        ApprovalManager {
            store: self.store,
            risk_engine: self.risk_engine,
            notifier: self.notifier,
        }
    }
}

pub struct ApprovalManager {
    store: Arc<dyn ApprovalStore>,
    risk_engine: Arc<dyn RiskEngine>,
    notifier: Arc<dyn ApprovalNotifier>,
}

impl ApprovalManager {
    pub fn builder() -> ApprovalManagerBuilder {
        ApprovalManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn ApprovalStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn request(
        &self,
        mut request: ApprovalRequest,
        actor: &str,
    ) -> ApprovalResult<ApprovalRequest> {
        validate_actor("approval requester", actor)?;
        request.actor = actor.into();
        request.validate()?;
        self.store.create_request(&request, actor).await?;
        self.notifier.notify_pending(&request);
        Ok(request)
    }

    pub async fn approve(
        &self,
        id: Uuid,
        principal_id: Uuid,
        comment: &str,
        actor: &str,
    ) -> ApprovalResult<ApprovalRequest> {
        validate_actor("approval approver", actor)?;
        let current = self.required_request(id).await?;
        if current.requester == actor {
            return Err(ApprovalError::Denied(
                "requester cannot self-approve".into(),
            ));
        }
        if current.state != ApprovalState::Pending {
            return Err(ApprovalError::Conflict(format!(
                "approval request is not pending: {}",
                current.state.as_str()
            )));
        }
        let decision = ApprovalDecision {
            id: Uuid::new_v4(),
            request_id: id,
            principal_id,
            decision: ApprovalState::Approved,
            comment: comment.into(),
            decided_at: chrono::Utc::now(),
            actor: actor.into(),
        };
        decision.validate()?;
        self.store.add_decision(id, &decision, actor).await?;
        let updated = self.required_request(id).await?;
        if updated.state == ApprovalState::Approved {
            self.notifier.notify_decided(&updated, &decision);
        }
        Ok(updated)
    }

    pub async fn reject(
        &self,
        id: Uuid,
        principal_id: Uuid,
        comment: &str,
        actor: &str,
    ) -> ApprovalResult<ApprovalRequest> {
        validate_actor("approval rejecter", actor)?;
        let current = self.required_request(id).await?;
        if current.state != ApprovalState::Pending {
            return Err(ApprovalError::Conflict(format!(
                "approval request is not pending: {}",
                current.state.as_str()
            )));
        }
        let decision = ApprovalDecision {
            id: Uuid::new_v4(),
            request_id: id,
            principal_id,
            decision: ApprovalState::Rejected,
            comment: comment.into(),
            decided_at: chrono::Utc::now(),
            actor: actor.into(),
        };
        decision.validate()?;
        self.store.add_decision(id, &decision, actor).await?;
        // Force reject state regardless of required_approvals count
        self.store.update_state(id, ApprovalState::Rejected, current.version.saturating_add(1), actor).await?;
        let updated = self.required_request(id).await?;
        self.notifier.notify_decided(&updated, &decision);
        Ok(updated)
    }

    pub async fn execute(&self, id: Uuid, actor: &str) -> ApprovalResult<ApprovalRequest> {
        validate_actor("approval executor", actor)?;
        let current = self.required_request(id).await?;
        if current.state != ApprovalState::Approved {
            return Err(ApprovalError::Conflict(format!(
                "approval request is not approved: {}",
                current.state.as_str()
            )));
        }
        self.store
            .update_state(id, ApprovalState::Executed, current.version, actor)
            .await?;
        self.required_request(id).await
    }

    pub async fn find(&self, id: Uuid) -> ApprovalResult<Option<ApprovalRequest>> {
        self.store.find_request(id).await
    }

    pub async fn list_pending(&self, tenant_id: Uuid) -> ApprovalResult<Vec<ApprovalRequest>> {
        self.store.list_pending(tenant_id).await
    }

    pub async fn list_by_requester(
        &self,
        tenant_id: Uuid,
        requester: &str,
    ) -> ApprovalResult<Vec<ApprovalRequest>> {
        self.store.list_by_requester(tenant_id, requester).await
    }

    pub async fn expire_pending(&self) -> ApprovalResult<u64> {
        self.store.expire_pending().await
    }

    pub fn evaluate_risk(
        &self,
        action: &str,
        resource: &str,
        risk_rules: &[RiskRule],
    ) -> RiskLevel {
        self.risk_engine.evaluate(action, resource, risk_rules)
    }

    pub async fn list_risk_rules(&self, tenant_id: Uuid) -> ApprovalResult<Vec<RiskRule>> {
        self.store.list_risk_rules(tenant_id).await
    }

    pub async fn upsert_risk_rule(&self, rule: &RiskRule, actor: &str) -> ApprovalResult<()> {
        self.store.upsert_risk_rule(rule, actor).await
    }

    async fn required_request(&self, id: Uuid) -> ApprovalResult<ApprovalRequest> {
        self.store
            .find_request(id)
            .await?
            .ok_or_else(|| ApprovalError::NotFound(id.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn request_approve_execute_flow() {
        let manager = ApprovalManager::builder().build();
        let tenant = Uuid::new_v4();
        let requester = "operator";
        let approver = "manager";

        let req = ApprovalRequest::new(
            tenant,
            crate::ApprovalType::Tool,
            requester,
            "kubectl.delete",
            "production/pod",
            RiskLevel::High,
        );
        let created = manager.request(req, requester).await.unwrap();
        assert_eq!(created.state, ApprovalState::Pending);

        let approved = manager.approve(created.id, Uuid::new_v4(), "Approved for maintenance", approver).await.unwrap();
        assert_eq!(approved.state, ApprovalState::Approved);

        let executed = manager.execute(approved.id, requester).await.unwrap();
        assert_eq!(executed.state, ApprovalState::Executed);
    }

    #[tokio::test]
    async fn self_approve_is_denied() {
        let manager = ApprovalManager::builder().build();
        let tenant = Uuid::new_v4();
        let req = ApprovalRequest::new(
            tenant,
            crate::ApprovalType::Tool,
            "operator",
            "action",
            "resource",
            RiskLevel::Low,
        );
        let created = manager.request(req, "operator").await.unwrap();
        let result = manager.approve(created.id, Uuid::new_v4(), "", "operator").await;
        assert!(matches!(result, Err(ApprovalError::Denied(_))));
    }

    #[tokio::test]
    async fn reject_terminates_approval() {
        let manager = ApprovalManager::builder().build();
        let tenant = Uuid::new_v4();
        let req = ApprovalRequest::new(
            tenant,
            crate::ApprovalType::Data,
            "analyst",
            "db.query",
            "production/users",
            RiskLevel::Critical,
        );
        let created = manager.request(req, "analyst").await.unwrap();
        let rejected = manager.reject(created.id, Uuid::new_v4(), "No access", "security").await.unwrap();
        assert_eq!(rejected.state, ApprovalState::Rejected);
    }

    #[tokio::test]
    async fn risk_engine_defaults() {
        let engine = DefaultRiskEngine;
        let rules = vec![
            RiskRule::new(Uuid::new_v4(), "delete.*", "production/*", RiskLevel::Critical, "admin"),
            RiskRule::new(Uuid::new_v4(), "read.*", "production/*", RiskLevel::Medium, "admin"),
        ];
        assert_eq!(engine.evaluate("delete.pod", "production/app", &rules), RiskLevel::Critical);
        assert_eq!(engine.evaluate("read.logs", "production/app", &rules), RiskLevel::Medium);
        assert_eq!(engine.evaluate("read.logs", "staging/app", &rules), RiskLevel::Low);
    }

    #[tokio::test]
    async fn expire_pending_requests() {
        let manager = ApprovalManager::builder().build();
        let tenant = Uuid::new_v4();
        let mut req = ApprovalRequest::new(
            tenant,
            crate::ApprovalType::Tool,
            "operator",
            "action",
            "resource",
            RiskLevel::Low,
        );
        req.expires_at = Some(chrono::Utc::now() - chrono::Duration::hours(1));
        manager.request(req, "operator").await.unwrap();

        let expired = manager.expire_pending().await.unwrap();
        assert_eq!(expired, 1);
    }
}