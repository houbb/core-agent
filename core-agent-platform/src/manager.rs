use crate::defaults::{
    DeterministicPolicyEngine, EmptyHealthCenter, InMemoryMetricsCenter, InMemoryPlatformStore,
};
use crate::domain::{
    validate_actor, ActionPolicy, AuditDecision, AuditRecord, DataPolicy,
    Department, EnterpriseUser, GovernanceDecision, GovernanceRequest, HealthStatus, MetricPoint,
    PlatformOrganization, PlatformPolicy, PlatformState, Quota, Team, Tenant,
    TenantState,
};
use crate::error::{PlatformError, PlatformResult};
use crate::infrastructure::{
    GovernanceCommit, HealthCenter, MetricsCenter, PlatformInterceptor, PlatformObserver,
    PlatformPolicyEngine, PlatformStore,
};
use chrono::{Duration, Utc};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

pub struct PlatformManagerBuilder {
    store: Arc<dyn PlatformStore>,
    engine: Arc<dyn PlatformPolicyEngine>,
    health: Arc<dyn HealthCenter>,
    metrics: Arc<dyn MetricsCenter>,
    interceptors: Vec<Arc<dyn PlatformInterceptor>>,
    observers: Vec<Arc<dyn PlatformObserver>>,
}
impl Default for PlatformManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryPlatformStore::default()),
            engine: Arc::new(DeterministicPolicyEngine),
            health: Arc::new(EmptyHealthCenter),
            metrics: Arc::new(InMemoryMetricsCenter::default()),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}
impl PlatformManagerBuilder {
    pub fn store(mut self, v: Arc<dyn PlatformStore>) -> Self {
        self.store = v;
        self
    }
    pub fn policy_engine(mut self, v: Arc<dyn PlatformPolicyEngine>) -> Self {
        self.engine = v;
        self
    }
    pub fn health_center(mut self, v: Arc<dyn HealthCenter>) -> Self {
        self.health = v;
        self
    }
    pub fn metrics_center(mut self, v: Arc<dyn MetricsCenter>) -> Self {
        self.metrics = v;
        self
    }
    pub fn interceptor(mut self, v: Arc<dyn PlatformInterceptor>) -> Self {
        self.interceptors.push(v);
        self
    }
    pub fn observer(mut self, v: Arc<dyn PlatformObserver>) -> Self {
        self.observers.push(v);
        self
    }
    pub fn build(self) -> PlatformManager {
        PlatformManager {
            store: self.store,
            engine: self.engine,
            health: self.health,
            metrics: self.metrics,
            interceptors: self.interceptors,
            observers: self.observers,
            state: Mutex::new(PlatformState::Created),
        }
    }
}
pub struct PlatformManager {
    store: Arc<dyn PlatformStore>,
    engine: Arc<dyn PlatformPolicyEngine>,
    health: Arc<dyn HealthCenter>,
    metrics: Arc<dyn MetricsCenter>,
    interceptors: Vec<Arc<dyn PlatformInterceptor>>,
    observers: Vec<Arc<dyn PlatformObserver>>,
    state: Mutex<PlatformState>,
}
impl PlatformManager {
    pub fn builder() -> PlatformManagerBuilder {
        PlatformManagerBuilder::default()
    }
    pub fn start(&self) -> PlatformResult<PlatformState> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| PlatformError::Internal("Platform state lock poisoned".into()))?;
        match *s {
            PlatformState::Created | PlatformState::Stopped => {
                *s = PlatformState::Running;
                Ok(*s)
            }
            PlatformState::Running => Ok(*s),
        }
    }
    pub fn shutdown(&self) -> PlatformResult<PlatformState> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| PlatformError::Internal("Platform state lock poisoned".into()))?;
        *s = PlatformState::Stopped;
        Ok(*s)
    }
    pub fn status(&self) -> PlatformResult<PlatformState> {
        self.state
            .lock()
            .map(|s| *s)
            .map_err(|_| PlatformError::Internal("Platform state lock poisoned".into()))
    }
    pub async fn create_tenant(&self, v: Tenant) -> PlatformResult<Tenant> {
        self.require_running()?;
        v.validate()?;
        if self.store.find_tenant_by_key(&v.key).await?.is_some() {
            return Err(PlatformError::Conflict("Tenant key exists".into()));
        }
        self.store.save_tenant(&v, None, &v.actor).await?;
        Ok(v)
    }
    pub async fn set_tenant_state(
        &self,
        id: Uuid,
        state: TenantState,
        actor: &str,
    ) -> PlatformResult<Tenant> {
        self.require_running()?;
        validate_actor(actor)?;
        let mut v = self.required_tenant(id).await?;
        if v.state == TenantState::Archived
            || state == TenantState::Active && v.state == TenantState::Archived
        {
            return Err(PlatformError::InvalidState(
                "Archived Tenant is terminal".into(),
            ));
        }
        let e = v.version;
        v.state = state;
        advance_tenant(&mut v, actor);
        self.store.save_tenant(&v, Some(e), actor).await?;
        Ok(v)
    }
    pub async fn create_organization(
        &self,
        v: PlatformOrganization,
    ) -> PlatformResult<PlatformOrganization> {
        self.require_running()?;
        v.validate()?;
        self.required_active_tenant(v.tenant_id).await?;
        if let Some(p) = v.parent_id {
            let p = self
                .store
                .find_organization(p)
                .await?
                .ok_or_else(|| PlatformError::not_found(p))?;
            if p.tenant_id != v.tenant_id {
                return Err(PlatformError::Validation(
                    "cross-Tenant Organization parent".into(),
                ));
            }
        }
        self.store.save_organization(&v, None, &v.actor).await?;
        Ok(v)
    }
    pub async fn create_department(&self, v: Department) -> PlatformResult<Department> {
        self.require_running()?;
        v.validate()?;
        self.required_active_tenant(v.tenant_id).await?;
        let org = self.store.find_organization(v.organization_id).await?
            .ok_or_else(|| PlatformError::not_found(v.organization_id))?;
        if org.tenant_id != v.tenant_id {
            return Err(PlatformError::Validation("cross-Tenant Department organization".into()));
        }
        self.store.save_department(&v, None, &v.actor).await?;
        Ok(v)
    }
    pub async fn create_team(&self, v: Team) -> PlatformResult<Team> {
        self.require_running()?;
        v.validate()?;
        self.required_active_tenant(v.tenant_id).await?;
        let org = self.store.find_organization(v.organization_id).await?
            .ok_or_else(|| PlatformError::not_found(v.organization_id))?;
        if org.tenant_id != v.tenant_id {
            return Err(PlatformError::Validation("cross-Tenant Team organization".into()));
        }
        if let Some(dept_id) = v.department_id {
            let dept = self.store.find_department(dept_id).await?
                .ok_or_else(|| PlatformError::not_found(dept_id))?;
            if dept.tenant_id != v.tenant_id || dept.organization_id != v.organization_id {
                return Err(PlatformError::Validation("cross-org Team department".into()));
            }
        }
        self.store.save_team(&v, None, &v.actor).await?;
        Ok(v)
    }
    pub async fn create_user(&self, v: EnterpriseUser) -> PlatformResult<EnterpriseUser> {
        self.require_running()?;
        v.validate()?;
        self.required_active_tenant(v.tenant_id).await?;
        self.store.save_user(&v, None, &v.actor).await?;
        Ok(v)
    }
    pub async fn create_data_policy(&self, v: DataPolicy) -> PlatformResult<DataPolicy> {
        self.require_running()?;
        v.validate()?;
        self.required_active_tenant(v.tenant_id).await?;
        self.store.save_data_policy(&v, None, &v.actor).await?;
        Ok(v)
    }
    pub async fn create_action_policy(&self, v: ActionPolicy) -> PlatformResult<ActionPolicy> {
        self.require_running()?;
        v.validate()?;
        self.required_active_tenant(v.tenant_id).await?;
        self.store.save_action_policy(&v, None, &v.actor).await?;
        Ok(v)
    }
    pub async fn create_policy(&self, v: PlatformPolicy) -> PlatformResult<PlatformPolicy> {
        self.require_running()?;
        v.validate()?;
        self.required_active_tenant(v.tenant_id).await?;
        self.store.save_policy(&v, None, &v.actor).await?;
        Ok(v)
    }
    pub async fn create_quota(&self, v: Quota) -> PlatformResult<Quota> {
        self.require_running()?;
        v.validate()?;
        self.required_active_tenant(v.tenant_id).await?;
        self.store.save_quota(&v, None, &v.actor).await?;
        Ok(v)
    }
    pub async fn govern(&self, mut r: GovernanceRequest) -> PlatformResult<GovernanceDecision> {
        self.require_running()?;
        r.validate()?;
        let id = (
            r.request_id,
            r.tenant_id,
            r.subject.clone(),
            r.action.clone(),
            r.resource.clone(),
            r.actor.clone(),
        );
        for i in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| i.before_governance(&mut r)))
                .map_err(|_| PlatformError::Extension("Platform interceptor panicked".into()))??;
        }
        r.validate()?;
        if id
            != (
                r.request_id,
                r.tenant_id,
                r.subject.clone(),
                r.action.clone(),
                r.resource.clone(),
                r.actor.clone(),
            )
        {
            return Err(PlatformError::Validation(
                "Platform interceptor changed governance identity".into(),
            ));
        }
        if let Some(a) = self
            .store
            .find_audit_by_request(r.tenant_id, r.request_id)
            .await?
        {
            return Ok(decision_from_audit(&a));
        }
        self.required_active_tenant(r.tenant_id).await?;
        if let Some(o) = r.organization_id {
            let o = self
                .store
                .find_organization(o)
                .await?
                .ok_or_else(|| PlatformError::not_found(o))?;
            if o.tenant_id != r.tenant_id {
                return Err(PlatformError::Validation(
                    "cross-Tenant Governance Organization".into(),
                ));
            }
        }
        let policies = self.store.list_policies(r.tenant_id).await?;
        let (policy_id, rule_id, allowed, reason) = self.engine.evaluate(&r, &policies)?;
        if !allowed {
            let a = audit(&r, AuditDecision::Denied, reason, policy_id, rule_id, None);
            self.store.append_audit(&a, &r.actor).await?;
            self.notify(&a);
            return Ok(decision_from_audit(&a));
        }
        let mut quota_id = None;
        let mut quota_update = None;
        let mut expected = None;
        if let Some(key) = &r.quota_key {
            let mut q = self
                .store
                .find_quota_by_key(r.tenant_id, r.organization_id, key)
                .await?
                .ok_or_else(|| PlatformError::NotFound(format!("Quota {key}")))?;
            quota_id = Some(q.id);
            if Utc::now() >= q.window_ends_at {
                q.consumed = 0;
                q.ledger.clear();
                q.window_started_at = Utc::now();
                let seconds = i64::try_from(q.window_seconds)
                    .map_err(|_| PlatformError::Validation("Quota window too large".into()))?;
                q.window_ends_at = q.window_started_at + Duration::seconds(seconds);
            }
            if q.ledger.len() >= 1024 || q.consumed.saturating_add(r.units) > q.limit {
                let a = audit(
                    &r,
                    AuditDecision::QuotaExceeded,
                    "Quota limit or idempotency ledger capacity exceeded".into(),
                    policy_id,
                    rule_id,
                    quota_id,
                );
                self.store.append_audit(&a, &r.actor).await?;
                self.notify(&a);
                return Ok(decision_from_audit(&a));
            }
            expected = Some(q.version);
            q.consumed = q.consumed.saturating_add(r.units);
            q.ledger.insert(r.request_id, r.units);
            advance_quota(&mut q, &r.actor);
            quota_update = Some(q);
        }
        let a = audit(
            &r,
            AuditDecision::Allowed,
            reason,
            policy_id,
            rule_id,
            quota_id,
        );
        self.store
            .commit_governance(
                &GovernanceCommit {
                    quota: quota_update,
                    expected_quota_version: expected,
                    audit: a.clone(),
                },
                &r.actor,
            )
            .await?;
        self.notify(&a);
        Ok(decision_from_audit(&a))
    }
    pub async fn health(&self) -> PlatformResult<Vec<HealthStatus>> {
        self.health.check().await
    }
    pub async fn report_metric(&self, p: MetricPoint) -> PlatformResult<()> {
        self.metrics.report(p).await
    }
    pub async fn find_tenant(&self, id: Uuid) -> PlatformResult<Option<Tenant>> {
        self.store.find_tenant(id).await
    }
    pub async fn list_tenants(&self) -> PlatformResult<Vec<Tenant>> {
        self.store.list_tenants().await
    }
    pub async fn list_organizations(&self, t: Uuid) -> PlatformResult<Vec<PlatformOrganization>> {
        self.store.list_organizations(t).await
    }
    pub async fn list_departments(&self, t: Uuid, o: Uuid) -> PlatformResult<Vec<Department>> {
        self.store.list_departments(t, o).await
    }
    pub async fn list_teams(&self, t: Uuid, o: Uuid, d: Option<Uuid>) -> PlatformResult<Vec<Team>> {
        self.store.list_teams(t, o, d).await
    }
    pub async fn list_users(&self, t: Uuid) -> PlatformResult<Vec<EnterpriseUser>> {
        self.store.list_users(t).await
    }
    pub async fn list_data_policies(&self, t: Uuid) -> PlatformResult<Vec<DataPolicy>> {
        self.store.list_data_policies(t).await
    }
    pub async fn list_action_policies(&self, t: Uuid) -> PlatformResult<Vec<ActionPolicy>> {
        self.store.list_action_policies(t).await
    }
    pub async fn list_policies(&self, t: Uuid) -> PlatformResult<Vec<PlatformPolicy>> {
        self.store.list_policies(t).await
    }
    pub async fn list_quotas(&self, t: Uuid) -> PlatformResult<Vec<Quota>> {
        self.store.list_quotas(t).await
    }
    pub async fn list_audits(&self, t: Uuid) -> PlatformResult<Vec<AuditRecord>> {
        self.store.list_audits(t).await
    }
    fn require_running(&self) -> PlatformResult<()> {
        if self.status()? != PlatformState::Running {
            return Err(PlatformError::InvalidState(
                "Platform Runtime is not Running".into(),
            ));
        }
        Ok(())
    }
    async fn required_tenant(&self, id: Uuid) -> PlatformResult<Tenant> {
        self.store
            .find_tenant(id)
            .await?
            .ok_or_else(|| PlatformError::not_found(id))
    }
    async fn required_active_tenant(&self, id: Uuid) -> PlatformResult<Tenant> {
        let t = self.required_tenant(id).await?;
        if t.state != TenantState::Active {
            return Err(PlatformError::Denied("Tenant is not Active".into()));
        }
        Ok(t)
    }
    fn notify(&self, a: &AuditRecord) {
        for o in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| o.on_audit(a)));
        }
    }
}
fn audit(
    r: &GovernanceRequest,
    d: AuditDecision,
    reason: String,
    p: Option<Uuid>,
    rule: Option<Uuid>,
    q: Option<Uuid>,
) -> AuditRecord {
    AuditRecord {
        id: Uuid::new_v4(),
        request_id: r.request_id,
        tenant_id: r.tenant_id,
        organization_id: r.organization_id,
        subject: r.subject.clone(),
        action: r.action.clone(),
        resource: r.resource.clone(),
        decision: d,
        reason,
        policy_id: p,
        rule_id: rule,
        quota_id: q,
        quota_key: r.quota_key.clone(),
        units: r.units,
        attributes: r.attributes.clone(),
        actor: r.actor.clone(),
        created_at: Utc::now(),
    }
}
fn decision_from_audit(a: &AuditRecord) -> GovernanceDecision {
    GovernanceDecision {
        request_id: a.request_id,
        allowed: a.decision == AuditDecision::Allowed,
        policy_id: a.policy_id,
        rule_id: a.rule_id,
        quota_id: a.quota_id,
        reason: a.reason.clone(),
    }
}
fn advance_tenant(v: &mut Tenant, a: &str) {
    v.version = v.version.saturating_add(1);
    v.actor = a.into();
    v.updated_at = Utc::now().max(v.updated_at)
}
fn advance_quota(v: &mut Quota, a: &str) {
    v.version = v.version.saturating_add(1);
    v.actor = a.into();
    v.updated_at = Utc::now().max(v.updated_at)
}
