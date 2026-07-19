//! SQLite SessionStore 实现

use async_trait::async_trait;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{types::Type, Connection, Row};
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
            // SQLite 的 :memory: 数据库按连接隔离，单连接可确保测试和并发调用看到同一份数据。
            .max_size(if path == ":memory:" { 1 } else { 8 })
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
        migrate_audit_columns(&conn)?;
        Ok(())
    }
}

/// 为 0.1.0 数据库增量补齐强制审计字段，不删除或重命名旧字段。
fn migrate_audit_columns(conn: &Connection) -> SessionResult<()> {
    for (table, update_source) in [
        ("session", "updated_at"),
        ("conversation", "created_at"),
        ("message", "created_at"),
        ("attachment", "created_at"),
        ("manifest", "updated_at"),
    ] {
        ensure_column(conn, table, "create_time", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(conn, table, "update_time", "TEXT NOT NULL DEFAULT ''")?;
        ensure_column(conn, table, "create_user", "TEXT NOT NULL DEFAULT 'system'")?;
        ensure_column(conn, table, "update_user", "TEXT NOT NULL DEFAULT 'system'")?;

        conn.execute_batch(&format!(
            "UPDATE {table}
             SET create_time = CASE WHEN create_time = '' THEN created_at ELSE create_time END,
                 update_time = CASE WHEN update_time = '' THEN {update_source} ELSE update_time END,
                 create_user = CASE WHEN create_user = '' THEN 'system' ELSE create_user END,
                 update_user = CASE WHEN update_user = '' THEN 'system' ELSE update_user END"
        ))
        .map_err(|error| SessionError::Persistence(error.to_string()))?;
    }
    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> SessionResult<()> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| SessionError::Persistence(error.to_string()))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| SessionError::Persistence(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| SessionError::Persistence(error.to_string()))?;
    drop(statement);

    if !columns.iter().any(|existing| existing == column) {
        conn.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))
        .map_err(|error| SessionError::Persistence(error.to_string()))?;
    }
    Ok(())
}

// ── 辅助函数：序列化/反序列化 ──

fn serialize_metadata(meta: &Metadata) -> String {
    serde_json::to_string(meta).unwrap_or_else(|_| "{}".to_string())
}

fn serialize_tags(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
}

fn data_error(
    index: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(index, Type::Text, Box::new(error))
}

fn invalid_enum(index: usize, kind: &str, value: &str) -> rusqlite::Error {
    data_error(
        index,
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Invalid {kind}: {value}"),
        ),
    )
}

fn parse_uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<uuid::Uuid> {
    let value: String = row.get(index)?;
    uuid::Uuid::parse_str(&value).map_err(|error| data_error(index, error))
}

fn parse_optional_uuid(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<uuid::Uuid>> {
    let value: Option<String> = row.get(index)?;
    value
        .map(|value| uuid::Uuid::parse_str(&value).map_err(|error| data_error(index, error)))
        .transpose()
}

fn parse_datetime(row: &Row<'_>, index: usize) -> rusqlite::Result<chrono::DateTime<chrono::Utc>> {
    let value: String = row.get(index)?;
    chrono::DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&chrono::Utc))
        .map_err(|error| data_error(index, error))
}

fn parse_metadata(row: &Row<'_>, index: usize) -> rusqlite::Result<Metadata> {
    let value: String = row.get(index)?;
    serde_json::from_str(&value).map_err(|error| data_error(index, error))
}

fn parse_tags(row: &Row<'_>, index: usize) -> rusqlite::Result<Vec<String>> {
    let value: String = row.get(index)?;
    serde_json::from_str(&value).map_err(|error| data_error(index, error))
}

fn parse_session_state(value: &str, index: usize) -> rusqlite::Result<SessionState> {
    match value {
        "CREATED" => Ok(SessionState::Created),
        "READY" => Ok(SessionState::Ready),
        "RUNNING" => Ok(SessionState::Running),
        "PAUSED" => Ok(SessionState::Paused),
        "ARCHIVED" => Ok(SessionState::Archived),
        "DELETED" => Ok(SessionState::Deleted),
        _ => Err(invalid_enum(index, "session state", value)),
    }
}

fn parse_conversation_type(value: &str, index: usize) -> rusqlite::Result<ConversationType> {
    match value {
        "MAIN" => Ok(ConversationType::Main),
        "PLAN" => Ok(ConversationType::Plan),
        "REVIEW" => Ok(ConversationType::Review),
        "SYSTEM" => Ok(ConversationType::System),
        "DEBUG" => Ok(ConversationType::Debug),
        _ => Err(invalid_enum(index, "conversation type", value)),
    }
}

fn parse_message_role(value: &str, index: usize) -> rusqlite::Result<MessageRole> {
    match value {
        "SYSTEM" => Ok(MessageRole::System),
        "USER" => Ok(MessageRole::User),
        "ASSISTANT" => Ok(MessageRole::Assistant),
        "TOOL" => Ok(MessageRole::Tool),
        "AGENT" => Ok(MessageRole::Agent),
        _ => Err(invalid_enum(index, "message role", value)),
    }
}

fn parse_message_status(value: &str, index: usize) -> rusqlite::Result<MessageStatus> {
    match value {
        "PENDING" => Ok(MessageStatus::Pending),
        "STREAMING" => Ok(MessageStatus::Streaming),
        "DONE" => Ok(MessageStatus::Done),
        "FAILED" => Ok(MessageStatus::Failed),
        _ => Err(invalid_enum(index, "message status", value)),
    }
}

fn parse_attachment_type(value: &str, index: usize) -> rusqlite::Result<AttachmentType> {
    match value {
        "IMAGE" => Ok(AttachmentType::Image),
        "FILE" => Ok(AttachmentType::File),
        "LOG" => Ok(AttachmentType::Log),
        "DIFF" => Ok(AttachmentType::Diff),
        "TERMINAL" => Ok(AttachmentType::Terminal),
        "PDF" => Ok(AttachmentType::Pdf),
        "OTHER" => Ok(AttachmentType::Other),
        _ => Err(invalid_enum(index, "attachment type", value)),
    }
}

fn map_session(row: &Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        id: parse_uuid(row, 0)?,
        title: row.get(1)?,
        description: row.get(2)?,
        state: parse_session_state(&row.get::<_, String>(3)?, 3)?,
        created_at: parse_datetime(row, 4)?,
        updated_at: parse_datetime(row, 5)?,
        last_active_at: parse_datetime(row, 6)?,
        owner: row.get(7)?,
        workspace_id: row.get(8)?,
        metadata: parse_metadata(row, 9)?,
    })
}

fn map_conversation(row: &Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: parse_uuid(row, 0)?,
        session_id: parse_uuid(row, 1)?,
        conversation_type: parse_conversation_type(&row.get::<_, String>(2)?, 2)?,
        name: row.get(3)?,
        created_at: parse_datetime(row, 4)?,
    })
}

fn map_message(row: &Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        id: parse_uuid(row, 0)?,
        conversation_id: parse_uuid(row, 1)?,
        role: parse_message_role(&row.get::<_, String>(2)?, 2)?,
        content: row.get(3)?,
        status: parse_message_status(&row.get::<_, String>(4)?, 4)?,
        created_at: parse_datetime(row, 5)?,
        metadata: parse_metadata(row, 6)?,
    })
}

fn map_manifest(row: &Row<'_>) -> rusqlite::Result<Manifest> {
    Ok(Manifest {
        id: parse_uuid(row, 0)?,
        session_id: parse_uuid(row, 1)?,
        name: row.get(2)?,
        model: row.get(3)?,
        workspace_path: row.get(4)?,
        tags: parse_tags(row, 5)?,
        state: parse_session_state(&row.get::<_, String>(6)?, 6)?,
        last_active_at: parse_datetime(row, 7)?,
        conversation_count: row.get(8)?,
        message_count: row.get(9)?,
        token_count: row.get(10)?,
        last_conversation_id: parse_optional_uuid(row, 11)?,
        created_at: parse_datetime(row, 12)?,
        updated_at: parse_datetime(row, 13)?,
    })
}

fn map_attachment(row: &Row<'_>) -> rusqlite::Result<Attachment> {
    Ok(Attachment {
        id: parse_uuid(row, 0)?,
        message_id: parse_optional_uuid(row, 1)?,
        session_id: parse_optional_uuid(row, 2)?,
        attachment_type: parse_attachment_type(&row.get::<_, String>(3)?, 3)?,
        name: row.get(4)?,
        mime_type: row.get(5)?,
        size_bytes: row.get(6)?,
        storage_path: row.get(7)?,
        content: row.get(8)?,
        created_at: parse_datetime(row, 9)?,
        metadata: parse_metadata(row, 10)?,
    })
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
            let audit_user = s.owner.clone().unwrap_or_else(|| "system".to_string());
            conn.execute(
                "INSERT INTO session (id, title, description, state, created_at, updated_at, last_active_at, owner, workspace_id, metadata,
                                      create_time, update_time, create_user, update_user)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                rusqlite::params![
                    s.id.to_string(),
                    s.title,
                    s.description,
                    format!("{:?}", s.state).to_uppercase(),
                    s.created_at.to_rfc3339(),
                    s.updated_at.to_rfc3339(),
                    s.last_active_at.to_rfc3339(),
                    s.owner.clone(),
                    s.workspace_id,
                    serialize_metadata(&s.metadata),
                    s.created_at.to_rfc3339(),
                    s.updated_at.to_rfc3339(),
                    audit_user.clone(),
                    audit_user,
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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, title, description, state, created_at, updated_at, last_active_at,
                            owner, workspace_id, metadata
                     FROM session WHERE id = ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let result = stmt.query_row(rusqlite::params![id_str], map_session);

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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            // 统计总数
            let total: u64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM session WHERE state != 'DELETED'",
                    [],
                    |row| row.get(0),
                )
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
                .query_map(rusqlite::params![offset, limit], map_session)
                .map_err(|e| SessionError::Persistence(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            Ok((sessions, total))
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn update_session(&self, session: &Session) -> SessionResult<()> {
        let pool = self.pool.clone();
        let s = session.clone();
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            let audit_user = s.owner.clone().unwrap_or_else(|| "system".to_string());
            conn.execute(
                "UPDATE session SET title=?2, description=?3, state=?4, updated_at=?5,
                        last_active_at=?6, owner=?7, workspace_id=?8, metadata=?9,
                        update_time=?5, update_user=?10
                 WHERE id=?1",
                rusqlite::params![
                    s.id.to_string(),
                    s.title,
                    s.description,
                    format!("{:?}", s.state).to_uppercase(),
                    s.updated_at.to_rfc3339(),
                    s.last_active_at.to_rfc3339(),
                    s.owner.clone(),
                    s.workspace_id,
                    serialize_metadata(&s.metadata),
                    audit_user,
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
                "UPDATE session SET state='DELETED', updated_at=?2, update_time=?2, update_user='system' WHERE id=?1",
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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute(
                "INSERT INTO conversation (id, session_id, conversation_type, name, created_at,
                                           create_time, update_time, create_user, update_user)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?5, 'system', 'system')",
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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, conversation_type, name, created_at
                     FROM conversation WHERE id = ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let result = stmt.query_row(rusqlite::params![id_str], map_conversation);

            match result {
                Ok(conv) => Ok(Some(conv)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(SessionError::Persistence(e.to_string())),
            }
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn list_conversations(&self, session_id: &SessionId) -> SessionResult<Vec<Conversation>> {
        let pool = self.pool.clone();
        let sid = session_id.to_string();
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, conversation_type, name, created_at
                     FROM conversation WHERE session_id = ?1 ORDER BY created_at",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let conversations = stmt
                .query_map(rusqlite::params![sid], map_conversation)
                .map_err(|e| SessionError::Persistence(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

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
                "INSERT INTO message (id, conversation_id, role, content, status, created_at, metadata,
                                      create_time, update_time, create_user, update_user)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?6, ?6, 'system', 'system')",
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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, conversation_id, role, content, status, created_at, metadata
                     FROM message WHERE id = ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let result = stmt.query_row(rusqlite::params![id_str], map_message);

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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute(
                "UPDATE message SET content=?2, status=?3, metadata=?4,
                                    update_time=?5, update_user='system' WHERE id=?1",
                rusqlite::params![
                    m.id.to_string(),
                    m.content,
                    m.status.as_str(),
                    serialize_metadata(&m.metadata),
                    chrono::Utc::now().to_rfc3339(),
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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

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
                     ORDER BY created_at, rowid
                     LIMIT ?3 OFFSET ?2",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let messages = stmt
                .query_map(rusqlite::params![cid, offset, limit], map_message)
                .map_err(|e| SessionError::Persistence(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            Ok((messages, total))
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn delete_message(&self, id: &MessageId) -> SessionResult<()> {
        let pool = self.pool.clone();
        let id_str = id.to_string();
        task::spawn_blocking(move || {
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute(
                "INSERT INTO manifest (id, session_id, name, model, workspace_path, tags, state,
                        last_active_at, conversation_count, message_count, token_count,
                        last_conversation_id, created_at, updated_at,
                        create_time, update_time, create_user, update_user)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                         ?13, ?14, 'system', 'system')
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
                     updated_at=excluded.updated_at,
                     update_time=excluded.update_time,
                     update_user=excluded.update_user",
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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, name, model, workspace_path, tags, state,
                            last_active_at, conversation_count, message_count, token_count,
                            last_conversation_id, created_at, updated_at
                     FROM manifest WHERE session_id = ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let result = stmt.query_row(rusqlite::params![sid], map_manifest);

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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let total: u64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM manifest WHERE state != 'DELETED'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let mut stmt = conn
                .prepare(
                    "SELECT id, session_id, name, model, workspace_path, tags, state,
                            last_active_at, conversation_count, message_count, token_count,
                            last_conversation_id, created_at, updated_at
                     FROM manifest
                     WHERE state != 'DELETED'
                     ORDER BY last_active_at DESC
                     LIMIT ?2 OFFSET ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let manifests = stmt
                .query_map(rusqlite::params![offset, limit], map_manifest)
                .map_err(|e| SessionError::Persistence(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            conn.execute(
                "INSERT INTO attachment (id, message_id, session_id, attachment_type, name,
                        mime_type, size_bytes, storage_path, content, created_at, metadata,
                        create_time, update_time, create_user, update_user)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                         ?10, ?10, 'system', 'system')",
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
            let conn = pool
                .get()
                .map_err(|e| SessionError::Persistence(e.to_string()))?;
            let mut stmt = conn
                .prepare(
                    "SELECT id, message_id, session_id, attachment_type, name,
                            mime_type, size_bytes, storage_path, content, created_at, metadata
                     FROM attachment WHERE id = ?1",
                )
                .map_err(|e| SessionError::Persistence(e.to_string()))?;

            let result = stmt.query_row(rusqlite::params![id_str], map_attachment);

            match result {
                Ok(att) => Ok(Some(att)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(SessionError::Persistence(e.to_string())),
            }
        })
        .await
        .map_err(|e| SessionError::Internal(e.to_string()))?
    }

    async fn create_session_bundle(
        &self,
        session: &Session,
        manifest: &Manifest,
        conversation: &Conversation,
    ) -> SessionResult<()> {
        let pool = self.pool.clone();
        let session = session.clone();
        let manifest = manifest.clone();
        let conversation = conversation.clone();
        task::spawn_blocking(move || {
            let mut conn = pool
                .get()
                .map_err(|error| SessionError::Persistence(error.to_string()))?;
            let transaction = conn
                .transaction()
                .map_err(|error| SessionError::Persistence(error.to_string()))?;
            let audit_user = session
                .owner
                .clone()
                .unwrap_or_else(|| "system".to_string());

            transaction
                .execute(
                    "INSERT INTO session (id, title, description, state, created_at, updated_at,
                                          last_active_at, owner, workspace_id, metadata,
                                          create_time, update_time, create_user, update_user)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    rusqlite::params![
                        session.id.to_string(),
                        session.title,
                        session.description,
                        format!("{:?}", session.state).to_uppercase(),
                        session.created_at.to_rfc3339(),
                        session.updated_at.to_rfc3339(),
                        session.last_active_at.to_rfc3339(),
                        session.owner,
                        session.workspace_id,
                        serialize_metadata(&session.metadata),
                        session.created_at.to_rfc3339(),
                        session.updated_at.to_rfc3339(),
                        audit_user.clone(),
                        audit_user,
                    ],
                )
                .map_err(|error| SessionError::Persistence(error.to_string()))?;

            transaction
                .execute(
                    "INSERT INTO manifest (id, session_id, name, model, workspace_path, tags, state,
                                           last_active_at, conversation_count, message_count, token_count,
                                           last_conversation_id, created_at, updated_at,
                                           create_time, update_time, create_user, update_user)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                             ?13, ?14, 'system', 'system')",
                    rusqlite::params![
                        manifest.id.to_string(),
                        manifest.session_id.to_string(),
                        manifest.name,
                        manifest.model,
                        manifest.workspace_path,
                        serialize_tags(&manifest.tags),
                        format!("{:?}", manifest.state).to_uppercase(),
                        manifest.last_active_at.to_rfc3339(),
                        manifest.conversation_count,
                        manifest.message_count,
                        manifest.token_count,
                        manifest.last_conversation_id.map(|id| id.to_string()),
                        manifest.created_at.to_rfc3339(),
                        manifest.updated_at.to_rfc3339(),
                    ],
                )
                .map_err(|error| SessionError::Persistence(error.to_string()))?;

            transaction
                .execute(
                    "INSERT INTO conversation (id, session_id, conversation_type, name, created_at,
                                               create_time, update_time, create_user, update_user)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?5, 'system', 'system')",
                    rusqlite::params![
                        conversation.id.to_string(),
                        conversation.session_id.to_string(),
                        conversation.conversation_type.as_str(),
                        conversation.name,
                        conversation.created_at.to_rfc3339(),
                    ],
                )
                .map_err(|error| SessionError::Persistence(error.to_string()))?;

            transaction
                .commit()
                .map_err(|error| SessionError::Persistence(error.to_string()))?;
            Ok::<_, SessionError>(())
        })
        .await
        .map_err(|error| SessionError::Internal(error.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        attachment::Attachment, conversation::Conversation, manifest::Manifest, message::Message,
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

    #[test]
    fn test_all_tables_have_required_audit_columns() {
        let store = create_test_store();
        let conn = store.pool.get().unwrap();

        for table in [
            "session",
            "conversation",
            "message",
            "attachment",
            "manifest",
        ] {
            let mut statement = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();

            for required in [
                "id",
                "create_time",
                "update_time",
                "create_user",
                "update_user",
            ] {
                assert!(
                    columns.iter().any(|column| column == required),
                    "{table} is missing {required}"
                );
            }
        }
    }

    #[test]
    fn test_migrate_legacy_schema_adds_audit_columns() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE session (id TEXT, created_at TEXT, updated_at TEXT);
             CREATE TABLE conversation (id TEXT, created_at TEXT);
             CREATE TABLE message (id TEXT, created_at TEXT);
             CREATE TABLE attachment (id TEXT, created_at TEXT);
             CREATE TABLE manifest (id TEXT, created_at TEXT, updated_at TEXT);",
        )
        .unwrap();

        migrate_audit_columns(&conn).unwrap();

        for table in [
            "session",
            "conversation",
            "message",
            "attachment",
            "manifest",
        ] {
            let mut statement = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert!(columns.iter().any(|column| column == "create_time"));
            assert!(columns.iter().any(|column| column == "update_time"));
            assert!(columns.iter().any(|column| column == "create_user"));
            assert!(columns.iter().any(|column| column == "update_user"));
        }
    }

    #[tokio::test]
    async fn test_session_bundle_rolls_back_on_failure() {
        let store = create_test_store();
        let existing = Conversation::new_main(uuid::Uuid::new_v4());
        store.create_conversation(&existing).await.unwrap();

        let session = Session::new("Atomic");
        let manifest = Manifest::from_session(&session);
        let mut duplicate = Conversation::new_main(session.id);
        duplicate.id = existing.id;

        assert!(store
            .create_session_bundle(&session, &manifest, &duplicate)
            .await
            .is_err());
        assert!(store.get_session(&session.id).await.unwrap().is_none());
        assert!(store.get_manifest(&session.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_corrupt_row_returns_error_instead_of_being_dropped() {
        let store = create_test_store();
        {
            let conn = store.pool.get().unwrap();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO session (
                    id, title, state, created_at, updated_at, last_active_at, metadata,
                    create_time, update_time, create_user, update_user
                 ) VALUES ('invalid-uuid', 'Corrupt', 'READY', ?1, ?1, ?1, '{}', ?1, ?1, 'system', 'system')",
                rusqlite::params![now],
            )
            .unwrap();
        }

        assert!(store.list_sessions(0, 10).await.is_err());
    }
}
