use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::{
    AuditEvent, AuditQuery, AuditSnapshot,
    validate_actor,
};
use crate::error::{AuditError, AuditResult};
use crate::infrastructure::AuditStore;

use super::schema::SCHEMA_SQL;

pub struct SqliteAuditStore {
    connection: Mutex<Connection>,
}

impl SqliteAuditStore {
    pub fn new(path: impl AsRef<Path>) -> AuditResult<Self> {
        let connection = Connection::open(path)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> AuditResult<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> AuditResult<Self> {
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> AuditResult<MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| AuditError::Internal("audit SQLite lock poisoned".into()))
    }
}

#[async_trait]
impl AuditStore for SqliteAuditStore {
    async fn record(&self, event: &AuditEvent, actor: &str) -> AuditResult<()> {
        validate_actor("audit writer", actor)?;
        event.validate()?;
        let connection = self.lock()?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM audit_event WHERE id = ?1",
                [event.id.to_string()],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if exists {
            return Err(AuditError::Conflict(
                "audit event already exists".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO audit_event (
                id, tenant_id, actor, event_type, action, resource,
                payload, severity, result, request_id, session_id, trace_id,
                client_ip, user_agent, occurred_at, version, content, created_at,
                create_time, update_time, create_user, update_user
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
              ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?19, ?20, ?20)",
            params![
                event.id.to_string(),
                event.tenant_id.to_string(),
                event.actor,
                event.event_type.as_str(),
                event.action,
                event.resource,
                serde_json::to_string(&event.payload)?,
                event.severity.as_str(),
                event.result,
                event.request_id.map(|id| id.to_string()),
                event.session_id.map(|id| id.to_string()),
                event.trace_id,
                event.client_ip,
                event.user_agent,
                event.occurred_at.to_rfc3339(),
                u64_i64(event.version)?,
                serde_json::to_string(event)?,
                event.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find(&self, id: Uuid) -> AuditResult<Option<AuditEvent>> {
        let connection = self.lock()?;
        let raw: Option<(String, String, String, String, String, String, String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, String, i64, String, String)> = connection
            .query_row(
                "SELECT id, tenant_id, actor, event_type, action, resource,
                        payload, severity, result, request_id, session_id, trace_id,
                        client_ip, user_agent, occurred_at, version, content, created_at
                 FROM audit_event WHERE id = ?1",
                [id.to_string()],
                |row| {
                    Ok((
                        row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?,
                        row.get(4)?, row.get(5)?, row.get(6)?, row.get(7)?,
                        row.get(8)?, row.get(9)?, row.get(10)?, row.get(11)?,
                        row.get(12)?, row.get(13)?, row.get(14)?, row.get(15)?,
                        row.get(16)?, row.get(17)?,
                    ))
                },
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let value: AuditEvent = serde_json::from_str(&raw.16)?;
        value.validate()?;
        if raw.0 != value.id.to_string()
            || raw.1 != value.tenant_id.to_string()
            || raw.2 != value.actor
            || raw.3 != value.event_type.as_str()
            || raw.4 != value.action
            || raw.5 != value.resource
            || raw.14 != value.occurred_at.to_rfc3339()
            || raw.15 != u64_i64(value.version)?
            || raw.17 != value.created_at.to_rfc3339()
        {
            return Err(AuditError::Validation(
                "audit columns do not match serialized content".into(),
            ));
        }
        Ok(Some(value))
    }

    async fn list(&self, query: &AuditQuery) -> AuditResult<Vec<AuditEvent>> {
        query.validate()?;
        let ids = {
            let connection = self.lock()?;
            list_ids_sync(&connection, query)?
        };
        let mut events = Vec::new();
        for id in ids {
            let uuid = Uuid::parse_str(&id)
                .map_err(|e| AuditError::Validation(format!("invalid uuid: {e}")))?;
            if let Some(event) = self.find(uuid).await? {
                events.push(event);
            }
        }
        Ok(events)
    }

    async fn count(&self, query: &AuditQuery) -> AuditResult<u64> {
        query.validate()?;
        let connection = self.lock()?;
        let mut sql = String::from("SELECT COUNT(*) FROM audit_event");
        let mut clauses: Vec<String> = Vec::new();
        if let Some(tenant_id) = &query.tenant_id {
            clauses.push(format!("tenant_id = '{}'", tenant_id));
        }
        if let Some(actor) = &query.actor {
            clauses.push(format!("actor = '{}'", actor.replace('\'', "''")));
        }
        if let Some(event_type) = &query.event_type {
            clauses.push(format!("event_type = '{}'", event_type.as_str()));
        }
        if let Some(severity) = &query.severity {
            clauses.push(format!("severity = '{}'", severity.as_str()));
        }
        if !clauses.is_empty() {
            sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
        }
        let count: i64 = connection.query_row(&sql, [], |row| row.get(0))?;
        Ok(count as u64)
    }

    async fn snapshot(&self, tenant_id: Uuid) -> AuditResult<AuditSnapshot> {
        let connection = self.lock()?;
        let total: i64 = connection.query_row(
            "SELECT COUNT(*) FROM audit_event WHERE tenant_id = ?1",
            [tenant_id.to_string()],
            |row| row.get(0),
        )?;
        let mut statement = connection.prepare(
            "SELECT event_type, COUNT(*) FROM audit_event WHERE tenant_id = ?1 GROUP BY event_type"
        )?;
        let by_type: std::collections::BTreeMap<String, u64> = statement
            .query_map([tenant_id.to_string()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })?
            .collect::<Result<_, _>>()?;
        let mut statement = connection.prepare(
            "SELECT severity, COUNT(*) FROM audit_event WHERE tenant_id = ?1 GROUP BY severity"
        )?;
        let by_severity: std::collections::BTreeMap<String, u64> = statement
            .query_map([tenant_id.to_string()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
            })?
            .collect::<Result<_, _>>()?;
        Ok(AuditSnapshot {
            tenant_id,
            total_events: total as u64,
            by_event_type: by_type,
            by_severity,
            from: None,
            to: None,
        })
    }
}

fn u64_i64(value: u64) -> AuditResult<i64> {
    i64::try_from(value)
        .map_err(|_| AuditError::Validation("audit integer exceeds SQLite range".into()))
}

fn list_ids_sync(connection: &Connection, query: &AuditQuery) -> AuditResult<Vec<String>> {
    let mut sql = String::from("SELECT id FROM audit_event");
    let mut clauses: Vec<String> = Vec::new();
    if let Some(tenant_id) = &query.tenant_id {
        clauses.push(format!("tenant_id = '{}'", tenant_id));
    }
    if let Some(actor) = &query.actor {
        clauses.push(format!("actor = '{}'", actor.replace('\'', "''")));
    }
    if let Some(event_type) = &query.event_type {
        clauses.push(format!("event_type = '{}'", event_type.as_str()));
    }
    if let Some(action) = &query.action {
        clauses.push(format!("action = '{}'", action.replace('\'', "''")));
    }
    if let Some(severity) = &query.severity {
        clauses.push(format!("severity = '{}'", severity.as_str()));
    }
    if let Some(from) = &query.from {
        clauses.push(format!("occurred_at >= '{}'", from.to_rfc3339()));
    }
    if let Some(to) = &query.to {
        clauses.push(format!("occurred_at <= '{}'", to.to_rfc3339()));
    }
    if !clauses.is_empty() {
        sql.push_str(&format!(" WHERE {}", clauses.join(" AND ")));
    }
    sql.push_str(&format!(" ORDER BY occurred_at DESC, id LIMIT {} OFFSET {}", query.limit, query.offset));
    let mut statement = connection.prepare(&sql)?;
    let result: Vec<String> = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(result)
}