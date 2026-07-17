//! SQLite SessionStore 实现

use async_trait::async_trait;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use tokio::task;

use super::schema::SCHEMA_SQL;
use crate::domain::{
    attachment::{Attachment, AttachmentId, AttachmentType},
    conversation::{Conversation, ConversationId, ConversationType},
    manifest::Manifest,
    message::{Message, MessageId, MessageRole, MessageStatus},
    session::{Session, SessionId, SessionState},
    Metadata,
};
use crate::error::{SessionError, SessionResult};
use crate::infrastructure::SessionStore;

/// SQLite 连接池类型
pub type SqlitePool = Pool<SqliteConnectionManager>;

/// SQLite Session 存储实现
pub struct SqliteSessionStore {
    pool: SqlitePool,
}

impl SqliteSessionStore {
    /// 创建新的 SQLite 存储
    ///
    /// `path` 可以是文件路径或 `:memory:`。
    pub fn new(path: &str) -> SessionResult<Self> {
        let manager = SqliteConnectionManager::file(path);
        let pool = Pool::builder()
            .max_size(8)
            .build(manager)
            .map_err(|e| SessionError::Persistence(e.to_string()))?;

        let store = Self { pool };
        store.init_schema()?;
        Ok(store)
    }

    /// 初始化数据库 Schema
    fn init_schema(&self) -> SessionResult<()> {
        let conn = self
            .pool
            .get()
            .map_err(|e| SessionError::Persistence(e.to_string()))?;
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| SessionError::Persistence(e.to_string()))?;
        Ok(())
    }
}

// ── 辅助函数：序列化/反序列化 ──

fn serialize_metadata(meta: &Metadata) -> String {
    serde_json::to_string(meta).unwrap_or_else(|_| "{}".to_string())
}

fn deserialize_metadata(json: &str) -> Metadata {
    serde_json::from_str(json).unwrap_or_default()
}

fn serialize_tags(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
}

fn deserialize_tags(json: &str) -> Vec<String> {
    serde_json::from_str(json).unwrap_or_default()
}

fn parse_session_state(s: &str) -> SessionState {
    match s {
        "CREATED" => SessionState::Created,
        "READY" => SessionState::Ready,
        "RUNNING" => SessionState::Running,
        "PAUSED" => SessionState::Paused,
        "ARCHIVED" => SessionState::Archived,
        "DELETED" => SessionState::Deleted,
        _ => SessionState::Created,
    }
}

fn parse_conversation_type(s: &str) -> ConversationType {
    match s {
        "MAIN" => ConversationType::Main,
        "PLAN" => ConversationType::Plan,
        "REVIEW" => ConversationType::Review,
        "SYSTEM" => ConversationType::System,
        "DEBUG" => ConversationType::Debug,
        _ => ConversationType::Main,
    }
}

fn parse_message_role(s: &str) -> MessageRole {
    match s {
        "SYSTEM" => MessageRole::System,
        "USER" => MessageRole::User,
        "ASSISTANT" => MessageRole::Assistant,
        "TOOL" => MessageRole::Tool,
        "AGENT" => MessageRole::Agent,
        _ => MessageRole::User,
    }
}

fn parse_message_status(s: &str) -> MessageStatus {
    match s {
        "PENDING" => MessageStatus::Pending,
        "STREAMING" => MessageStatus::Streaming,
        "DONE" => MessageStatus::Done,
        "FAILED" => MessageStatus::Failed,
        _ => MessageStatus::Pending,
    }
}

fn parse_attachment_type(s: &str) -> AttachmentType {
    match s {
        "IMAGE" => AttachmentType::Image,
        "FILE" => AttachmentType::File,
        "LOG" => AttachmentType::Log,
        "DIFF" => AttachmentType::Diff,
        "TERMINAL" => AttachmentType::Terminal,
        "PDF" => AttachmentType::Pdf,
        _ => AttachmentType::Other,
    }
}

// ── SessionStore impl ──

#[async_trait]
impl SessionStore for SqliteSessionStore {
    // ── Session ──

    async fn create_session(&self, session: &Session) -> SessionResult<()> {
        let pool = self.pool.clone();
        let s = session.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute(
                "INSERT INTO session (id, title, description, state, created_at, updated_at, last_active_at, owner, workspace_id, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params![
                    s.id.to_string(),
                    s.title,
                    s.description,
                    format!("{:?}", s.state).to_uppercase(),
                    s.created_at.to_rfc3339(),
                    s.updated_at.to_rfc3339(),
                    s.last_active_at.to_rfc3339(),
                    s.owner,
                    s.workspace_id,
                    serialize_metadata(&s.metadata),
                ],
            )
            .map_err(|e| SessionError::Persistence(e.to_string()))?;
            Ok::<_, SessionError>(())
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn get_session(&self, id: &SessionId) -> SessionResult<Option<Session>> {
        let pool = self.pool.clone();
        let id_str = id.to_string();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, description, state, created_at, updated_at, last_active_at,
                            owner, workspace_id, metadata
                     FROM session WHERE id = ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let result = stmt
                .query_row(rusqlite::params![id_str], |row| {
                    Ok(Session {
                        id: SessionId::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                        title: row.get(1)?,
                        description: row.get(2)?,
                        state: parse_session_state(&row.get::<_, String>(3)?),
                        created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_default(),
                        updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_default(),
                        last_active_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_default(),
                        owner: row.get(7)?,
                        workspace_id: row.get(8)?,
                        metadata: deserialize_metadata(&row.get::<_, String>(9)?),
                    })
                });

            match result {
                Ok(session) => Ok(Some(session)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(SessionError::Persistence(e.to_string())),
            }
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn list_sessions(&self, offset: u64, limit: u64) -> SessionResult<(Vec<Session>, u64)> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;

            // 统计总数
            let total: u64 = conn
                .query_row("SELECT COUNT(*) FROM session WHERE state != 'DELETED'", [], |row| {
                    row.get(0)
                })
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            // 分页查询
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, description, state, created_at, updated_at, last_active_at,
                            owner, workspace_id, metadata
                     FROM session WHERE state != 'DELETED'
                     ORDER BY last_active_at DESC
                     LIMIT ?2 OFFSET ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let sessions = stmt
                .query_map(rusqlite::params![offset, limit], |row| {
                    Ok(Session {
                        id: SessionId::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                        title: row.get(1)?,
                        description: row.get(2)?,
                        state: parse_session_state(&row.get::<_, String>(3)?),
                        created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_default(),
                        updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(5)?)
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_default(),
                        last_active_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_default(),
                        owner: row.get(7)?,
                        workspace_id: row.get(8)?,
                        metadata: deserialize_metadata(&row.get::<_, String>(9)?),
                    })
                })
                .map_err(|e| SessionError::Persistence(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>();

            Ok((sessions, total))
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn update_session(&self, session: &Session) -> SessionResult<()> {
        let pool = self.pool.clone();
        let s = session.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute(
                "UPDATE session SET title=?2, description=?3, state=?4, updated_at=?5,
                        last_active_at=?6, owner=?7, workspace_id=?8, metadata=?9
                 WHERE id=?1",
                rusqlite::params![
                    s.id.to_string(),
                    s.title,
                    s.description,
                    format!("{:?}", s.state).to_uppercase(),
                    s.updated_at.to_rfc3339(),
                    s.last_active_at.to_rfc3339(),
                    s.owner,
                    s.workspace_id,
                    serialize_metadata(&s.metadata),
                ],
            )
            .map_err(|e| SessionError::Persistence(e.to_string()))?;
            Ok::<_, SessionError>(())
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn delete_session(&self, id: &SessionId) -> SessionResult<()> {
        let pool = self.pool.clone();
        let id_str = id.to_string();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            // 软删除：标记为 DELETED
            conn.execute(
                "UPDATE session SET state='DELETED', updated_at=?2 WHERE id=?1",
                rusqlite::params![id_str, chrono::Utc::now().to_rfc3339()],
            )
            .map_err(|e| SessionError::Persistence(e.to_string()))?;
            Ok::<_, SessionError>(())
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    // ── Conversation ──

    async fn create_conversation(&self, conversation: &Conversation) -> SessionResult<()> {
        let pool = self.pool.clone();
        let c = conversation.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute(
                "INSERT INTO conversation (id, session_id, conversation_type, name, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    c.id.to_string(),
                    c.session_id.to_string(),
                    c.conversation_type.as_str(),
                    c.name,
                    c.created_at.to_rfc3339(),
                ],
            )
            .map_err(|e| SessionError::Persistence(e.to_string()))?;
            Ok::<_, SessionError>(())
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn get_conversation(&self, id: &ConversationId) -> SessionResult<Option<Conversation>> {
        let pool = self.pool.clone();
        let id_str = id.to_string();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, conversation_type, name, created_at
                     FROM conversation WHERE id = ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let result = stmt.query_row(rusqlite::params![id_str], |row| {
                Ok(Conversation {
                    id: ConversationId::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                    session_id: SessionId::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                    conversation_type: parse_conversation_type(&row.get::<_, String>(2)?),
                    name: row.get(3)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                })
            });

            match result {
                Ok(conv) => Ok(Some(conv)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(SessionError::Persistence(e.to_string())),
            }
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn list_conversations(
        &self,
        session_id: &SessionId,
    ) -> SessionResult<Vec<Conversation>> {
        let pool = self.pool.clone();
        let sid = session_id.to_string();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, conversation_type, name, created_at
                     FROM conversation WHERE session_id = ?1 ORDER BY created_at",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let conversations = stmt
                .query_map(rusqlite::params![sid], |row| {
                    Ok(Conversation {
                        id: ConversationId::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                        session_id: SessionId::parse_str(&row.get::<_, String>(1)?)
                            .unwrap_or_default(),
                        conversation_type: parse_conversation_type(&row.get::<_, String>(2)?),
                        name: row.get(3)?,
                        created_at: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(4)?,
                        )
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                    })
                })
                .map_err(|e| SessionError::Persistence(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>();

            Ok(conversations)
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    // ── Message ──

    async fn append_message(&self, message: &Message) -> SessionResult<()> {
        let pool = self.pool.clone();
        let m = message.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute(
                "INSERT INTO message (id, conversation_id, role, content, status, created_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    m.id.to_string(),
                    m.conversation_id.to_string(),
                    m.role.as_str(),
                    m.content,
                    m.status.as_str(),
                    m.created_at.to_rfc3339(),
                    serialize_metadata(&m.metadata),
                ],
            )
            .map_err(|e| SessionError::Persistence(e.to_string()))?;
            Ok::<_, SessionError>(())
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn get_message(&self, id: &MessageId) -> SessionResult<Option<Message>> {
        let pool = self.pool.clone();
        let id_str = id.to_string();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, conversation_id, role, content, status, created_at, metadata
                     FROM message WHERE id = ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let result = stmt.query_row(rusqlite::params![id_str], |row| {
                Ok(Message {
                    id: MessageId::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                    conversation_id: ConversationId::parse_str(&row.get::<_, String>(1)?)
                        .unwrap_or_default(),
                    role: parse_message_role(&row.get::<_, String>(2)?),
                    content: row.get(3)?,
                    status: parse_message_status(&row.get::<_, String>(4)?),
                    created_at: chrono::DateTime::parse_from_rfc3339(
                        &row.get::<_, String>(5)?,
                    )
                    .map(|d| d.with_timezone(&chrono::Utc))
                    .unwrap_or_default(),
                    metadata: deserialize_metadata(&row.get::<_, String>(6)?),
                })
            });

            match result {
                Ok(msg) => Ok(Some(msg)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(SessionError::Persistence(e.to_string())),
            }
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn update_message(&self, message: &Message) -> SessionResult<()> {
        let pool = self.pool.clone();
        let m = message.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute(
                "UPDATE message SET content=?2, status=?3, metadata=?4 WHERE id=?1",
                rusqlite::params![
                    m.id.to_string(),
                    m.content,
                    m.status.as_str(),
                    serialize_metadata(&m.metadata),
                ],
            )
            .map_err(|e| SessionError::Persistence(e.to_string()))?;
            Ok::<_, SessionError>(())
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn list_messages(
        &self,
        conversation_id: &ConversationId,
        offset: u64,
        limit: u64,
    ) -> SessionResult<(Vec<Message>, u64)> {
        let pool = self.pool.clone();
        let cid = conversation_id.to_string();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;

            let total: u64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM message WHERE conversation_id = ?1",
                    rusqlite::params![cid],
                    |row| row.get(0),
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, conversation_id, role, content, status, created_at, metadata
                     FROM message WHERE conversation_id = ?1
                     ORDER BY created_at
                     LIMIT ?3 OFFSET ?2",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let messages = stmt
                .query_map(rusqlite::params![cid, offset, limit], |row| {
                    Ok(Message {
                        id: MessageId::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                        conversation_id: ConversationId::parse_str(&row.get::<_, String>(1)?)
                            .unwrap_or_default(),
                        role: parse_message_role(&row.get::<_, String>(2)?),
                        content: row.get(3)?,
                        status: parse_message_status(&row.get::<_, String>(4)?),
                        created_at: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(5)?,
                        )
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                        metadata: deserialize_metadata(&row.get::<_, String>(6)?),
                    })
                })
                .map_err(|e| SessionError::Persistence(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>();

            Ok((messages, total))
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn delete_message(&self, id: &MessageId) -> SessionResult<()> {
        let pool = self.pool.clone();
        let id_str = id.to_string();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute("DELETE FROM message WHERE id=?1", rusqlite::params![id_str])
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            Ok::<_, SessionError>(())
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    // ── Manifest ──

    async fn upsert_manifest(&self, manifest: &Manifest) -> SessionResult<()> {
        let pool = self.pool.clone();
        let m = manifest.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute(
                "INSERT INTO manifest (id, session_id, name, model, workspace_path, tags, state,
                        last_active_at, conversation_count, message_count, token_count,
                        last_conversation_id, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
                 ON CONFLICT(session_id) DO UPDATE SET
                     name=excluded.name,
                     model=excluded.model,
                     workspace_path=excluded.workspace_path,
                     tags=excluded.tags,
                     state=excluded.state,
                     last_active_at=excluded.last_active_at,
                     conversation_count=excluded.conversation_count,
                     message_count=excluded.message_count,
                     token_count=excluded.token_count,
                     last_conversation_id=excluded.last_conversation_id,
                     updated_at=excluded.updated_at",
                rusqlite::params![
                    m.id.to_string(),
                    m.session_id.to_string(),
                    m.name,
                    m.model,
                    m.workspace_path,
                    serialize_tags(&m.tags),
                    format!("{:?}", m.state).to_uppercase(),
                    m.last_active_at.to_rfc3339(),
                    m.conversation_count,
                    m.message_count,
                    m.token_count,
                    m.last_conversation_id.map(|id| id.to_string()),
                    m.created_at.to_rfc3339(),
                    m.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|e| SessionError::Persistence(e.to_string()))?;
            Ok::<_, SessionError>(())
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn get_manifest(&self, session_id: &SessionId) -> SessionResult<Option<Manifest>> {
        let pool = self.pool.clone();
        let sid = session_id.to_string();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, name, model, workspace_path, tags, state,
                            last_active_at, conversation_count, message_count, token_count,
                            last_conversation_id, created_at, updated_at
                     FROM manifest WHERE session_id = ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let result = stmt.query_row(rusqlite::params![sid], |row| {
                let last_conv: Option<String> = row.get(11)?;
                Ok(Manifest {
                    id: crate::domain::manifest::ManifestId::parse_str(&row.get::<_, String>(0)?)
                        .unwrap_or_default(),
                    session_id: SessionId::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                    name: row.get(2)?,
                    model: row.get(3)?,
                    workspace_path: row.get(4)?,
                    tags: deserialize_tags(&row.get::<_, String>(5)?),
                    state: parse_session_state(&row.get::<_, String>(6)?),
                    last_active_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(7)?)
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                    conversation_count: row.get(8)?,
                    message_count: row.get(9)?,
                    token_count: row.get(10)?,
                    last_conversation_id: last_conv
                        .and_then(|s| crate::domain::conversation::ConversationId::parse_str(&s).ok()),
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(12)?)
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(13)?)
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                })
            });

            match result {
                Ok(m) => Ok(Some(m)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(SessionError::Persistence(e.to_string())),
            }
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn list_manifests(&self, offset: u64, limit: u64) -> SessionResult<(Vec<Manifest>, u64)> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;

            let total: u64 = conn
                .query_row("SELECT COUNT(*) FROM manifest", [], |row| row.get(0))
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, name, model, workspace_path, tags, state,
                            last_active_at, conversation_count, message_count, token_count,
                            last_conversation_id, created_at, updated_at
                     FROM manifest
                     ORDER BY last_active_at DESC
                     LIMIT ?2 OFFSET ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let manifests = stmt
                .query_map(rusqlite::params![offset, limit], |row| {
                    let last_conv: Option<String> = row.get(11)?;
                    Ok(Manifest {
                        id: crate::domain::manifest::ManifestId::parse_str(&row.get::<_, String>(0)?)
                            .unwrap_or_default(),
                        session_id: SessionId::parse_str(&row.get::<_, String>(1)?)
                            .unwrap_or_default(),
                        name: row.get(2)?,
                        model: row.get(3)?,
                        workspace_path: row.get(4)?,
                        tags: deserialize_tags(&row.get::<_, String>(5)?),
                        state: parse_session_state(&row.get::<_, String>(6)?),
                        last_active_at: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(7)?,
                        )
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                        conversation_count: row.get(8)?,
                        message_count: row.get(9)?,
                        token_count: row.get(10)?,
                        last_conversation_id: last_conv
                            .and_then(|s| crate::domain::conversation::ConversationId::parse_str(&s).ok()),
                        created_at: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(12)?,
                        )
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                        updated_at: chrono::DateTime::parse_from_rfc3339(
                            &row.get::<_, String>(13)?,
                        )
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                    })
                })
                .map_err(|e| SessionError::Persistence(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect::<Vec<_>>();

            Ok((manifests, total))
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    // ── Attachment ──

    async fn create_attachment(&self, attachment: &Attachment) -> SessionResult<()> {
        let pool = self.pool.clone();
        let a = attachment.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute(
                "INSERT INTO attachment (id, message_id, session_id, attachment_type, name,
                        mime_type, size_bytes, storage_path, content, created_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                rusqlite::params![
                    a.id.to_string(),
                    a.message_id.map(|id| id.to_string()),
                    a.session_id.map(|id| id.to_string()),
                    a.attachment_type.as_str(),
                    a.name,
                    a.mime_type,
                    a.size_bytes,
                    a.storage_path,
                    a.content,
                    a.created_at.to_rfc3339(),
                    serialize_metadata(&a.metadata),
                ],
            )
            .map_err(|e| SessionError::Persistence(e.to_string()))?;
            Ok::<_, SessionError>(())
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn get_attachment(&self, id: &AttachmentId) -> SessionResult<Option<Attachment>> {
        let pool = self.pool.clone();
        let id_str = id.to_string();
        task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, message_id, session_id, attachment_type, name,
                            mime_type, size_bytes, storage_path, content, created_at, metadata
                     FROM attachment WHERE id = ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let result = stmt.query_row(rusqlite::params![id_str], |row| {
                let msg_id: Option<String> = row.get(1)?;
                let ses_id: Option<String> = row.get(2)?;
                Ok(Attachment {
                    id: AttachmentId::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                    message_id: msg_id.and_then(|s| MessageId::parse_str(&s).ok()),
                    session_id: ses_id.and_then(|s| SessionId::parse_str(&s).ok()),
                    attachment_type: parse_attachment_type(&row.get::<_, String>(3)?),
                    name: row.get(4)?,
                    mime_type: row.get(5)?,
                    size_bytes: row.get(6)?,
                    storage_path: row.get(7)?,
                    content: row.get(8)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(9)?)
                        .map(|d| d.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                    metadata: deserialize_metadata(&row.get::<_, String>(10)?),
                })
            });

            match result {
                Ok(att) => Ok(Some(att)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(SessionError::Persistence(e.to_string())),
            }
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        attachment::Attachment,
        conversation::Conversation,
        manifest::Manifest,
        message::Message,
        session::Session,
    };

    fn create_test_store() -> SqliteSessionStore {
        SqliteSessionStore::new(":memory:").unwrap()
    }

    #[tokio::test]
    async fn test_create_and_get_session() {
        let store = create_test_store();
        let session = Session::new("Test Session");

        store.create_session(&session).await.unwrap();

        let fetched = store.get_session(&session.id).await.unwrap().unwrap();
        assert_eq!(fetched.title, "Test Session");
        assert_eq!(fetched.state, SessionState::Created);
    }

    #[tokio::test]
    async fn test_list_sessions() {
        let store = create_test_store();

        for i in 1..=3 {
            let s = Session::new(format!("Session {}", i));
            store.create_session(&s).await.unwrap();
        }

        let (sessions, total) = store.list_sessions(0, 10).await.unwrap();
        assert_eq!(total, 3);
        assert_eq!(sessions.len(), 3);
    }

    #[tokio::test]
    async fn test_create_conversation_and_append_message() {
        let store = create_test_store();
        let session = Session::new("Test");
        store.create_session(&session).await.unwrap();

        let conv = Conversation::new_main(session.id);
        store.create_conversation(&conv).await.unwrap();

        let msg = Message::new(conv.id, crate::domain::message::MessageRole::User, "Hello");
        store.append_message(&msg).await.unwrap();

        let (messages, total) = store.list_messages(&conv.id, 0, 10).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(messages[0].content, "Hello");
    }

    #[tokio::test]
    async fn test_manifest_upsert_and_get() {
        let store = create_test_store();
        let session = Session::new("Test");
        store.create_session(&session).await.unwrap();

        let mut manifest = Manifest::from_session(&session);
        manifest.update_stats(1, 5, Some(1000));

        store.upsert_manifest(&manifest).await.unwrap();

        let fetched = store.get_manifest(&session.id).await.unwrap().unwrap();
        assert_eq!(fetched.conversation_count, 1);
        assert_eq!(fetched.message_count, 5);
    }

    #[tokio::test]
    async fn test_attachment_create_and_get() {
        let store = create_test_store();
        let att = Attachment::new(AttachmentType::File, "report.pdf");

        store.create_attachment(&att).await.unwrap();

        let fetched = store.get_attachment(&att.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "report.pdf");
        assert_eq!(fetched.attachment_type, AttachmentType::File);
    }

    #[tokio::test]
    async fn test_session_lifecycle() {
        let store = create_test_store();
        let mut session = Session::new("Lifecycle Test");
        store.create_session(&session).await.unwrap();

        // Created → Ready
        session.transition_to(SessionState::Ready).unwrap();
        store.update_session(&session).await.unwrap();

        let fetched = store.get_session(&session.id).await.unwrap().unwrap();
        assert_eq!(fetched.state, SessionState::Ready);

        // Ready → Running
        session.transition_to(SessionState::Running).unwrap();
        store.update_session(&session).await.unwrap();

        let fetched = store.get_session(&session.id).await.unwrap().unwrap();
        assert_eq!(fetched.state, SessionState::Running);
    }
}