use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension};
use tokio::task;
use uuid::Uuid;

use crate::domain::{
    ModelCapability, ModelLimits, ModelOperation, ModelPerformance, ModelPolicy, ModelPricing,
    ModelProfile, ModelUsage, ProviderDefinition, UsageRecord,
};
use crate::error::{ModelError, ModelResult};
use crate::infrastructure::{ModelCatalog, UsageCollector};

use super::schema::SCHEMA_SQL;

/// SQLite implementation shared by ModelCatalog and UsageCollector.
pub struct SqliteModelStore {
    pool: Pool<SqliteConnectionManager>,
}

impl SqliteModelStore {
    pub fn new(path: &str) -> ModelResult<Self> {
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
            .map_err(|error| ModelError::Persistence(error.to_string()))?;
        {
            let conn = pool
                .get()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            conn.execute_batch(SCHEMA_SQL)
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            migrate_audit_columns(&conn)?;
        }
        Ok(Self { pool })
    }

    pub async fn list_usage(&self, offset: u64, limit: u64) -> ModelResult<Vec<UsageRecord>> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            let mut statement = conn
                .prepare(
                    "SELECT id, request_id, operation, provider_key, model_name, profile_key,
                            prompt_tokens, completion_tokens, cache_tokens, total_tokens,
                            latency_ms, cost, success, error_kind, metadata, created_at
                     FROM model_usage ORDER BY created_at DESC, rowid DESC LIMIT ?1 OFFSET ?2",
                )
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            let records = statement
                .query_map(
                    rusqlite::params![to_i64(limit, "limit")?, to_i64(offset, "offset")?],
                    map_usage,
                )
                .map_err(|error| ModelError::Persistence(error.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            Ok(records)
        })
        .await
        .map_err(|error| ModelError::Internal(error.to_string()))?
    }

    pub async fn usage_count(&self) -> ModelResult<u64> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM model_usage", [], |row| row.get(0))
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            u64::try_from(count).map_err(|_| ModelError::Persistence("negative usage count".into()))
        })
        .await
        .map_err(|error| ModelError::Internal(error.to_string()))?
    }
}

#[async_trait]
impl ModelCatalog for SqliteModelStore {
    async fn upsert_provider(&self, provider: &ProviderDefinition) -> ModelResult<()> {
        provider.validate()?;
        let pool = self.pool.clone();
        let provider = provider.clone();
        let metadata = encode_json(&provider.metadata)?;
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            conn.execute(
                "INSERT INTO model_provider (
                    id, provider_key, name, endpoint, enabled, timeout_ms, max_retries,
                    rate_limit_per_minute, metadata, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?10, ?11, 'system', 'system')
                 ON CONFLICT(provider_key) DO UPDATE SET
                    name=excluded.name, endpoint=excluded.endpoint, enabled=excluded.enabled,
                    timeout_ms=excluded.timeout_ms, max_retries=excluded.max_retries,
                    rate_limit_per_minute=excluded.rate_limit_per_minute,
                    metadata=excluded.metadata, updated_at=excluded.updated_at,
                    update_time=excluded.update_time, update_user=excluded.update_user",
                rusqlite::params![
                    provider.id.to_string(),
                    provider.key,
                    provider.name,
                    provider.endpoint,
                    bool_i64(provider.enabled),
                    to_i64(provider.timeout_ms, "timeout_ms")?,
                    i64::from(provider.max_retries),
                    provider.rate_limit_per_minute.map(i64::from),
                    metadata,
                    provider.created_at.to_rfc3339(),
                    provider.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|error| ModelError::Persistence(error.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|error| ModelError::Internal(error.to_string()))?
    }

    async fn find_provider(&self, key: &str) -> ModelResult<Option<ProviderDefinition>> {
        let pool = self.pool.clone();
        let key = key.to_owned();
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            conn.query_row(
                "SELECT id, provider_key, name, endpoint, enabled, timeout_ms, max_retries,
                        rate_limit_per_minute, metadata, created_at, updated_at
                 FROM model_provider WHERE provider_key=?1",
                rusqlite::params![key],
                map_provider,
            )
            .optional()
            .map_err(|error| ModelError::Persistence(error.to_string()))
        })
        .await
        .map_err(|error| ModelError::Internal(error.to_string()))?
    }

    async fn list_providers(&self) -> ModelResult<Vec<ProviderDefinition>> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            let mut statement = conn
                .prepare(
                    "SELECT id, provider_key, name, endpoint, enabled, timeout_ms, max_retries,
                            rate_limit_per_minute, metadata, created_at, updated_at
                     FROM model_provider ORDER BY provider_key",
                )
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            let providers = statement
                .query_map([], map_provider)
                .map_err(|error| ModelError::Persistence(error.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            Ok(providers)
        })
        .await
        .map_err(|error| ModelError::Internal(error.to_string()))?
    }

    async fn upsert_profile(&self, profile: &ModelProfile) -> ModelResult<()> {
        profile.validate()?;
        let pool = self.pool.clone();
        let profile = profile.clone();
        let capabilities = encode_json(&profile.capabilities)?;
        let limits = encode_json(&profile.limits)?;
        let pricing = encode_json(&profile.pricing)?;
        let performance = encode_json(&profile.performance)?;
        let policies = encode_json(&profile.policy)?;
        let metadata = encode_json(&profile.metadata)?;
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            conn.execute(
                "INSERT INTO model (
                    id, profile_key, provider_key, model_name, capabilities, limits, pricing,
                    performance, policies, metadata, priority, enabled, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                           ?13, ?14, 'system', 'system')
                 ON CONFLICT(profile_key) DO UPDATE SET
                    provider_key=excluded.provider_key, model_name=excluded.model_name,
                    capabilities=excluded.capabilities, limits=excluded.limits,
                    pricing=excluded.pricing, performance=excluded.performance,
                    policies=excluded.policies, metadata=excluded.metadata,
                    priority=excluded.priority, enabled=excluded.enabled,
                    updated_at=excluded.updated_at, update_time=excluded.update_time,
                    update_user=excluded.update_user",
                rusqlite::params![
                    profile.id.to_string(),
                    profile.key,
                    profile.provider,
                    profile.model,
                    capabilities,
                    limits,
                    pricing,
                    performance,
                    policies,
                    metadata,
                    i64::from(profile.priority),
                    bool_i64(profile.enabled),
                    profile.created_at.to_rfc3339(),
                    profile.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|error| ModelError::Persistence(error.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|error| ModelError::Internal(error.to_string()))?
    }

    async fn find_profile(&self, key: &str) -> ModelResult<Option<ModelProfile>> {
        let pool = self.pool.clone();
        let key = key.to_owned();
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            conn.query_row(
                "SELECT id, profile_key, provider_key, model_name, capabilities, limits,
                        pricing, performance, policies, metadata, priority, enabled,
                        created_at, updated_at FROM model WHERE profile_key=?1",
                rusqlite::params![key],
                map_profile,
            )
            .optional()
            .map_err(|error| ModelError::Persistence(error.to_string()))
        })
        .await
        .map_err(|error| ModelError::Internal(error.to_string()))?
    }

    async fn list_profiles(&self) -> ModelResult<Vec<ModelProfile>> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            let mut statement = conn
                .prepare(
                    "SELECT id, profile_key, provider_key, model_name, capabilities, limits,
                            pricing, performance, policies, metadata, priority, enabled,
                            created_at, updated_at FROM model ORDER BY priority DESC, profile_key",
                )
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            let profiles = statement
                .query_map([], map_profile)
                .map_err(|error| ModelError::Persistence(error.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            Ok(profiles)
        })
        .await
        .map_err(|error| ModelError::Internal(error.to_string()))?
    }
}

#[async_trait]
impl UsageCollector for SqliteModelStore {
    async fn record(&self, record: &UsageRecord) -> ModelResult<()> {
        record.validate()?;
        let pool = self.pool.clone();
        let record = record.clone();
        let metadata = encode_json(&record.metadata)?;
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            conn.execute(
                "INSERT INTO model_usage (
                    id, request_id, operation, provider_key, model_name, profile_key,
                    prompt_tokens, completion_tokens, cache_tokens, total_tokens, latency_ms,
                    cost, success, error_kind, metadata, created_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                           ?15, ?16, ?16, ?16, 'system', 'system')",
                rusqlite::params![
                    record.id.to_string(),
                    record.request_id.to_string(),
                    record.operation.as_str(),
                    record.provider,
                    record.model,
                    record.profile,
                    to_i64(record.usage.prompt_tokens, "prompt_tokens")?,
                    to_i64(record.usage.completion_tokens, "completion_tokens")?,
                    to_i64(record.usage.cache_tokens, "cache_tokens")?,
                    to_i64(record.usage.total_tokens, "total_tokens")?,
                    to_i64(record.usage.latency_ms, "latency_ms")?,
                    record.usage.cost,
                    bool_i64(record.success),
                    record.error_kind,
                    metadata,
                    record.created_at.to_rfc3339(),
                ],
            )
            .map_err(|error| ModelError::Persistence(error.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|error| ModelError::Internal(error.to_string()))?
    }
}

fn map_provider(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderDefinition> {
    let provider = ProviderDefinition {
        id: parse_uuid(row.get::<_, String>(0)?, 0)?,
        key: row.get(1)?,
        name: row.get(2)?,
        endpoint: row.get(3)?,
        enabled: parse_bool(row.get(4)?, 4)?,
        timeout_ms: parse_u64(row.get(5)?, 5)?,
        max_retries: parse_u32(row.get(6)?, 6)?,
        rate_limit_per_minute: row
            .get::<_, Option<i64>>(7)?
            .map(|value| parse_u32(value, 7))
            .transpose()?,
        metadata: parse_json(&row.get::<_, String>(8)?, 8)?,
        created_at: parse_time(row.get::<_, String>(9)?, 9)?,
        updated_at: parse_time(row.get::<_, String>(10)?, 10)?,
    };
    provider
        .validate()
        .map_err(|error| conversion_error(0, Type::Text, error.to_string()))?;
    Ok(provider)
}

fn map_profile(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModelProfile> {
    let profile = ModelProfile {
        id: parse_uuid(row.get::<_, String>(0)?, 0)?,
        key: row.get(1)?,
        provider: row.get(2)?,
        model: row.get(3)?,
        capabilities: parse_json::<BTreeSet<ModelCapability>>(&row.get::<_, String>(4)?, 4)?,
        limits: parse_json::<ModelLimits>(&row.get::<_, String>(5)?, 5)?,
        pricing: parse_json::<ModelPricing>(&row.get::<_, String>(6)?, 6)?,
        performance: parse_json::<ModelPerformance>(&row.get::<_, String>(7)?, 7)?,
        policy: parse_json::<ModelPolicy>(&row.get::<_, String>(8)?, 8)?,
        metadata: parse_json::<BTreeMap<String, String>>(&row.get::<_, String>(9)?, 9)?,
        priority: parse_i32(row.get(10)?, 10)?,
        enabled: parse_bool(row.get(11)?, 11)?,
        created_at: parse_time(row.get::<_, String>(12)?, 12)?,
        updated_at: parse_time(row.get::<_, String>(13)?, 13)?,
    };
    profile
        .validate()
        .map_err(|error| conversion_error(0, Type::Text, error.to_string()))?;
    Ok(profile)
}

fn map_usage(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsageRecord> {
    let operation_raw: String = row.get(2)?;
    let operation = ModelOperation::parse(&operation_raw).ok_or_else(|| {
        conversion_error(2, Type::Text, format!("invalid operation {operation_raw}"))
    })?;
    let record = UsageRecord {
        id: parse_uuid(row.get::<_, String>(0)?, 0)?,
        request_id: parse_uuid(row.get::<_, String>(1)?, 1)?,
        operation,
        provider: row.get(3)?,
        model: row.get(4)?,
        profile: row.get(5)?,
        usage: ModelUsage {
            prompt_tokens: parse_u64(row.get(6)?, 6)?,
            completion_tokens: parse_u64(row.get(7)?, 7)?,
            cache_tokens: parse_u64(row.get(8)?, 8)?,
            total_tokens: parse_u64(row.get(9)?, 9)?,
            latency_ms: parse_u64(row.get(10)?, 10)?,
            cost: row.get(11)?,
        },
        success: parse_bool(row.get(12)?, 12)?,
        error_kind: row.get(13)?,
        metadata: parse_json::<BTreeMap<String, String>>(&row.get::<_, String>(14)?, 14)?,
        created_at: parse_time(row.get::<_, String>(15)?, 15)?,
    };
    record
        .validate()
        .map_err(|error| conversion_error(0, Type::Text, error.to_string()))?;
    Ok(record)
}

fn encode_json<T: serde::Serialize>(value: &T) -> ModelResult<String> {
    serde_json::to_string(value).map_err(|error| ModelError::Serialization(error.to_string()))
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

fn parse_u32(value: i64, column: usize) -> rusqlite::Result<u32> {
    u32::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, Box::new(error))
    })
}

fn parse_i32(value: i64, column: usize) -> rusqlite::Result<i32> {
    i32::try_from(value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, Box::new(error))
    })
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

fn to_i64(value: u64, field: &str) -> ModelResult<i64> {
    i64::try_from(value)
        .map_err(|_| ModelError::InvalidArgument(format!("{field} exceeds SQLite integer range")))
}

fn bool_i64(value: bool) -> i64 {
    i64::from(value)
}

fn migrate_audit_columns(conn: &Connection) -> ModelResult<()> {
    for table in ["model_provider", "model", "model_usage"] {
        let columns = table_columns(conn, table)?;
        for (column, definition) in [
            ("create_time", "TEXT NOT NULL DEFAULT ''"),
            ("update_time", "TEXT NOT NULL DEFAULT ''"),
            ("create_user", "TEXT NOT NULL DEFAULT 'system'"),
            ("update_user", "TEXT NOT NULL DEFAULT 'system'"),
        ] {
            if !columns.iter().any(|existing| existing == column) {
                conn.execute_batch(&format!(
                    "ALTER TABLE {table} ADD COLUMN {column} {definition}"
                ))
                .map_err(|error| ModelError::Persistence(error.to_string()))?;
            }
        }
        if columns.iter().any(|column| column == "created_at") {
            conn.execute_batch(&format!(
                "UPDATE {table}
                 SET create_time = CASE WHEN create_time = '' THEN created_at ELSE create_time END,
                     update_time = CASE WHEN update_time = '' THEN {} ELSE update_time END",
                if columns.iter().any(|column| column == "updated_at") {
                    "updated_at"
                } else {
                    "created_at"
                }
            ))
            .map_err(|error| ModelError::Persistence(error.to_string()))?;
        }
    }
    Ok(())
}

fn table_columns(conn: &Connection, table: &str) -> ModelResult<Vec<String>> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| ModelError::Persistence(error.to_string()))?;
    let columns = statement
        .query_map([], |row| row.get(1))
        .map_err(|error| ModelError::Persistence(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| ModelError::Persistence(error.to_string()))?;
    Ok(columns)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sqlite_round_trips_catalog_and_usage() {
        let store = SqliteModelStore::new(":memory:").unwrap();
        let provider = ProviderDefinition::new("openai", "OpenAI");
        store.upsert_provider(&provider).await.unwrap();
        let profile =
            ModelProfile::new("coding", "openai", "gpt").with_capability(ModelCapability::Chat);
        store.upsert_profile(&profile).await.unwrap();
        let record = UsageRecord::success(
            Uuid::new_v4(),
            ModelOperation::Generate,
            "openai",
            "gpt",
            "coding",
            ModelUsage {
                prompt_tokens: 3,
                total_tokens: 3,
                ..Default::default()
            },
            BTreeMap::new(),
        );
        store.record(&record).await.unwrap();

        assert_eq!(store.list_providers().await.unwrap(), vec![provider]);
        assert_eq!(store.find_profile("coding").await.unwrap(), Some(profile));
        assert_eq!(store.list_usage(0, 10).await.unwrap(), vec![record]);
    }

    #[test]
    fn all_tables_have_required_audit_columns_and_indexes() {
        let store = SqliteModelStore::new(":memory:").unwrap();
        let conn = store.pool.get().unwrap();
        for table in ["model_provider", "model", "model_usage"] {
            let columns = table_columns(&conn, table).unwrap();
            for required in [
                "id",
                "create_time",
                "update_time",
                "create_user",
                "update_user",
            ] {
                assert!(
                    columns.iter().any(|column| column == required),
                    "{table} missing {required}"
                );
            }
        }
        let index_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_model_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(index_count >= 7);
    }

    #[test]
    fn audit_columns_are_migrated_for_legacy_p2_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE model_provider (id TEXT);
             CREATE TABLE model (id TEXT);
             CREATE TABLE model_usage (id TEXT);",
        )
        .unwrap();
        migrate_audit_columns(&conn).unwrap();
        for table in ["model_provider", "model", "model_usage"] {
            let columns = table_columns(&conn, table).unwrap();
            assert!(columns.iter().any(|column| column == "create_time"));
            assert!(columns.iter().any(|column| column == "update_user"));
        }
    }

    #[tokio::test]
    async fn corrupt_profile_json_returns_error() {
        let store = SqliteModelStore::new(":memory:").unwrap();
        let now = Utc::now().to_rfc3339();
        {
            let conn = store.pool.get().unwrap();
            conn.execute(
                "INSERT INTO model (
                    id, profile_key, provider_key, model_name, capabilities, limits, pricing,
                    performance, policies, metadata, priority, enabled, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, 'bad', 'p', 'm', 'not-json', '{}', '{}', '{}', '{}', '{}',
                           0, 1, ?2, ?2, ?2, ?2, 'system', 'system')",
                rusqlite::params![Uuid::new_v4().to_string(), now],
            )
            .unwrap();
        }
        assert!(store.list_profiles().await.is_err());
    }
}
