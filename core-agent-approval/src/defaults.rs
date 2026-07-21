use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::domain::{
    ApprovalDecision, ApprovalRequest, ApprovalState, RiskLevel, RiskRule,
    validate_actor,
};
use crate::error::{ApprovalError, ApprovalResult};
use crate::infrastructure::{ApprovalNotifier, ApprovalStore, RiskEngine};

#[derive(Default)]
pub struct InMemoryApprovalStore {
    requests: RwLock<BTreeMap<Uuid, ApprovalRequest>>,
    risk_rules: RwLock<BTreeMap<Uuid, RiskRule>>,
}

#[async_trait]
impl ApprovalStore for InMemoryApprovalStore {
    async fn create_request(&self, request: &ApprovalRequest, actor: &str) -> ApprovalResult<()> {
        validate_actor("approval creator", actor)?;
        request.validate()?;
        let mut requests = self.requests
            .write()
            .map_err(|_| ApprovalError::Internal("approval store lock poisoned".into()))?;
        if requests.contains_key(&request.id) {
            return Err(ApprovalError::Conflict(
                "approval request already exists".into(),
            ));
        }
        requests.insert(request.id, request.clone());
        Ok(())
    }

    async fn update_state(
        &self,
        id: Uuid,
        state: ApprovalState,
        expected_version: u64,
        actor: &str,
    ) -> ApprovalResult<()> {
        validate_actor("approval updater", actor)?;
        let mut requests = self.requests
            .write()
            .map_err(|_| ApprovalError::Internal("approval store lock poisoned".into()))?;
        let request = requests
            .get_mut(&id)
            .ok_or_else(|| ApprovalError::NotFound(id.to_string()))?;
        if request.version != expected_version {
            return Err(ApprovalError::Conflict(
                "approval request changed concurrently".into(),
            ));
        }
        request.state = state;
        request.version = request.version.saturating_add(1);
        request.actor = actor.into();
        request.updated_at = Utc::now().max(request.updated_at);
        request.validate()?;
        Ok(())
    }

    async fn add_decision(
        &self,
        request_id: Uuid,
        decision: &ApprovalDecision,
        actor: &str,
    ) -> ApprovalResult<()> {
        validate_actor("approval decider", actor)?;
        decision.validate()?;
        let mut requests = self.requests
            .write()
            .map_err(|_| ApprovalError::Internal("approval store lock poisoned".into()))?;
        let request = requests
            .get_mut(&request_id)
            .ok_or_else(|| ApprovalError::NotFound(request_id.to_string()))?;
        if request.decisions.iter().any(|d| d.principal_id == decision.principal_id) {
            return Err(ApprovalError::Conflict(
                "principal already decided on this request".into(),
            ));
        }
        request.decisions.push(decision.clone());
        if request.decisions.len() >= usize::from(request.required_approvals) {
            request.state = ApprovalState::Approved;
        }
        request.version = request.version.saturating_add(1);
        request.actor = actor.into();
        request.updated_at = Utc::now().max(request.updated_at);
        request.validate()?;
        Ok(())
    }

    async fn find_request(&self, id: Uuid) -> ApprovalResult<Option<ApprovalRequest>> {
        let requests = self.requests
            .read()
            .map_err(|_| ApprovalError::Internal("approval store lock poisoned".into()))?;
        Ok(requests.get(&id).cloned())
    }

    async fn list_pending(&self, tenant_id: Uuid) -> ApprovalResult<Vec<ApprovalRequest>> {
        let requests = self.requests
            .read()
            .map_err(|_| ApprovalError::Internal("approval store lock poisoned".into()))?;
        Ok(requests
            .values()
            .filter(|r| r.tenant_id == tenant_id && r.state == ApprovalState::Pending)
            .cloned()
            .collect())
    }

    async fn list_by_requester(
        &self,
        tenant_id: Uuid,
        requester: &str,
    ) -> ApprovalResult<Vec<ApprovalRequest>> {
        let requests = self.requests
            .read()
            .map_err(|_| ApprovalError::Internal("approval store lock poisoned".into()))?;
        Ok(requests
            .values()
            .filter(|r| r.tenant_id == tenant_id && r.requester == requester)
            .cloned()
            .collect())
    }

    async fn expire_pending(&self) -> ApprovalResult<u64> {
        let mut requests = self.requests
            .write()
            .map_err(|_| ApprovalError::Internal("approval store lock poisoned".into()))?;
        let now = Utc::now();
        let mut count = 0u64;
        let expired_ids: Vec<Uuid> = requests
            .values()
            .filter(|r| {
                r.state == ApprovalState::Pending
                    && r.expires_at.map_or(false, |exp| now > exp)
            })
            .map(|r| r.id)
            .collect();
        for id in expired_ids {
            if let Some(request) = requests.get_mut(&id) {
                request.state = ApprovalState::Expired;
                request.version = request.version.saturating_add(1);
                request.updated_at = now;
                count += 1;
            }
        }
        Ok(count)
    }

    async fn list_risk_rules(&self, tenant_id: Uuid) -> ApprovalResult<Vec<RiskRule>> {
        let rules = self.risk_rules
            .read()
            .map_err(|_| ApprovalError::Internal("approval store lock poisoned".into()))?;
        Ok(rules
            .values()
            .filter(|r| r.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn upsert_risk_rule(&self, rule: &RiskRule, actor: &str) -> ApprovalResult<()> {
        validate_actor("risk rule author", actor)?;
        rule.validate()?;
        let mut rules = self.risk_rules
            .write()
            .map_err(|_| ApprovalError::Internal("approval store lock poisoned".into()))?;
        if let Some(existing) = rules.get(&rule.id) {
            if existing.version != rule.version.saturating_sub(1) && rule.version != 1 {
                return Err(ApprovalError::Conflict(
                    "risk rule changed concurrently".into(),
                ));
            }
        }
        rules.insert(rule.id, rule.clone());
        Ok(())
    }
}

pub struct DefaultRiskEngine;

impl RiskEngine for DefaultRiskEngine {
    fn evaluate(&self, action: &str, resource: &str, risk_rules: &[RiskRule]) -> RiskLevel {
        // Check rules in order: highest priority first
        let mut level = RiskLevel::Low;
        for rule in risk_rules {
            if rule.matches(action, resource) {
                match rule.risk_level {
                    RiskLevel::Critical => return RiskLevel::Critical,
                    RiskLevel::High => level = RiskLevel::High,
                    RiskLevel::Medium => {
                        if level != RiskLevel::High {
                            level = RiskLevel::Medium;
                        }
                    }
                    RiskLevel::Low => {
                        if level == RiskLevel::Low {
                            level = RiskLevel::Low;
                        }
                    }
                }
            }
        }
        level
    }
}

pub struct NoopApprovalNotifier;

impl ApprovalNotifier for NoopApprovalNotifier {
    fn notify_pending(&self, _request: &ApprovalRequest) {}
    fn notify_decided(&self, _request: &ApprovalRequest, _decision: &ApprovalDecision) {}
}