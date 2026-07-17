//! SqliteContextSnapshotStore — SQLite 快照存储实现
//!
//! 使用 rusqlite + r2d2 连接池。
//! 遵循与 SqliteSessionStore 相同的模式。

use async_trait::async_trait;
use chrono::Utc;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use uuid::Uuid;

use crate::domain::context::Context;
use crate::error::{ContextError, ContextResult};
use crate::infrastructure::{ContextSnapshotMeta, ContextSnapshotStore};
use crate::persistence::schema::CONTEXT_SNAPSHOT_SCHEMA_SQL;

/// SqliteContextSnapshotStore
///
/// # Example
///
/// ```ignore
/// let store = SqliteContextSnapshotStore::new(":memory:").unwrap();
/// store.save_snapshot(&context).await?;
/// ```
pub struct SqliteContextSnapshotStore {
    pool: Pool<SqliteConnectionManager>,
}

impl SqliteContextSnapshotStore {
    /// 创建新的 SQLite 快照存储
    ///
    /// `path` 可以是文件路径或 ":memory:"。
    pub fn new(path: &str) -> ContextResult<Self> {
        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::builder()
            .max_size(8)
            .build(manager)
            .map_err(|e| ContextError::Persistence(e.to_string()))?;

        // 建表
        {
            let conn = pool
                .get()
                .map_err(|e| ContextError::Persistence(e.to_string()))?;
            conn.execute_batch(CONTEXT_SNAPSHOT_SCHEMA_SQL)
                .map_err(|e| ContextError::Persistence(e.to_string()))?;
        }

        Ok(Self { pool })
    }
}

#[async_trait]
impl ContextSnapshotStore for SqliteContextSnapshotStore {
    async fn save_snapshot(&self, context: &Context) -> ContextResult<()> {
        let id = context.id.to_string();
        let session_id = context.session_id.to_string();
        let conversation_id = context.conversation_id.map(|id| id.to_string());
        let created_at = context.built_at.to_rfc3339();
        let content =
            serde_json::to_string(context).map_err(|e| ContextError::Serialization(e.to_string()))?;
        let token_count = context.total_tokens as i64;
        let hash = context.hash.clone();
        let build_duration_ms = context.build_duration_ms as i64;

        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ContextError::Persistence(e.to_string()))?;
            conn.execute(
                "INSERT OR REPLACE INTO context_snapshot (id, session_id, conversation_id, created_at, content, token_count, hash, build_duration_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                rusqlite::params![id, session_id, conversation_id, created_at, content, token_count, hash, build_duration_ms],
            )
            .map_err(|e| ContextError::Persistence(e.to_string()))?;
            Ok::<_, ContextError>(())
        })
        .await
        .map_err(|e| ContextError::Internal(format!("Join error: {}", e)))?
    }

    async fn load_snapshot(&self, id: &Uuid) -> ContextResult<Option<Context>> {
        let id_str = id.to_string();
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ContextError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare("SELECT content FROM context_snapshot WHERE id = ?1")
                .map_err(|e| ContextError::Persistence(e.to_string()))?;

            let result: Option<String> = stmt
                .query_row(rusqlite::params![id_str], |row| row.get(0))
                .optional()
                .map_err(|e| ContextError::Persistence(e.to_string()))?;

            match result {
                Some(content_str) => {
                    let ctx: Context = serde_json::from_str(&content_str)
                        .map_err(|e| ContextError::Serialization(e.to_string()))?;
                    Ok(Some(ctx))
                }
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| ContextError::Internal(format!("Join error: {}", e)))?
    }

    async fn list_snapshots(
        &self,
        session_id: &Uuid,
        offset: u64,
        limit: u64,
    ) -> ContextResult<(Vec<ContextSnapshotMeta>, u64)> {
        let session_id_str = session_id.to_string();
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ContextError::Persistence(e.to_string()))?;

            // 总数
            let total: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM context_snapshot WHERE session_id = ?1",
                    rusqlite::params![session_id_str],
                    |row| row.get(0),
                )
                .map_err(|e| ContextError::Persistence(e.to_string()))?;

            // 列表
            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, conversation_id, created_at, token_count, hash
                     FROM context_snapshot
                     WHERE session_id = ?1
                     ORDER BY created_at DESC
                     LIMIT ?2 OFFSET ?3",
                )
                .map_err(|e| ContextError::Persistence(e.to_string()))?;

            let items: Vec<ContextSnapshotMeta> = stmt
                .query_map(
                    rusqlite::params![session_id_str, limit as i64, offset as i64],
                    |row| {
                        let id_str: String = row.get(0)?;
                        let sid_str: String = row.get(1)?;
                        let cid_str: Option<String> = row.get(2)?;
                        let created_at_str: String = row.get(3)?;
                        let token_count: i64 = row.get(4)?;
                        let hash: String = row.get(5)?;

                        Ok((
                            id_str,
                            sid_str,
                            cid_str,
                            created_at_str,
                            token_count,
                            hash,
                        ))
                    },
                )
                .map_err(|e| ContextError::Persistence(e.to_string()))?
                .filter_map(|r| r.ok())
                .map(
                    |(id_str, sid_str, cid_str, created_at_str, token_count, hash)| {
                        ContextSnapshotMeta {
                            id: Uuid::parse_str(&id_str).unwrap_or_default(),
                            session_id: Uuid::parse_str(&sid_str).unwrap_or_default(),
                            conversation_id: cid_str
                                .and_then(|s| Uuid::parse_str(&s).ok()),
                            created_at: chrono::DateTime::parse_from_rfc3339(
                                &created_at_str,
                            )
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_default(),
                            token_count: token_count as u64,
                            hash,
                        }
                    },
                )
                .collect();

            Ok::<_, ContextError>((items, total as u64))
        })
        .await
        .map_err(|e| ContextError::Internal(format!("Join error: {}", e)))?
    }

    async fn delete_snapshot(&self, id: &Uuid) -> ContextResult<()> {
        let id_str = id.to_string();
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ContextError::Persistence(e.to_string()))?;
            conn.execute(
                "DELETE FROM context_snapshot WHERE id = ?1",
                rusqlite::params![id_str],
            )
            .map_err(|e| ContextError::Persistence(e.to_string()))?;
            Ok::<_, ContextError>(())
        })
        .await
        .map_err(|e| ContextError::Internal(format!("Join error: {}", e)))?
    }

    async fn prune_snapshots(
        &self,
        session_id: &Uuid,
        keep_recent: usize,
    ) -> ContextResult<usize> {
        let session_id_str = session_id.to_string();
        let pool = self.pool.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| ContextError::Persistence(e.to_string()))?;

            // 删除超过 keep_recent 条的旧快照
            let deleted = conn
                .execute(
                    "DELETE FROM context_snapshot WHERE session_id = ?1 AND id NOT IN (
                        SELECT id FROM context_snapshot
                        WHERE session_id = ?1
                        ORDER BY created_at DESC
                        LIMIT ?2
                    )",
                    rusqlite::params![session_id_str, keep_recent as i64],
                )
                .map_err(|e| ContextError::Persistence(e.to_string()))?;

            Ok::<_, ContextError>(deleted)
        })
        .await
        .map_err(|e| ContextError::Internal(format!("Join error: {}", e)))?
    }
}

/// 辅助 trait：将 rusqlite 的 optional 查询结果转为 Option
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
