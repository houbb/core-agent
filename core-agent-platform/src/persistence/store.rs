use super::schema::SCHEMA_SQL;
use crate::domain::{
    validate_actor, ActionPolicy, AuditRecord, DataPolicy, Department, EnterpriseUser,
    PlatformOrganization, PlatformPolicy, Quota, Team, Tenant,
};
use crate::error::{PlatformError, PlatformResult};
use crate::infrastructure::{GovernanceCommit, PlatformStore};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use uuid::Uuid;
pub struct SqlitePlatformStore {
    connection: Mutex<Connection>,
}
impl SqlitePlatformStore {
    pub fn new(path: impl AsRef<Path>) -> PlatformResult<Self> {
        Self::from_connection(Connection::open(path)?)
    }
    pub fn open_in_memory() -> PlatformResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(c: Connection) -> PlatformResult<Self> {
        c.busy_timeout(std::time::Duration::from_secs(5))?;
        c.execute_batch("PRAGMA foreign_keys=OFF;")?;
        c.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(c),
        })
    }
    fn lock(&self) -> PlatformResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| PlatformError::Internal("SQLite Platform lock poisoned".into()))
    }
}
#[async_trait]
impl PlatformStore for SqlitePlatformStore {
    async fn save_tenant(&self, v: &Tenant, e: Option<u64>, a: &str) -> PlatformResult<()> {
        validate_actor(a)?;
        v.validate()?;
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        write_tenant(&tx, v, e, a)?;
        tx.commit()?;
        Ok(())
    }
    async fn find_tenant(&self, id: Uuid) -> PlatformResult<Option<Tenant>> {
        let connection = self.lock()?;
        read_tenant(&connection, id)
    }
    async fn find_tenant_by_key(&self, key: &str) -> PlatformResult<Option<Tenant>> {
        let c = self.lock()?;
        let id = c
            .query_row("SELECT id FROM tenant WHERE tenant_key=?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .optional()?;
        id.map(|id| read_tenant(&c, parse_uuid("Tenant", &id)?))
            .transpose()
            .map(Option::flatten)
    }
    async fn list_tenants(&self) -> PlatformResult<Vec<Tenant>> {
        let c = self.lock()?;
        ids(&c, "SELECT id FROM tenant ORDER BY tenant_key,id", [])?
            .into_iter()
            .map(|id| read_tenant(&c, id)?.ok_or_else(|| PlatformError::not_found(id)))
            .collect()
    }
    async fn save_organization(
        &self,
        v: &PlatformOrganization,
        e: Option<u64>,
        a: &str,
    ) -> PlatformResult<()> {
        validate_actor(a)?;
        v.validate()?;
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        validate_org_owner(&tx, v)?;
        write_org(&tx, v, e, a)?;
        tx.commit()?;
        Ok(())
    }
    async fn find_organization(&self, id: Uuid) -> PlatformResult<Option<PlatformOrganization>> {
        let c = self.lock()?;
        let v = read_org(&c, id)?;
        if let Some(v) = &v {
            validate_org_owner(&c, v)?;
        }
        Ok(v)
    }
    async fn list_organizations(&self, t: Uuid) -> PlatformResult<Vec<PlatformOrganization>> {
        let c = self.lock()?;
        require_tenant(&c, t)?;
        ids(
            &c,
            "SELECT id FROM organization WHERE tenant_id=?1 ORDER BY organization_key,id",
            [t.to_string()],
        )?
        .into_iter()
        .map(|id| {
            let v = read_org(&c, id)?.ok_or_else(|| PlatformError::not_found(id))?;
            validate_org_owner(&c, &v)?;
            Ok(v)
        })
        .collect()
    }
    async fn save_policy(&self, v: &PlatformPolicy, e: Option<u64>, a: &str) -> PlatformResult<()> {
        validate_actor(a)?;
        v.validate()?;
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        validate_scope(&tx, v.tenant_id, v.organization_id)?;
        write_policy(&tx, v, e, a)?;
        tx.commit()?;
        Ok(())
    }
    async fn find_policy(&self, id: Uuid) -> PlatformResult<Option<PlatformPolicy>> {
        let c = self.lock()?;
        let v = read_policy(&c, id)?;
        if let Some(v) = &v {
            validate_scope(&c, v.tenant_id, v.organization_id)?;
        }
        Ok(v)
    }
    async fn list_policies(&self, t: Uuid) -> PlatformResult<Vec<PlatformPolicy>> {
        let c = self.lock()?;
        require_tenant(&c, t)?;
        ids(
            &c,
            "SELECT id FROM policy WHERE tenant_id=?1 ORDER BY policy_key,id",
            [t.to_string()],
        )?
        .into_iter()
        .map(|id| read_policy(&c, id)?.ok_or_else(|| PlatformError::not_found(id)))
        .collect()
    }
    async fn save_quota(&self, v: &Quota, e: Option<u64>, a: &str) -> PlatformResult<()> {
        validate_actor(a)?;
        v.validate()?;
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        validate_scope(&tx, v.tenant_id, v.organization_id)?;
        write_quota(&tx, v, e, a)?;
        tx.commit()?;
        Ok(())
    }
    async fn find_quota(&self, id: Uuid) -> PlatformResult<Option<Quota>> {
        let c = self.lock()?;
        let v = read_quota(&c, id)?;
        if let Some(v) = &v {
            validate_scope(&c, v.tenant_id, v.organization_id)?;
        }
        Ok(v)
    }
    async fn find_quota_by_key(
        &self,
        t: Uuid,
        o: Option<Uuid>,
        key: &str,
    ) -> PlatformResult<Option<Quota>> {
        let c = self.lock()?;
        validate_scope(&c, t, o)?;
        let id=c.query_row("SELECT id FROM quota WHERE tenant_id=?1 AND organization_id IS ?2 AND quota_key=?3 LIMIT 1",params![t.to_string(),o.map(|id|id.to_string()),key],|r|r.get::<_,String>(0)).optional()?;
        let Some(id) = id else { return Ok(None) };
        read_quota(&c, parse_uuid("Quota", &id)?)
    }
    async fn list_quotas(&self, t: Uuid) -> PlatformResult<Vec<Quota>> {
        let c = self.lock()?;
        require_tenant(&c, t)?;
        ids(
            &c,
            "SELECT id FROM quota WHERE tenant_id=?1 ORDER BY quota_key,id",
            [t.to_string()],
        )?
        .into_iter()
        .map(|id| read_quota(&c, id)?.ok_or_else(|| PlatformError::not_found(id)))
        .collect()
    }
    async fn append_audit(&self, v: &AuditRecord, a: &str) -> PlatformResult<()> {
        validate_actor(a)?;
        v.validate()?;
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        validate_scope(&tx, v.tenant_id, v.organization_id)?;
        insert_audit(&tx, v, a)?;
        tx.commit()?;
        Ok(())
    }
    async fn commit_governance(&self, g: &GovernanceCommit, a: &str) -> PlatformResult<()> {
        validate_actor(a)?;
        g.validate()?;
        let mut c = self.lock()?;
        let tx = c.transaction()?;
        validate_scope(&tx, g.audit.tenant_id, g.audit.organization_id)?;
        if let Some(q) = &g.quota {
            validate_scope(&tx, q.tenant_id, q.organization_id)?;
            write_quota(&tx, q, g.expected_quota_version, a)?;
        }
        insert_audit(&tx, &g.audit, a)?;
        tx.commit()?;
        Ok(())
    }
    async fn find_audit_by_request(&self, t: Uuid, r: Uuid) -> PlatformResult<Option<AuditRecord>> {
        let c = self.lock()?;
        require_tenant(&c, t)?;
        let id = c
            .query_row(
                "SELECT id FROM audit WHERE tenant_id=?1 AND request_id=?2",
                params![t.to_string(), r.to_string()],
                |x| x.get::<_, String>(0),
            )
            .optional()?;
        let Some(id) = id else { return Ok(None) };
        read_audit(&c, parse_uuid("Audit", &id)?)
    }
    async fn list_audits(&self, t: Uuid) -> PlatformResult<Vec<AuditRecord>> {
        let c = self.lock()?;
        require_tenant(&c, t)?;
        ids(
            &c,
            "SELECT id FROM audit WHERE tenant_id=?1 ORDER BY created_at DESC,id",
            [t.to_string()],
        )?
        .into_iter()
        .map(|id| read_audit(&c, id)?.ok_or_else(|| PlatformError::not_found(id)))
        .collect()
    }
    async fn save_department(&self, _v: &Department, _e: Option<u64>, _a: &str) -> PlatformResult<()> {
        Err(PlatformError::Internal("SQLite: save_department not implemented".into()))
    }
    async fn find_department(&self, _id: Uuid) -> PlatformResult<Option<Department>> {
        Err(PlatformError::Internal("SQLite: find_department not implemented".into()))
    }
    async fn list_departments(&self, _t: Uuid, _o: Uuid) -> PlatformResult<Vec<Department>> {
        Err(PlatformError::Internal("SQLite: list_departments not implemented".into()))
    }
    async fn save_team(&self, _v: &Team, _e: Option<u64>, _a: &str) -> PlatformResult<()> {
        Err(PlatformError::Internal("SQLite: save_team not implemented".into()))
    }
    async fn find_team(&self, _id: Uuid) -> PlatformResult<Option<Team>> {
        Err(PlatformError::Internal("SQLite: find_team not implemented".into()))
    }
    async fn list_teams(&self, _t: Uuid, _o: Uuid, _d: Option<Uuid>) -> PlatformResult<Vec<Team>> {
        Err(PlatformError::Internal("SQLite: list_teams not implemented".into()))
    }
    async fn save_user(&self, _v: &EnterpriseUser, _e: Option<u64>, _a: &str) -> PlatformResult<()> {
        Err(PlatformError::Internal("SQLite: save_user not implemented".into()))
    }
    async fn find_user(&self, _id: Uuid) -> PlatformResult<Option<EnterpriseUser>> {
        Err(PlatformError::Internal("SQLite: find_user not implemented".into()))
    }
    async fn list_users(&self, _t: Uuid) -> PlatformResult<Vec<EnterpriseUser>> {
        Err(PlatformError::Internal("SQLite: list_users not implemented".into()))
    }
    async fn save_data_policy(&self, _v: &DataPolicy, _e: Option<u64>, _a: &str) -> PlatformResult<()> {
        Err(PlatformError::Internal("SQLite: save_data_policy not implemented".into()))
    }
    async fn list_data_policies(&self, _t: Uuid) -> PlatformResult<Vec<DataPolicy>> {
        Err(PlatformError::Internal("SQLite: list_data_policies not implemented".into()))
    }
    async fn save_action_policy(&self, _v: &ActionPolicy, _e: Option<u64>, _a: &str) -> PlatformResult<()> {
        Err(PlatformError::Internal("SQLite: save_action_policy not implemented".into()))
    }
    async fn list_action_policies(&self, _t: Uuid) -> PlatformResult<Vec<ActionPolicy>> {
        Err(PlatformError::Internal("SQLite: list_action_policies not implemented".into()))
    }
}
fn write_tenant(tx: &Transaction<'_>, v: &Tenant, e: Option<u64>, a: &str) -> PlatformResult<()> {
    let cur = read_tenant(tx, v.id)?;
    check(cur.as_ref().map(|x| x.version), e, v.version)?;
    if let Some(c) = &cur {
        if c.key != v.key || c.created_at != v.created_at {
            return Err(PlatformError::Conflict("Tenant identity changed".into()));
        }
    }
    let now = Utc::now().to_rfc3339();
    match e{None=>{tx.execute("INSERT INTO tenant(id,tenant_key,name,state,version,content,created_at,updated_at,create_time,update_time,create_user,update_user)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9,?10,?10)",params![v.id.to_string(),v.key,v.name,v.state.as_str(),u64i(v.version)?,serde_json::to_string(v)?,v.created_at.to_rfc3339(),v.updated_at.to_rfc3339(),now,a])?;},Some(e)=>changed(tx,"UPDATE tenant SET name=?1,state=?2,version=?3,content=?4,updated_at=?5,update_time=?6,update_user=?7 WHERE id=?8 AND version=?9",params![v.name,v.state.as_str(),u64i(v.version)?,serde_json::to_string(v)?,v.updated_at.to_rfc3339(),now,a,v.id.to_string(),u64i(e)?])?,}
    Ok(())
}
fn write_org(
    tx: &Transaction<'_>,
    v: &PlatformOrganization,
    e: Option<u64>,
    a: &str,
) -> PlatformResult<()> {
    let cur = read_org(tx, v.id)?;
    check(cur.as_ref().map(|x| x.version), e, v.version)?;
    if let Some(c) = &cur {
        if c.tenant_id != v.tenant_id || c.key != v.key || c.created_at != v.created_at {
            return Err(PlatformError::Conflict(
                "Organization identity changed".into(),
            ));
        }
    }
    let now = Utc::now().to_rfc3339();
    match e{None=>{tx.execute("INSERT INTO organization(id,tenant_id,parent_id,organization_key,name,version,content,created_at,updated_at,create_time,update_time,create_user,update_user)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,?11,?11)",params![v.id.to_string(),v.tenant_id.to_string(),v.parent_id.map(|x|x.to_string()),v.key,v.name,u64i(v.version)?,serde_json::to_string(v)?,v.created_at.to_rfc3339(),v.updated_at.to_rfc3339(),now,a])?;},Some(e)=>changed(tx,"UPDATE organization SET parent_id=?1,name=?2,version=?3,content=?4,updated_at=?5,update_time=?6,update_user=?7 WHERE id=?8 AND version=?9",params![v.parent_id.map(|x|x.to_string()),v.name,u64i(v.version)?,serde_json::to_string(v)?,v.updated_at.to_rfc3339(),now,a,v.id.to_string(),u64i(e)?])?,}
    Ok(())
}
fn write_policy(
    tx: &Transaction<'_>,
    v: &PlatformPolicy,
    e: Option<u64>,
    a: &str,
) -> PlatformResult<()> {
    let cur = read_policy(tx, v.id)?;
    check(cur.as_ref().map(|x| x.version), e, v.version)?;
    if let Some(c) = &cur {
        if c.tenant_id != v.tenant_id || c.key != v.key || c.created_at != v.created_at {
            return Err(PlatformError::Conflict("Policy identity changed".into()));
        }
    }
    let now = Utc::now().to_rfc3339();
    match e{None=>{tx.execute("INSERT INTO policy(id,tenant_id,organization_id,policy_key,enabled,version,content,created_at,updated_at,create_time,update_time,create_user,update_user)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10,?11,?11)",params![v.id.to_string(),v.tenant_id.to_string(),v.organization_id.map(|x|x.to_string()),v.key,bi(v.enabled),u64i(v.version)?,serde_json::to_string(v)?,v.created_at.to_rfc3339(),v.updated_at.to_rfc3339(),now,a])?;},Some(e)=>changed(tx,"UPDATE policy SET organization_id=?1,enabled=?2,version=?3,content=?4,updated_at=?5,update_time=?6,update_user=?7 WHERE id=?8 AND version=?9",params![v.organization_id.map(|x|x.to_string()),bi(v.enabled),u64i(v.version)?,serde_json::to_string(v)?,v.updated_at.to_rfc3339(),now,a,v.id.to_string(),u64i(e)?])?,}
    Ok(())
}
fn write_quota(tx: &Transaction<'_>, v: &Quota, e: Option<u64>, a: &str) -> PlatformResult<()> {
    let cur = read_quota(tx, v.id)?;
    check(cur.as_ref().map(|x| x.version), e, v.version)?;
    if let Some(c) = &cur {
        if c.tenant_id != v.tenant_id
            || c.organization_id != v.organization_id
            || c.key != v.key
            || c.created_at != v.created_at
        {
            return Err(PlatformError::Conflict("Quota identity changed".into()));
        }
    }
    let now = Utc::now().to_rfc3339();
    match e{None=>{tx.execute("INSERT INTO quota(id,tenant_id,organization_id,quota_key,quota_limit,consumed,window_ends_at,version,content,created_at,updated_at,create_time,update_time,create_user,update_user)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?12,?13,?13)",params![v.id.to_string(),v.tenant_id.to_string(),v.organization_id.map(|x|x.to_string()),v.key,u64i(v.limit)?,u64i(v.consumed)?,v.window_ends_at.to_rfc3339(),u64i(v.version)?,serde_json::to_string(v)?,v.created_at.to_rfc3339(),v.updated_at.to_rfc3339(),now,a])?;},Some(e)=>changed(tx,"UPDATE quota SET quota_limit=?1,consumed=?2,window_ends_at=?3,version=?4,content=?5,updated_at=?6,update_time=?7,update_user=?8 WHERE id=?9 AND version=?10",params![u64i(v.limit)?,u64i(v.consumed)?,v.window_ends_at.to_rfc3339(),u64i(v.version)?,serde_json::to_string(v)?,v.updated_at.to_rfc3339(),now,a,v.id.to_string(),u64i(e)?])?,}
    Ok(())
}
fn insert_audit(tx: &Transaction<'_>, v: &AuditRecord, a: &str) -> PlatformResult<()> {
    v.validate()?;
    let now = Utc::now().to_rfc3339();
    tx.execute("INSERT INTO audit(id,request_id,tenant_id,organization_id,subject,action,resource,decision,content,created_at,create_time,update_time,create_user,update_user)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11,?12,?12)",params![v.id.to_string(),v.request_id.to_string(),v.tenant_id.to_string(),v.organization_id.map(|x|x.to_string()),v.subject,v.action,v.resource,v.decision.as_str(),serde_json::to_string(v)?,v.created_at.to_rfc3339(),now,a])?;
    Ok(())
}
fn read_tenant(c: &Connection, id: Uuid) -> PlatformResult<Option<Tenant>> {
    let raw=c.query_row("SELECT tenant_key,state,version,content,created_at,updated_at,update_user FROM tenant WHERE id=?1",[id.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,i64>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?))).optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let v: Tenant = serde_json::from_str(&raw.3)?;
    v.validate()?;
    if v.id != id
        || v.key != raw.0
        || v.state.as_str() != raw.1
        || v.version != i64u("Tenant version", raw.2)?
        || v.created_at != time("Tenant created", &raw.4)?
        || v.updated_at != time("Tenant updated", &raw.5)?
        || v.actor != raw.6
    {
        return Err(PlatformError::Validation(
            "Tenant columns mismatch content".into(),
        ));
    }
    Ok(Some(v))
}
// Owner-bearing entities use explicit readers because their structured columns differ.
fn read_org(c: &Connection, id: Uuid) -> PlatformResult<Option<PlatformOrganization>> {
    let raw=c.query_row("SELECT tenant_id,parent_id,organization_key,version,content,created_at,updated_at,update_user FROM organization WHERE id=?1",[id.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,String>(7)?))).optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let v: PlatformOrganization = serde_json::from_str(&raw.4)?;
    v.validate()?;
    if v.id != id
        || v.tenant_id != parse_uuid("Org tenant", &raw.0)?
        || v.parent_id != ou(raw.1.as_deref())?
        || v.key != raw.2
        || v.version != i64u("Org version", raw.3)?
        || v.created_at != time("Org created", &raw.5)?
        || v.updated_at != time("Org updated", &raw.6)?
        || v.actor != raw.7
    {
        return Err(PlatformError::Validation(
            "Organization columns mismatch content".into(),
        ));
    }
    Ok(Some(v))
}
fn read_policy(c: &Connection, id: Uuid) -> PlatformResult<Option<PlatformPolicy>> {
    let raw=c.query_row("SELECT tenant_id,organization_id,policy_key,enabled,version,content,created_at,updated_at,update_user FROM policy WHERE id=?1",[id.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,String>(7)?,r.get::<_,String>(8)?))).optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let v: PlatformPolicy = serde_json::from_str(&raw.5)?;
    v.validate()?;
    if v.id != id
        || v.tenant_id != parse_uuid("Policy tenant", &raw.0)?
        || v.organization_id != ou(raw.1.as_deref())?
        || v.key != raw.2
        || v.enabled != (raw.3 == 1)
        || v.version != i64u("Policy version", raw.4)?
        || v.created_at != time("Policy created", &raw.6)?
        || v.updated_at != time("Policy updated", &raw.7)?
        || v.actor != raw.8
    {
        return Err(PlatformError::Validation(
            "Policy columns mismatch content".into(),
        ));
    }
    Ok(Some(v))
}
fn read_quota(c: &Connection, id: Uuid) -> PlatformResult<Option<Quota>> {
    let raw=c.query_row("SELECT tenant_id,organization_id,quota_key,consumed,version,content,created_at,updated_at,update_user FROM quota WHERE id=?1",[id.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?,r.get::<_,String>(5)?,r.get::<_,String>(6)?,r.get::<_,String>(7)?,r.get::<_,String>(8)?))).optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let v: Quota = serde_json::from_str(&raw.5)?;
    v.validate()?;
    if v.id != id
        || v.tenant_id != parse_uuid("Quota tenant", &raw.0)?
        || v.organization_id != ou(raw.1.as_deref())?
        || v.key != raw.2
        || v.consumed != i64u("consumed", raw.3)?
        || v.version != i64u("Quota version", raw.4)?
        || v.created_at != time("Quota created", &raw.6)?
        || v.updated_at != time("Quota updated", &raw.7)?
        || v.actor != raw.8
    {
        return Err(PlatformError::Validation(
            "Quota columns mismatch content".into(),
        ));
    }
    Ok(Some(v))
}
fn read_audit(c: &Connection, id: Uuid) -> PlatformResult<Option<AuditRecord>> {
    let raw=c.query_row("SELECT tenant_id,request_id,decision,content,created_at,update_user FROM audit WHERE id=?1",[id.to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?))).optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let v: AuditRecord = serde_json::from_str(&raw.3)?;
    v.validate()?;
    if v.id != id
        || v.tenant_id != parse_uuid("Audit tenant", &raw.0)?
        || v.request_id != parse_uuid("Audit request", &raw.1)?
        || v.decision.as_str() != raw.2
        || v.created_at != time("Audit created", &raw.4)?
        || v.actor != raw.5
    {
        return Err(PlatformError::Validation(
            "Audit columns mismatch content".into(),
        ));
    }
    Ok(Some(v))
}
fn validate_scope(c: &Connection, t: Uuid, o: Option<Uuid>) -> PlatformResult<()> {
    require_tenant(c, t)?;
    if let Some(o) = o {
        let v = read_org(c, o)?.ok_or_else(|| PlatformError::not_found(o))?;
        if v.tenant_id != t {
            return Err(PlatformError::Validation("cross-Tenant scope".into()));
        }
    }
    Ok(())
}
fn validate_org_owner(c: &Connection, v: &PlatformOrganization) -> PlatformResult<()> {
    validate_scope(c, v.tenant_id, v.parent_id)
}
fn require_tenant(c: &Connection, id: Uuid) -> PlatformResult<Tenant> {
    read_tenant(c, id)?.ok_or_else(|| PlatformError::not_found(id))
}
fn check(c: Option<u64>, e: Option<u64>, n: u64) -> PlatformResult<()> {
    match (c, e) {
        (None, None) if n == 1 => Ok(()),
        (Some(c), Some(e)) if c == e && n == e.saturating_add(1) => Ok(()),
        _ => Err(PlatformError::Conflict(
            "optimistic version conflict".into(),
        )),
    }
}
fn changed(tx: &Transaction<'_>, sql: &str, p: impl rusqlite::Params) -> PlatformResult<()> {
    if tx.execute(sql, p)? != 1 {
        return Err(PlatformError::Conflict("stale SQLite writer".into()));
    }
    Ok(())
}
fn ids<P: rusqlite::Params>(c: &Connection, s: &str, p: P) -> PlatformResult<Vec<Uuid>> {
    let mut x = c.prepare(s)?;
    let values = x
        .query_map(p, |r| r.get::<_, String>(0))?
        .map(|v| parse_uuid("row", &v?))
        .collect();
    values
}
fn parse_uuid(l: &str, v: &str) -> PlatformResult<Uuid> {
    Uuid::parse_str(v).map_err(|e| PlatformError::Validation(format!("invalid {l}: {e}")))
}
fn ou(v: Option<&str>) -> PlatformResult<Option<Uuid>> {
    v.map(|x| parse_uuid("optional uuid", x)).transpose()
}
fn time(l: &str, v: &str) -> PlatformResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(v)
        .map(|x| x.with_timezone(&Utc))
        .map_err(|e| PlatformError::Validation(format!("invalid {l}: {e}")))
}
fn u64i(v: u64) -> PlatformResult<i64> {
    i64::try_from(v).map_err(|_| PlatformError::Validation("integer too large".into()))
}
fn i64u(l: &str, v: i64) -> PlatformResult<u64> {
    u64::try_from(v).map_err(|_| PlatformError::Validation(format!("invalid {l}")))
}
fn bi(v: bool) -> i64 {
    if v {
        1
    } else {
        0
    }
}
