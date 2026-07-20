//! SqliteContextReferenceStore — SQLite 引用存储实现。
//!
//! 遵循与 SqliteContextSnapshotStore 相同的模式：r2d2 连接池 + tokio::task::spawn_blocking。

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::OptionalExtension;
use uuid::Uuid;

use crate::domain::context_reference::{ContextReference, ReferenceLocator, ReferenceType};
use crate::error::{ContextError, ContextResult};
use crate::persistence::schema::CONTEXT_REFERENCE_SCHEMA_SQL;

/// SQLite 上下文引用存储。
pub struct SqliteContextReferenceStore {
    pool: Pool<SqliteConnectionManager>,
}

impl SqliteContextReferenceStore {
    /// 创建 Store；`:memory:` 使用单连接。
    pub fn new(path: &str) -> ContextResult<Self> {
        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::builder()
            .max_size(if path == ":memory:" { 1 } else { 8 })
            .build(manager)
            .map_err(persistence_error)?;

        {
            let conn = pool.get().map_err(persistence_error)?;
            conn.execute_batch(CONTEXT_REFERENCE_SCHEMA_SQL)
                .map_err(persistence_error)?;
        }

        Ok(Self { pool })
    }

    /// 保存引用
    pub async fn save_reference(&self, reference: &ContextReference) -> ContextResult<()> {
        let id = reference.id.to_string();
        let session_id = reference.metadata.get("session_id").cloned().unwrap_or_default();
        let reference_type = reference.reference_type.as_str();
        let locator = serde_json::to_string(&reference.locator)
            .map_err(|e| ContextError::Serialization(e.to_string()))?;
        let snapshot = reference.snapshot.clone().unwrap_or_default();
        let metadata = serde_json::to_string(&reference.metadata)
            .map_err(|e| ContextError::Serialization(e.to_string()))?;
        let created_at = reference.created_at.to_rfc3339();
        let now = Utc::now().to_rfc3339();
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(persistence_error)?;
            conn.execute(
                "INSERT INTO context_reference (
                    id, session_id, reference_type, locator, snapshot, metadata, created_at,
                    create_time, update_time, create_user, update_user
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 'system', 'system')
                ON CONFLICT(id) DO UPDATE SET
                    session_id = excluded.session_id,
                    reference_type = excluded.reference_type,
                    locator = excluded.locator,
                    snapshot = excluded.snapshot,
                    metadata = excluded.metadata,
                    created_at = excluded.created_at,
                    update_time = excluded.update_time,
                    update_user = excluded.update_user",
                rusqlite::params![id, session_id, reference_type, locator, snapshot, metadata, created_at, now],
            )
            .map_err(persistence_error)?;
            Ok::<_, ContextError>(())
        })
        .await
        .map_err(join_error)?
    }

    /// 加载引用
    pub async fn load_reference(&self, id: &str) -> ContextResult<Option<ContextReference>> {
        let id = id.to_string();
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(persistence_error)?;
            let row: Option<ReferenceRow> = conn
                .query_row(
                    "SELECT reference_type, locator, snapshot, metadata, created_at FROM context_reference WHERE id = ?1",
                    rusqlite::params![id],
                    |row| {
                        Ok(ReferenceRow {
                            reference_type: row.get(0)?,
                            locator: row.get(1)?,
                            snapshot: row.get(2)?,
                            metadata: row.get(3)?,
                            created_at: row.get(4)?,
                        })
                    },
                )
                .optional()
                .map_err(persistence_error)?;

            let Some(row) = row else {
                return Ok(None);
            };

            let uid = Uuid::parse_str(&id)
                .map_err(|e| ContextError::Persistence(format!("invalid reference id: {e}")))?;
            let reference_type = parse_reference_type(&row.reference_type)?;
            let locator: ReferenceLocator = serde_json::from_str(&row.locator)
                .map_err(|e| ContextError::Serialization(e.to_string()))?;
            let metadata: HashMap<String, String> = serde_json::from_str(&row.metadata)
                .map_err(|e| ContextError::Serialization(e.to_string()))?;
            let snapshot = if row.snapshot.is_empty() { None } else { Some(row.snapshot) };
            let created_at = DateTime::parse_from_rfc3339(&row.created_at)
                .map_err(|e| ContextError::Persistence(format!("invalid timestamp: {e}")))?
                .with_timezone(&Utc);

            Ok(Some(ContextReference {
                id: uid,
                reference_type,
                locator,
                snapshot,
                metadata,
                created_at,
            }))
        })
        .await
        .map_err(join_error)?
    }

    /// 列出某 Session 的所有引用
    pub async fn list_references(
        &self,
        session_id: &str,
        offset: u64,
        limit: u64,
    ) -> ContextResult<(Vec<ContextReference>, u64)> {
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
                    "SELECT COUNT(*) FROM context_reference WHERE session_id = ?1",
                    rusqlite::params![&session_id],
                    |row| row.get(0),
                )
                .map_err(persistence_error)?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, reference_type, locator, snapshot, metadata, created_at
                     FROM context_reference
                     WHERE session_id = ?1
                     ORDER BY created_at DESC, id DESC
                     LIMIT ?2 OFFSET ?3",
                )
                .map_err(persistence_error)?;
            let _rows = stmt
                .query_map(rusqlite::params![&session_id, limit, offset], |row| {
                    Ok(ReferenceRow {
                        reference_type: row.get(0)?,
                        locator: row.get(1)?,
                        snapshot: row.get(2)?,
                        metadata: row.get(3)?,
                        created_at: row.get(4)?,
                    })
                })
                .map_err(persistence_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(persistence_error)?;

            // 需要重新查询 id 以构建完整的 ContextReference
            let mut stmt2 = conn
                .prepare(
                    "SELECT id, reference_type, locator, snapshot, metadata, created_at
                     FROM context_reference
                     WHERE session_id = ?1
                     ORDER BY created_at DESC, id DESC
                     LIMIT ?2 OFFSET ?3",
                )
                .map_err(persistence_error)?;
            let items: Vec<ContextReference> = stmt2
                .query_map(rusqlite::params![&session_id, limit, offset], |row| {
                    let id_str: String = row.get(0)?;
                    let uid = Uuid::parse_str(&id_str)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    let reference_type_str: String = row.get(1)?;
                    let locator_str: String = row.get(2)?;
                    let snapshot_str: String = row.get(3)?;
                    let metadata_str: String = row.get(4)?;
                    let created_at_str: String = row.get(5)?;

                    let reference_type = parse_reference_type(&reference_type_str)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    let locator: ReferenceLocator = serde_json::from_str(&locator_str)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    let metadata: HashMap<String, String> = serde_json::from_str(&metadata_str)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                    let snapshot = if snapshot_str.is_empty() { None } else { Some(snapshot_str) };
                    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                        .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?
                        .with_timezone(&Utc);

                    Ok(ContextReference {
                        id: uid,
                        reference_type,
                        locator,
                        snapshot,
                        metadata,
                        created_at,
                    })
                })
                .map_err(persistence_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(persistence_error)?;

            let total = u64::try_from(total)
                .map_err(|_| ContextError::Persistence("reference count cannot be negative".into()))?;
            Ok::<_, ContextError>((items, total))
        })
        .await
        .map_err(join_error)?
    }

    /// 删除引用
    pub async fn delete_reference(&self, id: &str) -> ContextResult<()> {
        let id = id.to_string();
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(persistence_error)?;
            conn.execute(
                "DELETE FROM context_reference WHERE id = ?1",
                rusqlite::params![id],
            )
            .map_err(persistence_error)?;
            Ok::<_, ContextError>(())
        })
        .await
        .map_err(join_error)?
    }

    /// 清理某 Session 的所有引用
    pub async fn clear_references(&self, session_id: &str) -> ContextResult<usize> {
        let session_id = session_id.to_string();
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(persistence_error)?;
            let count = conn
                .execute(
                    "DELETE FROM context_reference WHERE session_id = ?1",
                    rusqlite::params![session_id],
                )
                .map_err(persistence_error)?;
            Ok::<_, ContextError>(count)
        })
        .await
        .map_err(join_error)?
    }
}

#[derive(Debug)]
struct ReferenceRow {
    reference_type: String,
    locator: String,
    snapshot: String,
    metadata: String,
    created_at: String,
}

fn parse_reference_type(s: &str) -> ContextResult<ReferenceType> {
    match s {
        "FILE" => Ok(ReferenceType::File),
        "SELECTION" => Ok(ReferenceType::Selection),
        "MESSAGE" => Ok(ReferenceType::Message),
        _ => Err(ContextError::Persistence(format!("unknown reference type: {s}"))),
    }
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
    use uuid::Uuid;

    #[tokio::test]
    async fn test_reference_crud() {
        let store = SqliteContextReferenceStore::new(":memory:").unwrap();
        let session_id = Uuid::new_v4().to_string();

        let ref1 = ContextReference::file("src/main.rs")
            .with_line_range(10, 30)
            .with_meta("session_id", &session_id);

        store.save_reference(&ref1).await.unwrap();

        let loaded = store.load_reference(&ref1.id.to_string()).await.unwrap().unwrap();
        assert_eq!(loaded.reference_type, ReferenceType::File);
        if let ReferenceLocator::File { path, start_line, end_line, .. } = &loaded.locator {
            assert_eq!(path, "src/main.rs");
            assert_eq!(*start_line, Some(10));
            assert_eq!(*end_line, Some(30));
        }

        let (items, total) = store.list_references(&session_id, 0, 10).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(items.len(), 1);

        store.delete_reference(&ref1.id.to_string()).await.unwrap();
        assert!(store.load_reference(&ref1.id.to_string()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_clear_references() {
        let store = SqliteContextReferenceStore::new(":memory:").unwrap();
        let session_id = Uuid::new_v4().to_string();

        let r1 = ContextReference::file("a.rs").with_meta("session_id", &session_id);
        let r2 = ContextReference::file("b.rs").with_meta("session_id", &session_id);
        store.save_reference(&r1).await.unwrap();
        store.save_reference(&r2).await.unwrap();

        let count = store.clear_references(&session_id).await.unwrap();
        assert_eq!(count, 2);

        let (items, total) = store.list_references(&session_id, 0, 10).await.unwrap();
        assert_eq!(total, 0);
        assert!(items.is_empty());
    }
}