use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::{
    ApprovalDecision, ApprovalRequest, ApprovalState, RiskRule,
    validate_actor,
};
use crate::error::{ApprovalError, ApprovalResult};
use crate::infrastructure::ApprovalStore;

use super::schema::SCHEMA_SQL;

pub struct SqliteApprovalStore {
    connection: Mutex<Connection>,
}

impl SqliteApprovalStore {
    pub fn new(path: impl AsRef<Path>) -> ApprovalResult<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> ApprovalResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> ApprovalResult<Self> {
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> ApprovalResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| ApprovalError::Internal("approval SQLite lock poisoned".into()))
    }
}

#[async_trait]
impl ApprovalStore for SqliteApprovalStore {
    async fn create_request(&self, request: &ApprovalRequest, actor: &str) -> ApprovalResult<()> {
        validate_actor("approval creator", actor)?;
        request.validate()?;
        let connection = self.lock()?;
        let exists = connection
            .query_row("SELECT 1 FROM approval_request WHERE id = ?1", [request.id.to_string()], |_| Ok(()))
            .optional()?
            .is_some();
        if exists {
            return Err(ApprovalError::Conflict("approval request already exists".into()));
        }
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO approval_request (
                id, tenant_id, organization_id, request_type, requester, action, resource,
                risk_level, state, required_approvals, expires_at, version, content,
                created_at, updated_at, create_time, update_time, create_user, update_user
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16, ?17, ?17)",
            params![
                request.id.to_string(),
                request.tenant_id.to_string(),
                request.organization_id.map(|id| id.to_string()),
                request.request_type.as_str(),
                request.requester,
                request.action,
                request.resource,
                request.risk_level.as_str(),
                request.state.as_str(),
                request.required_approvals,
                request.expires_at.map(|t| t.to_rfc3339()),
                u64_i64(request.version)?,
                serde_json::to_string(request)?,
                request.created_at.to_rfc3339(),
                request.updated_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
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
        let connection = self.lock()?;
        let current = read_request(&connection, id)?
            .ok_or_else(|| ApprovalError::NotFound(id.to_string()))?;
        if current.version != expected_version {
            return Err(ApprovalError::Conflict("approval request changed concurrently".into()));
        }
        let mut updated = current.clone();
        updated.state = state;
        updated.version = updated.version.saturating_add(1);
        updated.actor = actor.into();
        updated.updated_at = Utc::now().max(current.updated_at);
        updated.validate()?;

        let changed = connection.execute(
            "UPDATE approval_request SET state = ?1, version = ?2, content = ?3,
                updated_at = ?4, update_time = ?5, update_user = ?6
             WHERE id = ?7 AND version = ?8",
            params![
                state.as_str(),
                u64_i64(updated.version)?,
                serde_json::to_string(&updated)?,
                updated.updated_at.to_rfc3339(),
                Utc::now().to_rfc3339(),
                actor,
                id.to_string(),
                u64_i64(expected_version)?,
            ],
        )?;
        if changed != 1 {
            return Err(ApprovalError::Conflict("approval request changed concurrently".into()));
        }
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
        // For InMemory-like behavior, we do a read-modify-write
        let connection = self.lock()?;
        let current = read_request(&connection, request_id)?
            .ok_or_else(|| ApprovalError::NotFound(request_id.to_string()))?;
        if current.decisions.iter().any(|d| d.principal_id == decision.principal_id) {
            return Err(ApprovalError::Conflict("principal already decided".into()));
        }
        let mut updated = current.clone();
        updated.decisions.push(decision.clone());
        if updated.decisions.len() >= usize::from(updated.required_approvals) {
            updated.state = ApprovalState::Approved;
        }
        updated.version = updated.version.saturating_add(1);
        updated.actor = actor.into();
        updated.updated_at = Utc::now().max(current.updated_at);
        updated.validate()?;

        let changed = connection.execute(
            "UPDATE approval_request SET state = ?1, version = ?2, content = ?3,
                updated_at = ?4, update_time = ?5, update_user = ?6
             WHERE id = ?7 AND version = ?8",
            params![
                updated.state.as_str(),
                u64_i64(updated.version)?,
                serde_json::to_string(&updated)?,
                updated.updated_at.to_rfc3339(),
                Utc::now().to_rfc3339(),
                actor,
                request_id.to_string(),
                u64_i64(current.version)?,
            ],
        )?;
        if changed != 1 {
            return Err(ApprovalError::Conflict("approval request changed concurrently".into()));
        }
        Ok(())
    }

    async fn find_request(&self, id: Uuid) -> ApprovalResult<Option<ApprovalRequest>> {
        let connection = self.lock()?;
        read_request(&connection, id)
    }

    async fn list_pending(&self, tenant_id: Uuid) -> ApprovalResult<Vec<ApprovalRequest>> {
        let connection = self.lock()?;
        list_by_field(&connection, "tenant_id = ?1 AND state = 'PENDING'", tenant_id.to_string())
    }

    async fn list_by_requester(
        &self,
        tenant_id: Uuid,
        requester: &str,
    ) -> ApprovalResult<Vec<ApprovalRequest>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id FROM approval_request WHERE tenant_id = ?1 AND requester = ?2 ORDER BY updated_at DESC, id LIMIT 1000",
        )?;
        let ids: Vec<String> = statement
            .query_map(params![tenant_id.to_string(), requester], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let mut requests = Vec::new();
        for id_str in ids {
            let uuid = Uuid::parse_str(&id_str)
                .map_err(|e| ApprovalError::Validation(format!("invalid uuid: {e}")))?;
            if let Some(req) = read_request(&connection, uuid)? {
                requests.push(req);
            }
        }
        Ok(requests)
    }

    async fn expire_pending(&self) -> ApprovalResult<u64> {
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let expired = connection.execute(
            "UPDATE approval_request SET state = 'EXPIRED', version = version + 1,
                update_time = ?1, update_user = 'system'
             WHERE state = 'PENDING' AND expires_at IS NOT NULL AND expires_at < ?1",
            params![now],
        )?;
        Ok(expired as u64)
    }

    async fn list_risk_rules(&self, tenant_id: Uuid) -> ApprovalResult<Vec<RiskRule>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, tenant_id, action_pattern, resource_pattern, risk_level, enabled, version, content, created_at, updated_at
             FROM risk_rule WHERE tenant_id = ?1 ORDER BY created_at DESC, id",
        )?;
        let rows = statement.query_map([tenant_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        })?;
        let mut rules = Vec::new();
        for row in rows {
            let (id_str, tenant_str, action_pat, resource_pat, risk_str, enabled, version, content, created_str, updated_str) = row?;
            let value: RiskRule = serde_json::from_str(&content)?;
            value.validate()?;
            if id_str != value.id.to_string() || tenant_str != value.tenant_id.to_string() {
                return Err(ApprovalError::Validation("risk rule columns do not match content".into()));
            }
            rules.push(value);
        }
        Ok(rules)
    }

    async fn upsert_risk_rule(&self, rule: &RiskRule, actor: &str) -> ApprovalResult<()> {
        validate_actor("risk rule author", actor)?;
        rule.validate()?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let existing: Option<bool> = connection
            .query_row("SELECT 1 FROM risk_rule WHERE id = ?1", [rule.id.to_string()], |_| Ok(true))
            .optional()?;
        if existing.is_some() {
            connection.execute(
                "UPDATE risk_rule SET action_pattern = ?1, resource_pattern = ?2, risk_level = ?3,
                    enabled = ?4, version = ?5, content = ?6, updated_at = ?7,
                    update_time = ?8, update_user = ?9
                 WHERE id = ?10 AND version = ?11",
                params![
                    rule.action_pattern,
                    rule.resource_pattern,
                    rule.risk_level.as_str(),
                    rule.enabled as i64,
                    u64_i64(rule.version)?,
                    serde_json::to_string(rule)?,
                    rule.updated_at.to_rfc3339(),
                    now,
                    actor,
                    rule.id.to_string(),
                    u64_i64(rule.version.saturating_sub(1))?,
                ],
            )?;
        } else {
            connection.execute(
                "INSERT INTO risk_rule (id, tenant_id, action_pattern, resource_pattern, risk_level,
                    enabled, version, content, created_at, updated_at, create_time, update_time, create_user, update_user)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?12, ?12)",
                params![
                    rule.id.to_string(),
                    rule.tenant_id.to_string(),
                    rule.action_pattern,
                    rule.resource_pattern,
                    rule.risk_level.as_str(),
                    rule.enabled as i64,
                    u64_i64(rule.version)?,
                    serde_json::to_string(rule)?,
                    rule.created_at.to_rfc3339(),
                    rule.updated_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
        }
        Ok(())
    }
}

fn read_request(connection: &Connection, id: Uuid) -> ApprovalResult<Option<ApprovalRequest>> {
    type Row = (String, String, Option<String>, String, String, String, String, String, String, i64, Option<String>, i64, String, String, String);
    let raw: Option<Row> = connection
        .query_row(
            "SELECT id, tenant_id, organization_id, request_type, requester, action, resource,
                    risk_level, state, required_approvals, expires_at, version, content, created_at, updated_at
             FROM approval_request WHERE id = ?1",
            [id.to_string()],
            |row| {
                Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                    row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?,
                    row.get(10)?, row.get(11)?, row.get(12)?, row.get(13)?, row.get(14)?,
                ))
            },
        )
        .optional()?;
    let Some(raw) = raw else { return Ok(None) };
    let value: ApprovalRequest = serde_json::from_str(&raw.12)?;
    value.validate()?;
    if raw.0 != value.id.to_string()
        || raw.1 != value.tenant_id.to_string()
        || raw.7 != value.risk_level.as_str()
        || raw.8 != value.state.as_str()
        || raw.11 != u64_i64(value.version)?
        || raw.13 != value.created_at.to_rfc3339()
        || raw.14 != value.updated_at.to_rfc3339()
    {
        return Err(ApprovalError::Validation(
            "approval columns do not match serialized content".into(),
        ));
    }
    Ok(Some(value))
}

fn list_by_field<T: AsRef<str>>(connection: &Connection, where_clause: &str, params: T) -> ApprovalResult<Vec<ApprovalRequest>> {
    let sql = format!(
        "SELECT id FROM approval_request WHERE {} ORDER BY updated_at DESC, id LIMIT 1000",
        where_clause
    );
    let mut statement = connection.prepare(&sql)?;
    let ids: Vec<String> = statement
        .query_map([params.as_ref()], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut requests = Vec::new();
    for id_str in ids {
        let uuid = Uuid::parse_str(&id_str)
            .map_err(|e| ApprovalError::Validation(format!("invalid uuid: {e}")))?;
        if let Some(req) = read_request(connection, uuid)? {
            requests.push(req);
        }
    }
    Ok(requests)
}

fn u64_i64(value: u64) -> ApprovalResult<i64> {
    i64::try_from(value)
        .map_err(|_| ApprovalError::Validation("approval integer exceeds SQLite range".into()))
}