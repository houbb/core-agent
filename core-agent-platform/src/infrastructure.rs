use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{
    AuditRecord, GovernanceRequest, HealthStatus, MetricPoint, PlatformOrganization,
    PlatformPolicy, Quota, Tenant,
};
use crate::error::PlatformResult;

#[derive(Debug, Clone)]
pub struct GovernanceCommit {
    pub quota: Option<Quota>,
    pub expected_quota_version: Option<u64>,
    pub audit: AuditRecord,
}
impl GovernanceCommit {
    pub fn validate(&self) -> PlatformResult<()> {
        self.audit.validate()?;
        if let Some(q) = &self.quota {
            q.validate()?;
            if q.tenant_id != self.audit.tenant_id
                || q.key.as_str() != self.audit.quota_key.as_deref().unwrap_or_default()
            {
                return Err(crate::error::PlatformError::Validation(
                    "Quota and Audit ownership mismatch".into(),
                ));
            }
        }
        Ok(())
    }
}

#[async_trait]
pub trait PlatformStore: Send + Sync {
    async fn save_tenant(
        &self,
        value: &Tenant,
        expected: Option<u64>,
        actor: &str,
    ) -> PlatformResult<()>;
    async fn find_tenant(&self, id: Uuid) -> PlatformResult<Option<Tenant>>;
    async fn find_tenant_by_key(&self, key: &str) -> PlatformResult<Option<Tenant>>;
    async fn list_tenants(&self) -> PlatformResult<Vec<Tenant>>;
    async fn save_organization(
        &self,
        value: &PlatformOrganization,
        expected: Option<u64>,
        actor: &str,
    ) -> PlatformResult<()>;
    async fn find_organization(&self, id: Uuid) -> PlatformResult<Option<PlatformOrganization>>;
    async fn list_organizations(
        &self,
        tenant_id: Uuid,
    ) -> PlatformResult<Vec<PlatformOrganization>>;
    async fn save_policy(
        &self,
        value: &PlatformPolicy,
        expected: Option<u64>,
        actor: &str,
    ) -> PlatformResult<()>;
    async fn find_policy(&self, id: Uuid) -> PlatformResult<Option<PlatformPolicy>>;
    async fn list_policies(&self, tenant_id: Uuid) -> PlatformResult<Vec<PlatformPolicy>>;
    async fn save_quota(
        &self,
        value: &Quota,
        expected: Option<u64>,
        actor: &str,
    ) -> PlatformResult<()>;
    async fn find_quota(&self, id: Uuid) -> PlatformResult<Option<Quota>>;
    async fn find_quota_by_key(
        &self,
        tenant_id: Uuid,
        organization_id: Option<Uuid>,
        key: &str,
    ) -> PlatformResult<Option<Quota>>;
    async fn list_quotas(&self, tenant_id: Uuid) -> PlatformResult<Vec<Quota>>;
    async fn append_audit(&self, value: &AuditRecord, actor: &str) -> PlatformResult<()>;
    async fn commit_governance(&self, commit: &GovernanceCommit, actor: &str)
        -> PlatformResult<()>;
    async fn find_audit_by_request(
        &self,
        tenant_id: Uuid,
        request_id: Uuid,
    ) -> PlatformResult<Option<AuditRecord>>;
    async fn list_audits(&self, tenant_id: Uuid) -> PlatformResult<Vec<AuditRecord>>;
}

pub trait PlatformPolicyEngine: Send + Sync {
    fn evaluate(
        &self,
        request: &GovernanceRequest,
        policies: &[PlatformPolicy],
    ) -> PlatformResult<(Option<Uuid>, Option<Uuid>, bool, String)>;
}
#[async_trait]
pub trait HealthCenter: Send + Sync {
    async fn check(&self) -> PlatformResult<Vec<HealthStatus>>;
}
#[async_trait]
pub trait MetricsCenter: Send + Sync {
    async fn report(&self, point: MetricPoint) -> PlatformResult<()>;
}
pub trait PlatformObserver: Send + Sync {
    fn on_audit(&self, audit: &AuditRecord);
}
pub trait PlatformInterceptor: Send + Sync {
    fn before_governance(&self, _request: &mut GovernanceRequest) -> PlatformResult<()> {
        Ok(())
    }
}
