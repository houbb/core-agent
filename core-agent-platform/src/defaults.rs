use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use crate::domain::{
    validate_actor, ActionPolicy, AuditRecord, DataPolicy, Department, EnterpriseUser,
    GovernanceRequest, HealthStatus, MetricPoint, ModelPolicy, PlatformOrganization, PlatformPolicy, PolicyEffect,
    Quota, Team, Tenant, TenantState, ToolPolicy,
};
use crate::error::{PlatformError, PlatformResult};
use crate::infrastructure::{
    GovernanceCommit, HealthCenter, MetricsCenter, PlatformPolicyEngine, PlatformStore,
};
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Default)]
pub struct DeterministicPolicyEngine;
impl PlatformPolicyEngine for DeterministicPolicyEngine {
    fn evaluate(
        &self,
        request: &GovernanceRequest,
        policies: &[PlatformPolicy],
    ) -> PlatformResult<(Option<Uuid>, Option<Uuid>, bool, String)> {
        let mut matches = policies
            .iter()
            .filter(|p| {
                p.enabled
                    && p.tenant_id == request.tenant_id
                    && p.organization_id
                        .is_none_or(|id| Some(id) == request.organization_id)
            })
            .flat_map(|p| {
                p.rules
                    .iter()
                    .filter(|r| r.matches(request))
                    .map(move |r| (p, r))
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|(p, r)| {
            (
                std::cmp::Reverse(r.priority),
                if r.effect == PolicyEffect::Deny { 0 } else { 1 },
                p.id,
                r.id,
            )
        });
        if let Some((p, r)) = matches.first() {
            let allow = r.effect == PolicyEffect::Allow;
            return Ok((
                Some(p.id),
                Some(r.id),
                allow,
                format!(
                    "Policy {} Rule {} decided {}",
                    p.key,
                    r.id,
                    r.effect.as_str()
                ),
            ));
        }
        Ok((
            None,
            None,
            false,
            "No matching Allow rule; default deny".into(),
        ))
    }
}

#[derive(Default)]
pub struct EmptyHealthCenter;
#[async_trait]
impl HealthCenter for EmptyHealthCenter {
    async fn check(&self) -> PlatformResult<Vec<HealthStatus>> {
        Ok(Vec::new())
    }
}
#[derive(Default)]
pub struct InMemoryMetricsCenter {
    points: Mutex<Vec<MetricPoint>>,
}
#[async_trait]
impl MetricsCenter for InMemoryMetricsCenter {
    async fn report(&self, point: MetricPoint) -> PlatformResult<()> {
        if !point.value.is_finite() {
            return Err(PlatformError::Validation("Metric must be finite".into()));
        }
        self.points
            .lock()
            .map_err(|_| PlatformError::Internal("Metrics lock poisoned".into()))?
            .push(point);
        Ok(())
    }
}

#[derive(Clone, Default)]
struct State {
    tenants: HashMap<Uuid, Tenant>,
    organizations: HashMap<Uuid, PlatformOrganization>,
    departments: HashMap<Uuid, Department>,
    teams: HashMap<Uuid, Team>,
    users: HashMap<Uuid, EnterpriseUser>,
    data_policies: HashMap<Uuid, DataPolicy>,
    action_policies: HashMap<Uuid, ActionPolicy>,
    tool_policies: HashMap<Uuid, ToolPolicy>,
    model_policies: HashMap<Uuid, ModelPolicy>,
    policies: HashMap<Uuid, PlatformPolicy>,
    quotas: HashMap<Uuid, Quota>,
    audits: HashMap<Uuid, AuditRecord>,
}
#[derive(Default)]
pub struct InMemoryPlatformStore {
    state: RwLock<State>,
}
impl InMemoryPlatformStore {
    fn read(&self) -> PlatformResult<std::sync::RwLockReadGuard<'_, State>> {
        self.state
            .read()
            .map_err(|_| PlatformError::Internal("Platform store lock poisoned".into()))
    }
    fn write(&self) -> PlatformResult<std::sync::RwLockWriteGuard<'_, State>> {
        self.state
            .write()
            .map_err(|_| PlatformError::Internal("Platform store lock poisoned".into()))
    }
}

#[async_trait]
impl PlatformStore for InMemoryPlatformStore {
    async fn save_tenant(&self, v: &Tenant, e: Option<u64>, actor: &str) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        version(s.tenants.get(&v.id).map(|x| x.version), e, v.version)?;
        if s.tenants.values().any(|x| x.id != v.id && x.key == v.key) {
            return Err(PlatformError::Conflict("Tenant key exists".into()));
        }
        if let Some(c) = s.tenants.get(&v.id) {
            if c.key != v.key || c.created_at != v.created_at {
                return Err(PlatformError::Conflict("Tenant identity changed".into()));
            }
        }
        s.tenants.insert(v.id, v.clone());
        Ok(())
    }
    async fn find_tenant(&self, id: Uuid) -> PlatformResult<Option<Tenant>> {
        Ok(self.read()?.tenants.get(&id).cloned())
    }
    async fn find_tenant_by_key(&self, key: &str) -> PlatformResult<Option<Tenant>> {
        Ok(self
            .read()?
            .tenants
            .values()
            .find(|x| x.key == key)
            .cloned())
    }
    async fn list_tenants(&self) -> PlatformResult<Vec<Tenant>> {
        let mut v = self.read()?.tenants.values().cloned().collect::<Vec<_>>();
        v.sort_by_key(|x| (x.key.clone(), x.id));
        Ok(v)
    }
    async fn save_organization(
        &self,
        v: &PlatformOrganization,
        e: Option<u64>,
        actor: &str,
    ) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        owner(&s, v.tenant_id, v.parent_id)?;
        version(s.organizations.get(&v.id).map(|x| x.version), e, v.version)?;
        if s.organizations
            .values()
            .any(|x| x.id != v.id && x.tenant_id == v.tenant_id && x.key == v.key)
        {
            return Err(PlatformError::Conflict("Organization key exists".into()));
        }
        if let Some(c) = s.organizations.get(&v.id) {
            if c.tenant_id != v.tenant_id || c.key != v.key || c.created_at != v.created_at {
                return Err(PlatformError::Conflict(
                    "Organization identity changed".into(),
                ));
            }
        }
        s.organizations.insert(v.id, v.clone());
        Ok(())
    }
    async fn find_organization(&self, id: Uuid) -> PlatformResult<Option<PlatformOrganization>> {
        Ok(self.read()?.organizations.get(&id).cloned())
    }
    async fn list_organizations(&self, t: Uuid) -> PlatformResult<Vec<PlatformOrganization>> {
        let s = self.read()?;
        active_owner(&s, t)?;
        let mut v = s
            .organizations
            .values()
            .filter(|x| x.tenant_id == t)
            .cloned()
            .collect::<Vec<_>>();
        v.sort_by_key(|x| (x.key.clone(), x.id));
        Ok(v)
    }
    async fn save_policy(
        &self,
        v: &PlatformPolicy,
        e: Option<u64>,
        actor: &str,
    ) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        owner(&s, v.tenant_id, v.organization_id)?;
        version(s.policies.get(&v.id).map(|x| x.version), e, v.version)?;
        if s.policies
            .values()
            .any(|x| x.id != v.id && x.tenant_id == v.tenant_id && x.key == v.key)
        {
            return Err(PlatformError::Conflict("Policy key exists".into()));
        }
        if let Some(c) = s.policies.get(&v.id) {
            if c.tenant_id != v.tenant_id || c.key != v.key || c.created_at != v.created_at {
                return Err(PlatformError::Conflict("Policy identity changed".into()));
            }
        }
        s.policies.insert(v.id, v.clone());
        Ok(())
    }
    async fn find_policy(&self, id: Uuid) -> PlatformResult<Option<PlatformPolicy>> {
        Ok(self.read()?.policies.get(&id).cloned())
    }
    async fn list_policies(&self, t: Uuid) -> PlatformResult<Vec<PlatformPolicy>> {
        let s = self.read()?;
        active_owner(&s, t)?;
        let mut v = s
            .policies
            .values()
            .filter(|x| x.tenant_id == t)
            .cloned()
            .collect::<Vec<_>>();
        v.sort_by_key(|x| (x.key.clone(), x.id));
        Ok(v)
    }
    async fn save_quota(&self, v: &Quota, e: Option<u64>, actor: &str) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        owner(&s, v.tenant_id, v.organization_id)?;
        version(s.quotas.get(&v.id).map(|x| x.version), e, v.version)?;
        if s.quotas.values().any(|x| {
            x.id != v.id
                && x.tenant_id == v.tenant_id
                && x.organization_id == v.organization_id
                && x.key == v.key
        }) {
            return Err(PlatformError::Conflict("Quota key exists".into()));
        }
        if let Some(c) = s.quotas.get(&v.id) {
            if c.tenant_id != v.tenant_id
                || c.organization_id != v.organization_id
                || c.key != v.key
                || c.created_at != v.created_at
            {
                return Err(PlatformError::Conflict("Quota identity changed".into()));
            }
        }
        s.quotas.insert(v.id, v.clone());
        Ok(())
    }
    async fn find_quota(&self, id: Uuid) -> PlatformResult<Option<Quota>> {
        Ok(self.read()?.quotas.get(&id).cloned())
    }
    async fn find_quota_by_key(
        &self,
        t: Uuid,
        o: Option<Uuid>,
        key: &str,
    ) -> PlatformResult<Option<Quota>> {
        Ok(self
            .read()?
            .quotas
            .values()
            .find(|x| x.tenant_id == t && x.organization_id == o && x.key == key)
            .cloned())
    }
    async fn list_quotas(&self, t: Uuid) -> PlatformResult<Vec<Quota>> {
        let s = self.read()?;
        active_owner(&s, t)?;
        let mut v = s
            .quotas
            .values()
            .filter(|x| x.tenant_id == t)
            .cloned()
            .collect::<Vec<_>>();
        v.sort_by_key(|x| (x.key.clone(), x.id));
        Ok(v)
    }
    async fn append_audit(&self, v: &AuditRecord, actor: &str) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        if !s.tenants.contains_key(&v.tenant_id) {
            return Err(PlatformError::not_found(v.tenant_id));
        }
        if s.audits
            .values()
            .any(|x| x.tenant_id == v.tenant_id && x.request_id == v.request_id)
            || s.audits.contains_key(&v.id)
        {
            return Err(PlatformError::Conflict(
                "Audit request already recorded".into(),
            ));
        }
        s.audits.insert(v.id, v.clone());
        Ok(())
    }
    async fn commit_governance(&self, c: &GovernanceCommit, actor: &str) -> PlatformResult<()> {
        validate_actor(actor)?;
        c.validate()?;
        let mut s = self.write()?;
        let mut n = s.clone();
        if n.audits
            .values()
            .any(|x| x.tenant_id == c.audit.tenant_id && x.request_id == c.audit.request_id)
        {
            return Err(PlatformError::Conflict(
                "Governance request already committed".into(),
            ));
        }
        if let Some(q) = &c.quota {
            owner(&n, q.tenant_id, q.organization_id)?;
            version(
                n.quotas.get(&q.id).map(|x| x.version),
                c.expected_quota_version,
                q.version,
            )?;
            n.quotas.insert(q.id, q.clone());
        }
        n.audits.insert(c.audit.id, c.audit.clone());
        *s = n;
        Ok(())
    }
    async fn find_audit_by_request(&self, t: Uuid, r: Uuid) -> PlatformResult<Option<AuditRecord>> {
        Ok(self
            .read()?
            .audits
            .values()
            .find(|x| x.tenant_id == t && x.request_id == r)
            .cloned())
    }
    async fn list_audits(&self, t: Uuid) -> PlatformResult<Vec<AuditRecord>> {
        let s = self.read()?;
        if !s.tenants.contains_key(&t) {
            return Err(PlatformError::not_found(t));
        }
        let mut v = s
            .audits
            .values()
            .filter(|x| x.tenant_id == t)
            .cloned()
            .collect::<Vec<_>>();
        v.sort_by_key(|x| (std::cmp::Reverse(x.created_at), x.id));
        Ok(v)
    }
    async fn save_department(&self, v: &Department, e: Option<u64>, actor: &str) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        active_owner(&s, v.tenant_id)?;
        version(s.departments.get(&v.id).map(|x| x.version), e, v.version)?;
        if s.departments.values().any(|x| x.id != v.id && x.tenant_id == v.tenant_id && x.key == v.key) {
            return Err(PlatformError::Conflict("Department key exists".into()));
        }
        s.departments.insert(v.id, v.clone());
        Ok(())
    }
    async fn find_department(&self, id: Uuid) -> PlatformResult<Option<Department>> {
        Ok(self.read()?.departments.get(&id).cloned())
    }
    async fn list_departments(&self, t: Uuid, o: Uuid) -> PlatformResult<Vec<Department>> {
        let s = self.read()?;
        active_owner(&s, t)?;
        let mut v = s.departments.values().filter(|x| x.tenant_id == t && x.organization_id == o).cloned().collect::<Vec<_>>();
        v.sort_by_key(|x| (x.key.clone(), x.id));
        Ok(v)
    }
    async fn save_team(&self, v: &Team, e: Option<u64>, actor: &str) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        active_owner(&s, v.tenant_id)?;
        version(s.teams.get(&v.id).map(|x| x.version), e, v.version)?;
        if s.teams.values().any(|x| x.id != v.id && x.tenant_id == v.tenant_id && x.key == v.key) {
            return Err(PlatformError::Conflict("Team key exists".into()));
        }
        s.teams.insert(v.id, v.clone());
        Ok(())
    }
    async fn find_team(&self, id: Uuid) -> PlatformResult<Option<Team>> {
        Ok(self.read()?.teams.get(&id).cloned())
    }
    async fn list_teams(&self, t: Uuid, o: Uuid, d: Option<Uuid>) -> PlatformResult<Vec<Team>> {
        let s = self.read()?;
        active_owner(&s, t)?;
        let mut v = s.teams.values().filter(|x| {
            x.tenant_id == t && x.organization_id == o && d.map_or(true, |dept_id| x.department_id == Some(dept_id))
        }).cloned().collect::<Vec<_>>();
        v.sort_by_key(|x| (x.key.clone(), x.id));
        Ok(v)
    }
    async fn save_user(&self, v: &EnterpriseUser, e: Option<u64>, actor: &str) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        active_owner(&s, v.tenant_id)?;
        version(s.users.get(&v.id).map(|x| x.version), e, v.version)?;
        if s.users.values().any(|x| x.id != v.id && x.tenant_id == v.tenant_id && x.external_subject == v.external_subject) {
            return Err(PlatformError::Conflict("User external subject exists".into()));
        }
        s.users.insert(v.id, v.clone());
        Ok(())
    }
    async fn find_user(&self, id: Uuid) -> PlatformResult<Option<EnterpriseUser>> {
        Ok(self.read()?.users.get(&id).cloned())
    }
    async fn list_users(&self, t: Uuid) -> PlatformResult<Vec<EnterpriseUser>> {
        let s = self.read()?;
        active_owner(&s, t)?;
        let mut v = s.users.values().filter(|x| x.tenant_id == t).cloned().collect::<Vec<_>>();
        v.sort_by_key(|x| (x.external_subject.clone(), x.id));
        Ok(v)
    }
    async fn save_data_policy(&self, v: &DataPolicy, e: Option<u64>, actor: &str) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        active_owner(&s, v.tenant_id)?;
        version(s.data_policies.get(&v.id).map(|x| x.version), e, v.version)?;
        if s.data_policies.values().any(|x| x.id != v.id && x.tenant_id == v.tenant_id && x.key == v.key) {
            return Err(PlatformError::Conflict("DataPolicy key exists".into()));
        }
        s.data_policies.insert(v.id, v.clone());
        Ok(())
    }
    async fn list_data_policies(&self, t: Uuid) -> PlatformResult<Vec<DataPolicy>> {
        let s = self.read()?;
        active_owner(&s, t)?;
        let mut v = s.data_policies.values().filter(|x| x.tenant_id == t).cloned().collect::<Vec<_>>();
        v.sort_by_key(|x| (x.key.clone(), x.id));
        Ok(v)
    }
    async fn save_action_policy(&self, v: &ActionPolicy, e: Option<u64>, actor: &str) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        active_owner(&s, v.tenant_id)?;
        version(s.action_policies.get(&v.id).map(|x| x.version), e, v.version)?;
        if s.action_policies.values().any(|x| x.id != v.id && x.tenant_id == v.tenant_id && x.key == v.key) {
            return Err(PlatformError::Conflict("ActionPolicy key exists".into()));
        }
        s.action_policies.insert(v.id, v.clone());
        Ok(())
    }
    async fn list_action_policies(&self, t: Uuid) -> PlatformResult<Vec<ActionPolicy>> {
        let s = self.read()?;
        active_owner(&s, t)?;
        let mut v = s.action_policies.values().filter(|x| x.tenant_id == t).cloned().collect::<Vec<_>>();
        v.sort_by_key(|x| (x.key.clone(), x.id));
        Ok(v)
    }
    async fn save_tool_policy(&self, v: &ToolPolicy, e: Option<u64>, actor: &str) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        active_owner(&s, v.tenant_id)?;
        version(s.tool_policies.get(&v.id).map(|x| x.version), e, v.version)?;
        if s.tool_policies.values().any(|x| x.id != v.id && x.tenant_id == v.tenant_id && x.key == v.key) {
            return Err(PlatformError::Conflict("ToolPolicy key exists".into()));
        }
        s.tool_policies.insert(v.id, v.clone());
        Ok(())
    }
    async fn list_tool_policies(&self, t: Uuid) -> PlatformResult<Vec<ToolPolicy>> {
        let s = self.read()?;
        active_owner(&s, t)?;
        let mut v = s.tool_policies.values().filter(|x| x.tenant_id == t).cloned().collect::<Vec<_>>();
        v.sort_by_key(|x| (x.key.clone(), x.id));
        Ok(v)
    }
    async fn save_model_policy(&self, v: &ModelPolicy, e: Option<u64>, actor: &str) -> PlatformResult<()> {
        validate_actor(actor)?;
        v.validate()?;
        let mut s = self.write()?;
        active_owner(&s, v.tenant_id)?;
        version(s.model_policies.get(&v.id).map(|x| x.version), e, v.version)?;
        if s.model_policies.values().any(|x| x.id != v.id && x.tenant_id == v.tenant_id && x.key == v.key) {
            return Err(PlatformError::Conflict("ModelPolicy key exists".into()));
        }
        s.model_policies.insert(v.id, v.clone());
        Ok(())
    }
    async fn list_model_policies(&self, t: Uuid) -> PlatformResult<Vec<ModelPolicy>> {
        let s = self.read()?;
        active_owner(&s, t)?;
        let mut v = s.model_policies.values().filter(|x| x.tenant_id == t).cloned().collect::<Vec<_>>();
        v.sort_by_key(|x| (x.key.clone(), x.id));
        Ok(v)
    }
}
fn version(c: Option<u64>, e: Option<u64>, n: u64) -> PlatformResult<()> {
    match (c, e) {
        (None, None) if n == 1 => Ok(()),
        (Some(c), Some(e)) if c == e && n == e.saturating_add(1) => Ok(()),
        _ => Err(PlatformError::Conflict(
            "optimistic version conflict".into(),
        )),
    }
}
fn active_owner(s: &State, t: Uuid) -> PlatformResult<&Tenant> {
    let x = s
        .tenants
        .get(&t)
        .ok_or_else(|| PlatformError::not_found(t))?;
    if x.state != TenantState::Active {
        return Err(PlatformError::Denied("Tenant is not Active".into()));
    }
    Ok(x)
}
fn owner(s: &State, t: Uuid, o: Option<Uuid>) -> PlatformResult<()> {
    active_owner(s, t)?;
    if let Some(id) = o {
        let x = s
            .organizations
            .get(&id)
            .ok_or_else(|| PlatformError::not_found(id))?;
        if x.tenant_id != t {
            return Err(PlatformError::Validation(
                "cross-Tenant Organization reference".into(),
            ));
        }
    }
    Ok(())
}
