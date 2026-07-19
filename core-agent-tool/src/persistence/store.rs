use std::collections::BTreeSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension};
use tokio::task;
use uuid::Uuid;

use crate::domain::{
    PermissionDecision, ToolCapability, ToolDefinition, ToolExecutionRecord, ToolLifecycleStatus,
    ToolPermissionRule, ToolProviderDefinition, ToolProviderKind, ToolRequest,
};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{ToolCatalog, ToolLifecycle, ToolPermission, ToolPermissionStore};

use super::schema::SCHEMA_SQL;

#[derive(Clone)]
pub struct SqliteToolStore {
    pool: Pool<SqliteConnectionManager>,
}

impl SqliteToolStore {
    pub fn new(path: &str) -> ToolRuntimeResult<Self> {
        let manager = if path == ":memory:" {
            SqliteConnectionManager::memory()
        } else {
            SqliteConnectionManager::file(path)
        };
        let mut builder = Pool::builder();
        if path == ":memory:" {
            builder = builder.max_size(1);
        }
        let pool = builder
            .build(manager)
            .map_err(|error| ToolError::Persistence(error.to_string()))?;
        {
            let connection = pool
                .get()
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            connection
                .execute_batch(SCHEMA_SQL)
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            migrate_audit_columns(&connection)?;
        }
        Ok(Self { pool })
    }

    pub async fn find_execution(
        &self,
        request_id: Uuid,
    ) -> ToolRuntimeResult<Option<ToolExecutionRecord>> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let connection = connection(&pool)?;
            connection
                .query_row(
                    "SELECT id, request_id, tool_key, provider_key, session_id, subject,
                            status, latency_ms, error_kind, metadata, started_at, completed_at,
                            created_at, updated_at
                     FROM tool_execution WHERE request_id=?1",
                    rusqlite::params![request_id.to_string()],
                    map_execution,
                )
                .optional()
                .map_err(|error| ToolError::Persistence(error.to_string()))
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }

    pub async fn list_executions(
        &self,
        offset: u64,
        limit: u64,
    ) -> ToolRuntimeResult<Vec<ToolExecutionRecord>> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let connection = connection(&pool)?;
            let mut statement = connection
                .prepare(
                    "SELECT id, request_id, tool_key, provider_key, session_id, subject,
                            status, latency_ms, error_kind, metadata, started_at, completed_at,
                            created_at, updated_at
                     FROM tool_execution ORDER BY created_at DESC, rowid DESC LIMIT ?1 OFFSET ?2",
                )
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            let records = statement
                .query_map(
                    rusqlite::params![to_i64(limit, "limit")?, to_i64(offset, "offset")?],
                    map_execution,
                )
                .map_err(|error| ToolError::Persistence(error.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            Ok(records)
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }

    pub async fn execution_count(&self) -> ToolRuntimeResult<u64> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let connection = connection(&pool)?;
            let count: i64 = connection
                .query_row("SELECT COUNT(*) FROM tool_execution", [], |row| row.get(0))
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            u64::try_from(count)
                .map_err(|_| ToolError::Persistence("negative execution count".into()))
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }
}

#[async_trait]
impl ToolCatalog for SqliteToolStore {
    async fn upsert_provider(&self, provider: &ToolProviderDefinition) -> ToolRuntimeResult<()> {
        provider.validate()?;
        let pool = self.pool.clone();
        let provider = provider.clone();
        let metadata = encode_json(&provider.metadata)?;
        task::spawn_blocking(move || {
            connection(&pool)?
                .execute(
                    "INSERT INTO tool_provider (
                        id, provider_key, name, provider_kind, enabled, metadata,
                        created_at, updated_at, create_time, update_time, create_user, update_user
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?7, ?8, 'system', 'system')
                     ON CONFLICT(provider_key) DO UPDATE SET
                        name=excluded.name, provider_kind=excluded.provider_kind,
                        enabled=excluded.enabled, metadata=excluded.metadata,
                        updated_at=excluded.updated_at, update_time=excluded.update_time,
                        update_user=excluded.update_user",
                    rusqlite::params![
                        provider.id.to_string(),
                        provider.key,
                        provider.name,
                        provider.kind.as_str(),
                        bool_i64(provider.enabled),
                        metadata,
                        provider.created_at.to_rfc3339(),
                        provider.updated_at.to_rfc3339(),
                    ],
                )
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }

    async fn find_provider(&self, key: &str) -> ToolRuntimeResult<Option<ToolProviderDefinition>> {
        let pool = self.pool.clone();
        let key = key.to_owned();
        task::spawn_blocking(move || {
            connection(&pool)?
                .query_row(
                    "SELECT id, provider_key, name, provider_kind, enabled, metadata,
                            created_at, updated_at FROM tool_provider WHERE provider_key=?1",
                    rusqlite::params![key],
                    map_provider,
                )
                .optional()
                .map_err(|error| ToolError::Persistence(error.to_string()))
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }

    async fn list_providers(&self) -> ToolRuntimeResult<Vec<ToolProviderDefinition>> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let connection = connection(&pool)?;
            let mut statement = connection
                .prepare(
                    "SELECT id, provider_key, name, provider_kind, enabled, metadata,
                            created_at, updated_at FROM tool_provider ORDER BY provider_key",
                )
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            let providers = statement
                .query_map([], map_provider)
                .map_err(|error| ToolError::Persistence(error.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            Ok(providers)
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }

    async fn remove_provider(&self, key: &str) -> ToolRuntimeResult<bool> {
        delete_by_key(&self.pool, "tool_provider", "provider_key", key).await
    }

    async fn upsert_tool(&self, tool: &ToolDefinition) -> ToolRuntimeResult<()> {
        tool.validate()?;
        let pool = self.pool.clone();
        let tool = tool.clone();
        let input_schema = encode_json(&tool.input_schema)?;
        let tags = encode_json(&tool.tags)?;
        let capabilities = encode_json(&tool.capabilities)?;
        let metadata = encode_json(&tool.metadata)?;
        task::spawn_blocking(move || {
            connection(&pool)?
                .execute(
                    "INSERT INTO tool (
                        id, tool_key, provider_key, name, description, input_schema, version,
                        category, icon, tags, capabilities, default_permission, timeout_ms,
                        enabled, metadata, created_at, updated_at,
                        create_time, update_time, create_user, update_user
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                               ?14, ?15, ?16, ?17, ?16, ?17, 'system', 'system')
                     ON CONFLICT(tool_key) DO UPDATE SET
                        provider_key=excluded.provider_key, name=excluded.name,
                        description=excluded.description, input_schema=excluded.input_schema,
                        version=excluded.version, category=excluded.category, icon=excluded.icon,
                        tags=excluded.tags, capabilities=excluded.capabilities,
                        default_permission=excluded.default_permission,
                        timeout_ms=excluded.timeout_ms, enabled=excluded.enabled,
                        metadata=excluded.metadata, updated_at=excluded.updated_at,
                        update_time=excluded.update_time, update_user=excluded.update_user",
                    rusqlite::params![
                        tool.id.to_string(),
                        tool.key,
                        tool.provider_key,
                        tool.name,
                        tool.description,
                        input_schema,
                        tool.version,
                        tool.category,
                        tool.icon,
                        tags,
                        capabilities,
                        tool.default_permission.as_str(),
                        to_i64(tool.timeout_ms, "timeout_ms")?,
                        bool_i64(tool.enabled),
                        metadata,
                        tool.created_at.to_rfc3339(),
                        tool.updated_at.to_rfc3339(),
                    ],
                )
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }

    async fn find_tool(&self, key: &str) -> ToolRuntimeResult<Option<ToolDefinition>> {
        let pool = self.pool.clone();
        let key = key.to_owned();
        task::spawn_blocking(move || {
            connection(&pool)?
                .query_row(TOOL_SELECT_BY_KEY, rusqlite::params![key], map_tool)
                .optional()
                .map_err(|error| ToolError::Persistence(error.to_string()))
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }

    async fn list_tools(&self) -> ToolRuntimeResult<Vec<ToolDefinition>> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let connection = connection(&pool)?;
            let mut statement = connection
                .prepare(TOOL_SELECT_ALL)
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            let tools = statement
                .query_map([], map_tool)
                .map_err(|error| ToolError::Persistence(error.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            Ok(tools)
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }

    async fn remove_tool(&self, key: &str) -> ToolRuntimeResult<bool> {
        delete_by_key(&self.pool, "tool", "tool_key", key).await
    }

    async fn find_by_capability(
        &self,
        capability: &ToolCapability,
        include_descendants: bool,
    ) -> ToolRuntimeResult<Vec<ToolDefinition>> {
        Ok(self
            .list_tools()
            .await?
            .into_iter()
            .filter(|tool| {
                tool.enabled
                    && tool.capabilities.iter().any(|candidate| {
                        candidate == capability
                            || (include_descendants
                                && candidate.is_same_or_descendant_of(capability))
                    })
            })
            .collect())
    }

    async fn categories(&self) -> ToolRuntimeResult<Vec<String>> {
        Ok(self
            .list_tools()
            .await?
            .into_iter()
            .filter(|tool| tool.enabled)
            .map(|tool| tool.category)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }
}

#[async_trait]
impl ToolLifecycle for SqliteToolStore {
    async fn transition(&self, record: &ToolExecutionRecord) -> ToolRuntimeResult<()> {
        record.validate()?;
        let pool = self.pool.clone();
        let record = record.clone();
        let metadata = encode_json(&record.metadata)?;
        task::spawn_blocking(move || {
            let mut connection = connection(&pool)?;
            let transaction = connection
                .transaction()
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            if record.status == ToolLifecycleStatus::Created {
                transaction
                    .execute(
                        "INSERT INTO tool_execution (
                            id, request_id, tool_key, provider_key, session_id, subject, status,
                            latency_ms, error_kind, metadata, started_at, completed_at,
                            created_at, updated_at, create_time, update_time, create_user, update_user
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                                   ?13, ?14, ?13, ?14, 'system', 'system')",
                        rusqlite::params![
                            record.id.to_string(),
                            record.request_id.to_string(),
                            record.tool_key,
                            record.provider_key,
                            record.session_id.map(|value| value.to_string()),
                            record.subject,
                            record.status.as_str(),
                            to_i64(record.latency_ms, "latency_ms")?,
                            record.error_kind,
                            metadata,
                            record.started_at.map(|value| value.to_rfc3339()),
                            record.completed_at.map(|value| value.to_rfc3339()),
                            record.created_at.to_rfc3339(),
                            record.updated_at.to_rfc3339(),
                        ],
                    )
                    .map_err(|error| ToolError::Persistence(error.to_string()))?;
            } else {
                let existing = transaction
                    .query_row(
                        "SELECT id, tool_key, provider_key, session_id, subject, status
                         FROM tool_execution WHERE request_id=?1",
                        rusqlite::params![record.request_id.to_string()],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, String>(2)?,
                                row.get::<_, Option<String>>(3)?,
                                row.get::<_, Option<String>>(4)?,
                                row.get::<_, String>(5)?,
                            ))
                        },
                    )
                    .optional()
                    .map_err(|error| ToolError::Persistence(error.to_string()))?
                    .ok_or_else(|| {
                        ToolError::Lifecycle(format!(
                            "execution {} has no CREATED record",
                            record.request_id
                        ))
                    })?;
                let existing_status = ToolLifecycleStatus::parse(&existing.5).ok_or_else(|| {
                    ToolError::Persistence(format!(
                        "execution {} has invalid status {}",
                        record.request_id, existing.5
                    ))
                })?;
                let session_id = record.session_id.map(|value| value.to_string());
                if existing.0 != record.id.to_string()
                    || existing.1 != record.tool_key
                    || existing.2 != record.provider_key
                    || existing.3 != session_id
                    || existing.4 != record.subject
                {
                    return Err(ToolError::Lifecycle(
                        "execution identity changed during lifecycle".into(),
                    ));
                }
                if !existing_status.can_transition_to(record.status) {
                    return Err(ToolError::Lifecycle(format!(
                        "invalid persisted transition {} -> {}",
                        existing_status.as_str(),
                        record.status.as_str()
                    )));
                }
                let updated = transaction
                    .execute(
                        "UPDATE tool_execution SET
                            status=?2, latency_ms=?3, error_kind=?4,
                            started_at=?5, completed_at=?6, updated_at=?7,
                            update_time=?7, update_user='system'
                         WHERE request_id=?1",
                        rusqlite::params![
                            record.request_id.to_string(),
                            record.status.as_str(),
                            to_i64(record.latency_ms, "latency_ms")?,
                            record.error_kind,
                            record.started_at.map(|value| value.to_rfc3339()),
                            record.completed_at.map(|value| value.to_rfc3339()),
                            record.updated_at.to_rfc3339(),
                        ],
                    )
                    .map_err(|error| ToolError::Persistence(error.to_string()))?;
                if updated != 1 {
                    return Err(ToolError::Lifecycle(
                        "execution lifecycle update did not affect exactly one row".into(),
                    ));
                }
            }
            transaction
                .commit()
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }
}

#[async_trait]
impl ToolPermissionStore for SqliteToolStore {
    async fn upsert_permission(&self, rule: &ToolPermissionRule) -> ToolRuntimeResult<()> {
        rule.validate()?;
        let pool = self.pool.clone();
        let rule = rule.clone();
        let metadata = encode_json(&rule.metadata)?;
        task::spawn_blocking(move || {
            connection(&pool)?
                .execute(
                    "INSERT INTO tool_permission (
                        id, tool_key, capability, subject, decision, priority, enabled, metadata,
                        created_at, updated_at, create_time, update_time, create_user, update_user
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?9, ?10, 'system', 'system')
                     ON CONFLICT(id) DO UPDATE SET
                        tool_key=excluded.tool_key, capability=excluded.capability,
                        subject=excluded.subject, decision=excluded.decision,
                        priority=excluded.priority, enabled=excluded.enabled,
                        metadata=excluded.metadata, updated_at=excluded.updated_at,
                        update_time=excluded.update_time, update_user=excluded.update_user",
                    rusqlite::params![
                        rule.id.to_string(),
                        rule.tool_key,
                        rule.capability.map(|value| value.to_string()),
                        rule.subject,
                        rule.decision.as_str(),
                        i64::from(rule.priority),
                        bool_i64(rule.enabled),
                        metadata,
                        rule.created_at.to_rfc3339(),
                        rule.updated_at.to_rfc3339(),
                    ],
                )
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }

    async fn list_permissions(&self) -> ToolRuntimeResult<Vec<ToolPermissionRule>> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let connection = connection(&pool)?;
            let mut statement = connection
                .prepare(
                    "SELECT id, tool_key, capability, subject, decision, priority, enabled,
                            metadata, created_at, updated_at
                     FROM tool_permission ORDER BY priority DESC, created_at, id",
                )
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            let rules = statement
                .query_map([], map_permission)
                .map_err(|error| ToolError::Persistence(error.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| ToolError::Persistence(error.to_string()))?;
            Ok(rules)
        })
        .await
        .map_err(|error| ToolError::Internal(error.to_string()))?
    }

    async fn remove_permission(&self, id: Uuid) -> ToolRuntimeResult<bool> {
        delete_by_key(&self.pool, "tool_permission", "id", &id.to_string()).await
    }
}

#[async_trait]
impl ToolPermission for SqliteToolStore {
    async fn check(
        &self,
        request: &ToolRequest,
        tool: &ToolDefinition,
    ) -> ToolRuntimeResult<PermissionDecision> {
        let mut matches = self
            .list_permissions()
            .await?
            .into_iter()
            .filter(|rule| {
                rule.enabled
                    && rule
                        .subject
                        .as_ref()
                        .is_none_or(|subject| request.subject.as_ref() == Some(subject))
                    && (rule.tool_key.as_ref() == Some(&tool.key)
                        || rule.capability.as_ref().is_some_and(|capability| {
                            tool.capabilities
                                .iter()
                                .any(|candidate| candidate.is_same_or_descendant_of(capability))
                        }))
            })
            .collect::<Vec<_>>();
        matches.sort_by_key(|rule| std::cmp::Reverse(permission_score(rule)));
        Ok(matches
            .first()
            .map_or(tool.default_permission, |rule| rule.decision))
    }
}

fn permission_score(rule: &ToolPermissionRule) -> (i32, bool, bool, u8, Uuid) {
    (
        rule.priority,
        rule.subject.is_some(),
        rule.tool_key.is_some(),
        match rule.decision {
            PermissionDecision::Deny => 2,
            PermissionDecision::Ask => 1,
            PermissionDecision::Allow => 0,
        },
        rule.id,
    )
}

const TOOL_SELECT_ALL: &str =
    "SELECT id, tool_key, provider_key, name, description, input_schema, version, category,
            icon, tags, capabilities, default_permission, timeout_ms, enabled, metadata,
            created_at, updated_at FROM tool ORDER BY tool_key";
const TOOL_SELECT_BY_KEY: &str =
    "SELECT id, tool_key, provider_key, name, description, input_schema, version, category,
            icon, tags, capabilities, default_permission, timeout_ms, enabled, metadata,
            created_at, updated_at FROM tool WHERE tool_key=?1";

fn map_provider(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolProviderDefinition> {
    let provider = ToolProviderDefinition {
        id: parse_uuid(row.get(0)?, 0)?,
        key: row.get(1)?,
        name: row.get(2)?,
        kind: ToolProviderKind::parse(&row.get::<_, String>(3)?),
        enabled: parse_bool(row.get(4)?, 4)?,
        metadata: parse_json(&row.get::<_, String>(5)?, 5)?,
        created_at: parse_time(row.get(6)?, 6)?,
        updated_at: parse_time(row.get(7)?, 7)?,
    };
    provider
        .validate()
        .map_err(|error| conversion_error(0, Type::Text, error.to_string()))?;
    Ok(provider)
}

fn map_tool(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolDefinition> {
    let permission_raw: String = row.get(11)?;
    let tool = ToolDefinition {
        id: parse_uuid(row.get(0)?, 0)?,
        key: row.get(1)?,
        provider_key: row.get(2)?,
        name: row.get(3)?,
        description: row.get(4)?,
        input_schema: parse_json(&row.get::<_, String>(5)?, 5)?,
        version: row.get(6)?,
        category: row.get(7)?,
        icon: row.get(8)?,
        tags: parse_json(&row.get::<_, String>(9)?, 9)?,
        capabilities: parse_json(&row.get::<_, String>(10)?, 10)?,
        default_permission: PermissionDecision::parse(&permission_raw).ok_or_else(|| {
            conversion_error(
                11,
                Type::Text,
                format!("invalid permission {permission_raw}"),
            )
        })?,
        timeout_ms: parse_u64(row.get(12)?, 12)?,
        enabled: parse_bool(row.get(13)?, 13)?,
        metadata: parse_json(&row.get::<_, String>(14)?, 14)?,
        created_at: parse_time(row.get(15)?, 15)?,
        updated_at: parse_time(row.get(16)?, 16)?,
    };
    tool.validate()
        .map_err(|error| conversion_error(0, Type::Text, error.to_string()))?;
    Ok(tool)
}

fn map_execution(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolExecutionRecord> {
    let status_raw: String = row.get(6)?;
    let record = ToolExecutionRecord {
        id: parse_uuid(row.get(0)?, 0)?,
        request_id: parse_uuid(row.get(1)?, 1)?,
        tool_key: row.get(2)?,
        provider_key: row.get(3)?,
        session_id: row
            .get::<_, Option<String>>(4)?
            .map(|value| parse_uuid(value, 4))
            .transpose()?,
        subject: row.get(5)?,
        status: ToolLifecycleStatus::parse(&status_raw).ok_or_else(|| {
            conversion_error(6, Type::Text, format!("invalid status {status_raw}"))
        })?,
        latency_ms: parse_u64(row.get(7)?, 7)?,
        error_kind: row.get(8)?,
        metadata: parse_json(&row.get::<_, String>(9)?, 9)?,
        started_at: row
            .get::<_, Option<String>>(10)?
            .map(|value| parse_time(value, 10))
            .transpose()?,
        completed_at: row
            .get::<_, Option<String>>(11)?
            .map(|value| parse_time(value, 11))
            .transpose()?,
        created_at: parse_time(row.get(12)?, 12)?,
        updated_at: parse_time(row.get(13)?, 13)?,
    };
    record
        .validate()
        .map_err(|error| conversion_error(0, Type::Text, error.to_string()))?;
    Ok(record)
}

fn map_permission(row: &rusqlite::Row<'_>) -> rusqlite::Result<ToolPermissionRule> {
    let decision_raw: String = row.get(4)?;
    let rule = ToolPermissionRule {
        id: parse_uuid(row.get(0)?, 0)?,
        tool_key: row.get(1)?,
        capability: row
            .get::<_, Option<String>>(2)?
            .map(|value| {
                ToolCapability::new(value)
                    .map_err(|error| conversion_error(2, Type::Text, error.to_string()))
            })
            .transpose()?,
        subject: row.get(3)?,
        decision: PermissionDecision::parse(&decision_raw).ok_or_else(|| {
            conversion_error(4, Type::Text, format!("invalid decision {decision_raw}"))
        })?,
        priority: parse_i32(row.get(5)?, 5)?,
        enabled: parse_bool(row.get(6)?, 6)?,
        metadata: parse_json(&row.get::<_, String>(7)?, 7)?,
        created_at: parse_time(row.get(8)?, 8)?,
        updated_at: parse_time(row.get(9)?, 9)?,
    };
    rule.validate()
        .map_err(|error| conversion_error(0, Type::Text, error.to_string()))?;
    Ok(rule)
}

fn connection(
    pool: &Pool<SqliteConnectionManager>,
) -> ToolRuntimeResult<r2d2::PooledConnection<SqliteConnectionManager>> {
    pool.get()
        .map_err(|error| ToolError::Persistence(error.to_string()))
}

async fn delete_by_key(
    pool: &Pool<SqliteConnectionManager>,
    table: &'static str,
    column: &'static str,
    key: &str,
) -> ToolRuntimeResult<bool> {
    let pool = pool.clone();
    let key = key.to_owned();
    task::spawn_blocking(move || {
        let sql = format!("DELETE FROM {table} WHERE {column}=?1");
        connection(&pool)?
            .execute(&sql, rusqlite::params![key])
            .map(|count| count > 0)
            .map_err(|error| ToolError::Persistence(error.to_string()))
    })
    .await
    .map_err(|error| ToolError::Internal(error.to_string()))?
}

fn encode_json<T: serde::Serialize>(value: &T) -> ToolRuntimeResult<String> {
    serde_json::to_string(value).map_err(|error| ToolError::Serialization(error.to_string()))
}

fn parse_json<T: serde::de::DeserializeOwned>(value: &str, column: usize) -> rusqlite::Result<T> {
    serde_json::from_str(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
    })
}

fn parse_uuid(value: String, column: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
    })
}

fn parse_time(value: String, column: usize) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
        })
}

fn parse_bool(value: i64, column: usize) -> rusqlite::Result<bool> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(conversion_error(
            column,
            Type::Integer,
            format!("invalid boolean {value}"),
        )),
    }
}

fn parse_u64(value: i64, column: usize) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, Box::new(error))
    })
}

fn parse_i32(value: i64, column: usize) -> rusqlite::Result<i32> {
    i32::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, Box::new(error))
    })
}

fn to_i64(value: u64, name: &str) -> ToolRuntimeResult<i64> {
    i64::try_from(value)
        .map_err(|_| ToolError::InvalidArgument(format!("{name} exceeds SQLite INTEGER")))
}

fn bool_i64(value: bool) -> i64 {
    i64::from(value)
}

fn conversion_error(column: usize, kind: Type, message: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        kind,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn migrate_audit_columns(connection: &Connection) -> ToolRuntimeResult<()> {
    for table in ["tool_provider", "tool", "tool_execution", "tool_permission"] {
        ensure_column(connection, table, "create_time", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(connection, table, "update_time", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(
            connection,
            table,
            "create_user",
            "TEXT NOT NULL DEFAULT 'system'",
        )?;
        ensure_column(
            connection,
            table,
            "update_user",
            "TEXT NOT NULL DEFAULT 'system'",
        )?;
        connection
            .execute(
                &format!(
                    "UPDATE {table} SET
                       create_time=CASE WHEN create_time='' THEN created_at ELSE create_time END,
                       update_time=CASE WHEN update_time='' THEN updated_at ELSE update_time END,
                       create_user=CASE WHEN create_user='' THEN 'system' ELSE create_user END,
                       update_user=CASE WHEN update_user='' THEN 'system' ELSE update_user END"
                ),
                [],
            )
            .map_err(|error| ToolError::Persistence(error.to_string()))?;
    }
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> ToolRuntimeResult<()> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| ToolError::Persistence(error.to_string()))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| ToolError::Persistence(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ToolError::Persistence(error.to_string()))?;
    if !columns.iter().any(|existing| existing == column) {
        connection
            .execute(
                &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
                [],
            )
            .map_err(|error| ToolError::Persistence(error.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::domain::PermissionDecision;
    use tempfile::tempdir;

    #[tokio::test]
    async fn sqlite_round_trips_catalog_permission_and_execution() {
        let store = SqliteToolStore::new(":memory:").unwrap();
        let provider = ToolProviderDefinition::new("builtin", "Builtin", ToolProviderKind::Builtin);
        store.upsert_provider(&provider).await.unwrap();
        let mut tool =
            ToolDefinition::new("builtin", "echo", "1", serde_json::json!({"type":"object"}));
        tool.capabilities
            .insert(ToolCapability::new("utility.echo").unwrap());
        store.upsert_tool(&tool).await.unwrap();
        let rule = ToolPermissionRule::for_tool(&tool.key, PermissionDecision::Allow);
        store.upsert_permission(&rule).await.unwrap();
        let mut record = ToolExecutionRecord::new(
            Uuid::new_v4(),
            &tool.key,
            "builtin",
            None,
            None,
            &BTreeMap::new(),
        );
        store.transition(&record).await.unwrap();
        record.transition(ToolLifecycleStatus::Ready).unwrap();
        store.transition(&record).await.unwrap();

        assert_eq!(store.list_tools().await.unwrap().len(), 1);
        assert_eq!(store.list_permissions().await.unwrap().len(), 1);
        assert_eq!(store.execution_count().await.unwrap(), 1);
    }

    #[test]
    fn all_tables_have_required_audit_columns_and_indexes() {
        let store = SqliteToolStore::new(":memory:").unwrap();
        let connection = store.pool.get().unwrap();
        for table in ["tool", "tool_provider", "tool_execution", "tool_permission"] {
            let mut statement = connection
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<BTreeSet<_>, _>>()
                .unwrap();
            for required in [
                "id",
                "create_time",
                "update_time",
                "create_user",
                "update_user",
            ] {
                assert!(columns.contains(required), "{table}.{required} is missing");
            }
        }
        let indexes: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_tool_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(indexes >= 10);
    }

    #[test]
    fn legacy_tool_table_migrates_audit_columns_additively() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("legacy.db");
        let connection = Connection::open(&path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE tool (
                    id TEXT PRIMARY KEY NOT NULL,
                    tool_key TEXT NOT NULL UNIQUE,
                    provider_key TEXT NOT NULL,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL DEFAULT '',
                    input_schema TEXT NOT NULL,
                    version TEXT NOT NULL,
                    category TEXT NOT NULL,
                    icon TEXT,
                    tags TEXT NOT NULL DEFAULT '[]',
                    capabilities TEXT NOT NULL DEFAULT '[]',
                    default_permission TEXT NOT NULL DEFAULT 'ASK',
                    timeout_ms INTEGER NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 1,
                    metadata TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                 );",
            )
            .unwrap();
        drop(connection);
        let path = path.to_string_lossy().into_owned();
        let store = SqliteToolStore::new(&path).unwrap();
        let connection = store.pool.get().unwrap();
        let columns = connection
            .prepare("PRAGMA table_info(tool)")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<BTreeSet<_>, _>>()
            .unwrap();
        for required in ["create_time", "update_time", "create_user", "update_user"] {
            assert!(columns.contains(required));
        }
    }

    #[tokio::test]
    async fn corrupt_catalog_rows_are_reported() {
        let store = SqliteToolStore::new(":memory:").unwrap();
        let connection = store.pool.get().unwrap();
        let now = Utc::now().to_rfc3339();
        connection
            .execute(
                "INSERT INTO tool_provider (
                    id, provider_key, name, provider_kind, enabled, metadata,
                    created_at, updated_at, create_time, update_time, create_user, update_user
                 ) VALUES ('not-a-uuid', 'broken', 'Broken', 'BUILTIN', 1, '{}',
                           ?1, ?1, ?1, ?1, 'system', 'system')",
                rusqlite::params![now],
            )
            .unwrap();
        drop(connection);
        assert!(store.list_providers().await.is_err());
    }

    #[tokio::test]
    async fn permission_resolution_is_deterministic_and_specific() {
        let store = SqliteToolStore::new(":memory:").unwrap();
        let mut tool =
            ToolDefinition::new("builtin", "read", "1", serde_json::json!({"type":"object"}));
        tool.capabilities
            .insert(ToolCapability::new("filesystem.read").unwrap());
        let mut broad = ToolPermissionRule {
            id: Uuid::new_v4(),
            tool_key: None,
            capability: Some(ToolCapability::new("filesystem").unwrap()),
            subject: None,
            decision: PermissionDecision::Allow,
            priority: 0,
            enabled: true,
            metadata: BTreeMap::new(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        store.upsert_permission(&broad).await.unwrap();
        broad.id = Uuid::new_v4();
        broad.tool_key = Some(tool.key.clone());
        broad.capability = None;
        broad.decision = PermissionDecision::Deny;
        store.upsert_permission(&broad).await.unwrap();
        let request = ToolRequest::new(&tool.key, serde_json::json!({}));
        assert_eq!(
            store.check(&request, &tool).await.unwrap(),
            PermissionDecision::Deny
        );

        broad.id = Uuid::new_v4();
        broad.decision = PermissionDecision::Allow;
        store.upsert_permission(&broad).await.unwrap();
        assert_eq!(
            store.check(&request, &tool).await.unwrap(),
            PermissionDecision::Deny,
            "deny must win an equally specific conflicting rule"
        );
    }
}
