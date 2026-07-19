//! SqliteContextSnapshotStore — SQLite 快照存储实现。

use std::collections::HashSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::context::Context;
use crate::error::{ContextError, ContextResult};
use crate::infrastructure::{ContextSnapshotMeta, ContextSnapshotStore};
use crate::persistence::schema::CONTEXT_SNAPSHOT_SCHEMA_SQL;

/// SQLite Context 快照存储。
pub struct SqliteContextSnapshotStore {
    pool: Pool<SqliteConnectionManager>,
}

impl SqliteContextSnapshotStore {
    /// 创建 Store；`:memory:` 使用单连接，避免 SQLite 每连接独立数据库。
    pub fn new(path: &str) -> ContextResult<Self> {
        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::builder()
            .max_size(if path == ":memory:" { 1 } else { 8 })
            .build(manager)
            .map_err(persistence_error)?;

        {
            let conn = pool.get().map_err(persistence_error)?;
            conn.execute_batch(CONTEXT_SNAPSHOT_SCHEMA_SQL)
                .map_err(persistence_error)?;
            migrate_audit_columns(&conn)?;
        }

        Ok(Self { pool })
    }
}

#[async_trait]
impl ContextSnapshotStore for SqliteContextSnapshotStore {
    async fn save_snapshot(&self, context: &Context) -> ContextResult<()> {
        let semantic_hash = context
            .semantic_hash()
            .map_err(|error| ContextError::Serialization(error.to_string()))?;
        if context.hash != semantic_hash {
            return Err(ContextError::Serialization(format!(
                "snapshot {} has a stale semantic hash",
                context.id
            )));
        }
        let id = context.id.to_string();
        let session_id = context.session_id.to_string();
        let conversation_id = context.conversation_id.map(|value| value.to_string());
        let created_at = context.built_at.to_rfc3339();
        let content = serde_json::to_string(context)
            .map_err(|error| ContextError::Serialization(error.to_string()))?;
        let token_count = i64::try_from(context.total_tokens).map_err(|_| {
            ContextError::InvalidArgument("snapshot token_count exceeds SQLite range".into())
        })?;
        let hash = context.hash.clone();
        let build_duration_ms = i64::try_from(context.build_duration_ms).map_err(|_| {
            ContextError::InvalidArgument("snapshot build_duration_ms exceeds SQLite range".into())
        })?;
        let now = Utc::now().to_rfc3339();
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(persistence_error)?;
            conn.execute(
                "INSERT INTO context_snapshot (
                    id, session_id, conversation_id, created_at, content, token_count, hash,
                    build_duration_ms, create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, 'system', 'system')
                 ON CONFLICT(id) DO UPDATE SET
                    session_id = excluded.session_id,
                    conversation_id = excluded.conversation_id,
                    created_at = excluded.created_at,
                    content = excluded.content,
                    token_count = excluded.token_count,
                    hash = excluded.hash,
                    build_duration_ms = excluded.build_duration_ms,
                    update_time = excluded.update_time,
                    update_user = excluded.update_user",
                rusqlite::params![
                    id,
                    session_id,
                    conversation_id,
                    created_at,
                    content,
                    token_count,
                    hash,
                    build_duration_ms,
                    now,
                ],
            )
            .map_err(persistence_error)?;
            Ok::<_, ContextError>(())
        })
        .await
        .map_err(join_error)?
    }

    async fn load_snapshot(&self, id: &Uuid) -> ContextResult<Option<Context>> {
        let id = *id;
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(persistence_error)?;
            let row: Option<(String, String)> = conn
                .query_row(
                    "SELECT content, hash FROM context_snapshot WHERE id = ?1",
                    rusqlite::params![id.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(persistence_error)?;

            let Some((content, stored_hash)) = row else {
                return Ok(None);
            };
            validate_hash(&stored_hash)?;
            let context: Context = serde_json::from_str(&content)
                .map_err(|error| ContextError::Serialization(error.to_string()))?;
            if context.id != id {
                return Err(ContextError::Serialization(format!(
                    "snapshot {} content contains id {}",
                    id, context.id
                )));
            }
            if context.hash != stored_hash {
                return Err(ContextError::Serialization(format!(
                    "snapshot {} hash column does not match content",
                    id
                )));
            }
            Ok(Some(context))
        })
        .await
        .map_err(join_error)?
    }

    async fn list_snapshots(
        &self,
        session_id: &Uuid,
        offset: u64,
        limit: u64,
    ) -> ContextResult<(Vec<ContextSnapshotMeta>, u64)> {
        let session_id = session_id.to_string();
        let offset = i64::try_from(offset)
            .map_err(|_| ContextError::InvalidArgument("offset exceeds SQLite range".into()))?;
        let limit = i64::try_from(limit)
            .map_err(|_| ContextError::InvalidArgument("limit exceeds SQLite range".into()))?;
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(persistence_error)?;
            let total: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM context_snapshot WHERE session_id = ?1",
                    rusqlite::params![&session_id],
                    |row| row.get(0),
                )
                .map_err(persistence_error)?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, conversation_id, created_at, token_count, hash
                     FROM context_snapshot
                     WHERE session_id = ?1
                     ORDER BY created_at DESC, id DESC
                     LIMIT ?2 OFFSET ?3",
                )
                .map_err(persistence_error)?;
            let rows = stmt
                .query_map(rusqlite::params![&session_id, limit, offset], |row| {
                    Ok(SnapshotMetaRow {
                        id: row.get(0)?,
                        session_id: row.get(1)?,
                        conversation_id: row.get(2)?,
                        created_at: row.get(3)?,
                        token_count: row.get(4)?,
                        hash: row.get(5)?,
                    })
                })
                .map_err(persistence_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(persistence_error)?;
            let items = rows
                .into_iter()
                .map(ContextSnapshotMeta::try_from)
                .collect::<ContextResult<Vec<_>>>()?;
            let total = u64::try_from(total).map_err(|_| {
                ContextError::Persistence("snapshot count cannot be negative".into())
            })?;
            Ok::<_, ContextError>((items, total))
        })
        .await
        .map_err(join_error)?
    }

    async fn delete_snapshot(&self, id: &Uuid) -> ContextResult<()> {
        let id = id.to_string();
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(persistence_error)?;
            conn.execute(
                "DELETE FROM context_snapshot WHERE id = ?1",
                rusqlite::params![id],
            )
            .map_err(persistence_error)?;
            Ok::<_, ContextError>(())
        })
        .await
        .map_err(join_error)?
    }

    async fn prune_snapshots(&self, session_id: &Uuid, keep_recent: usize) -> ContextResult<usize> {
        let session_id = session_id.to_string();
        let keep_recent = i64::try_from(keep_recent).map_err(|_| {
            ContextError::InvalidArgument("keep_recent exceeds SQLite range".into())
        })?;
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(persistence_error)?;
            conn.execute(
                "DELETE FROM context_snapshot WHERE session_id = ?1 AND id NOT IN (
                    SELECT id FROM context_snapshot
                    WHERE session_id = ?1
                    ORDER BY created_at DESC, id DESC
                    LIMIT ?2
                )",
                rusqlite::params![session_id, keep_recent],
            )
            .map_err(persistence_error)
        })
        .await
        .map_err(join_error)?
    }
}

#[derive(Debug)]
struct SnapshotMetaRow {
    id: String,
    session_id: String,
    conversation_id: Option<String>,
    created_at: String,
    token_count: i64,
    hash: String,
}

impl TryFrom<SnapshotMetaRow> for ContextSnapshotMeta {
    type Error = ContextError;

    fn try_from(row: SnapshotMetaRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(&row.id, "id")?,
            session_id: parse_uuid(&row.session_id, "session_id")?,
            conversation_id: row
                .conversation_id
                .map(|value| parse_uuid(&value, "conversation_id"))
                .transpose()?,
            created_at: parse_timestamp(&row.created_at, "created_at")?,
            token_count: u64::try_from(row.token_count).map_err(|_| {
                ContextError::Persistence("snapshot token_count cannot be negative".into())
            })?,
            hash: validate_hash(&row.hash)?,
        })
    }
}

fn migrate_audit_columns(conn: &Connection) -> ContextResult<()> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(context_snapshot)")
        .map_err(persistence_error)?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(persistence_error)?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(persistence_error)?;
    drop(stmt);

    let audit_columns = [
        ("create_time", "TEXT NOT NULL DEFAULT ''"),
        ("update_time", "TEXT NOT NULL DEFAULT ''"),
        ("create_user", "TEXT NOT NULL DEFAULT 'system'"),
        ("update_user", "TEXT NOT NULL DEFAULT 'system'"),
    ];
    for (name, definition) in audit_columns {
        if !columns.contains(name) {
            conn.execute_batch(&format!(
                "ALTER TABLE context_snapshot ADD COLUMN {name} {definition};"
            ))
            .map_err(persistence_error)?;
        }
    }

    conn.execute_batch(
        "UPDATE context_snapshot SET
            create_time = CASE WHEN create_time = '' THEN created_at ELSE create_time END,
            update_time = CASE WHEN update_time = '' THEN created_at ELSE update_time END,
            create_user = CASE WHEN create_user = '' THEN 'system' ELSE create_user END,
            update_user = CASE WHEN update_user = '' THEN 'system' ELSE update_user END;",
    )
    .map_err(persistence_error)?;
    Ok(())
}

fn parse_uuid(value: &str, field: &str) -> ContextResult<Uuid> {
    Uuid::parse_str(value).map_err(|error| {
        ContextError::Persistence(format!("invalid {field} UUID '{value}': {error}"))
    })
}

fn parse_timestamp(value: &str, field: &str) -> ContextResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|error| {
            ContextError::Persistence(format!("invalid {field} timestamp '{value}': {error}"))
        })
}

fn validate_hash(value: &str) -> ContextResult<String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ContextError::Persistence(format!(
            "invalid snapshot SHA-256 hash '{value}'"
        )));
    }
    Ok(value.to_owned())
}

fn persistence_error(error: impl std::fmt::Display) -> ContextError {
    ContextError::Persistence(error.to_string())
}

fn join_error(error: tokio::task::JoinError) -> ContextError {
    ContextError::Internal(format!("Join error: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::DefaultComposer;
    use crate::infrastructure::ContextComposer;

    async fn sample_context(session_id: Uuid) -> Context {
        DefaultComposer::new()
            .compose(session_id, None, Vec::new())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn snapshot_round_trip_list_and_prune() {
        let store = SqliteContextSnapshotStore::new(":memory:").unwrap();
        let session_id = Uuid::new_v4();
        let first = sample_context(session_id).await;
        let mut second = sample_context(session_id).await;
        second.built_at = first.built_at + chrono::Duration::seconds(1);
        store.save_snapshot(&first).await.unwrap();
        store.save_snapshot(&second).await.unwrap();

        let restored = store.load_snapshot(&first.id).await.unwrap().unwrap();
        assert_eq!(restored.id, first.id);
        assert_eq!(restored.hash, first.hash);
        let (items, total) = store.list_snapshots(&session_id, 0, 10).await.unwrap();
        assert_eq!(total, 2);
        assert_eq!(items[0].id, second.id);

        assert_eq!(store.prune_snapshots(&session_id, 1).await.unwrap(), 1);
        assert!(store.load_snapshot(&first.id).await.unwrap().is_none());
    }

    #[test]
    fn all_required_audit_columns_exist() {
        let store = SqliteContextSnapshotStore::new(":memory:").unwrap();
        let conn = store.pool.get().unwrap();
        let mut stmt = conn.prepare("PRAGMA table_info(context_snapshot)").unwrap();
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<HashSet<_>, _>>()
            .unwrap();

        for column in [
            "id",
            "create_time",
            "update_time",
            "create_user",
            "update_user",
        ] {
            assert!(columns.contains(column), "missing {column}");
        }
    }

    #[test]
    fn legacy_schema_migrates_additively() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE context_snapshot (
                id TEXT PRIMARY KEY NOT NULL,
                session_id TEXT NOT NULL,
                conversation_id TEXT,
                created_at TEXT NOT NULL,
                content TEXT NOT NULL,
                token_count INTEGER NOT NULL DEFAULT 0,
                hash TEXT NOT NULL,
                build_duration_ms INTEGER NOT NULL DEFAULT 0
            );",
        )
        .unwrap();
        let created_at = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO context_snapshot (
                id, session_id, created_at, content, token_count, hash, build_duration_ms
             ) VALUES (?1, ?2, ?3, '{}', 0, 'legacy', 0)",
            rusqlite::params![
                Uuid::new_v4().to_string(),
                Uuid::new_v4().to_string(),
                &created_at,
            ],
        )
        .unwrap();
        migrate_audit_columns(&conn).unwrap();

        let mut stmt = conn.prepare("PRAGMA table_info(context_snapshot)").unwrap();
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<HashSet<_>, _>>()
            .unwrap();
        assert!(columns.contains("create_time"));
        assert!(columns.contains("update_user"));
        let migrated_time: String = conn
            .query_row(
                "SELECT create_time FROM context_snapshot LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(migrated_time, created_at);
    }

    #[tokio::test]
    async fn corrupt_snapshot_metadata_returns_error() {
        let store = SqliteContextSnapshotStore::new(":memory:").unwrap();
        let session_id = Uuid::new_v4();
        {
            let conn = store.pool.get().unwrap();
            conn.execute(
                "INSERT INTO context_snapshot (
                    id, session_id, created_at, content, token_count, hash
                 ) VALUES ('broken', ?1, 'not-a-time', '{}', -1, 'hash')",
                rusqlite::params![session_id.to_string()],
            )
            .unwrap();
        }

        assert!(store.list_snapshots(&session_id, 0, 10).await.is_err());
    }

    #[tokio::test]
    async fn stale_semantic_hash_is_rejected() {
        let store = SqliteContextSnapshotStore::new(":memory:").unwrap();
        let mut context = sample_context(Uuid::new_v4()).await;
        context.user.current_input = Some("changed after hashing".into());

        assert!(matches!(
            store.save_snapshot(&context).await.unwrap_err(),
            ContextError::Serialization(_)
        ));
    }
}
