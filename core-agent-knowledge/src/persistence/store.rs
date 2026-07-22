use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::{KnowledgeCategory, KnowledgeItem, KnowledgeStatus};
use crate::error::{KnowledgeError, KnowledgeResult};
use crate::infrastructure::KnowledgeStore;

use super::schema::SCHEMA_SQL;

pub struct SqliteKnowledgeStore {
    connection: Mutex<Connection>,
}

impl SqliteKnowledgeStore {
    pub fn new(path: impl AsRef<Path>) -> KnowledgeResult<Self> {
        let connection = if path.as_ref() == Path::new(":memory:") {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };
        connection.execute_batch(SCHEMA_SQL)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn lock(&self) -> KnowledgeResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| KnowledgeError::Internal("knowledge database lock poisoned".into()))
    }
}

#[async_trait]
impl KnowledgeStore for SqliteKnowledgeStore {
    async fn save_item(&self, item: &KnowledgeItem, actor: &str) -> KnowledgeResult<()> {
        if actor.trim().is_empty() {
            return Err(KnowledgeError::Validation("actor must not be empty".into()));
        }
        item.validate()?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT OR REPLACE INTO knowledge_item (
                id, kind, title, content, source, confidence, owner, tags,
                version, status, document_id, metadata_json, actor, created_at, updated_at,
                create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14, ?15, ?15, ?16, ?16)",
            params![
                item.id.to_string(),
                item.kind.as_str(),
                item.title,
                item.content,
                item.source.as_str(),
                item.confidence,
                item.owner,
                serde_json::to_string(&item.tags)?,
                item.version as i64,
                item.status.as_str(),
                item.document_id.map(|id| id.to_string()),
                serde_json::to_string(&item.metadata)?,
                item.actor,
                item.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find_item(&self, id: Uuid) -> KnowledgeResult<Option<KnowledgeItem>> {
        let connection = self.lock()?;
        let raw = connection
            .query_row(
                "SELECT id, kind, title, content, source, confidence, owner, tags,
                        version, status, document_id, metadata_json, actor, created_at, updated_at
                 FROM knowledge_item WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, f64>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, String>(12)?,
                        row.get::<_, String>(13)?,
                        row.get::<_, String>(14)?,
                    ))
                },
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        Ok(Some(KnowledgeItem {
            id: Uuid::parse_str(&raw.0).map_err(|_| KnowledgeError::Validation("invalid id".into()))?,
            kind: KnowledgeKind::parse(&raw.1)
                .ok_or_else(|| KnowledgeError::Validation(format!("unknown kind: {}", raw.1)))?,
            title: raw.2,
            content: raw.3,
            source: KnowledgeSourceKind::parse(&raw.4)
                .ok_or_else(|| KnowledgeError::Validation(format!("unknown source: {}", raw.4)))?,
            confidence: raw.5,
            owner: raw.6,
            tags: serde_json::from_str(&raw.7).unwrap_or_default(),
            version: raw.8 as u64,
            status: KnowledgeStatus::parse(&raw.9)
                .ok_or_else(|| KnowledgeError::Validation(format!("unknown status: {}", raw.9)))?,
            document_id: raw.10.and_then(|id| Uuid::parse_str(&id).ok()),
            metadata: serde_json::from_str(&raw.11).unwrap_or_default(),
            actor: raw.12,
            created_at: chrono::DateTime::parse_from_rfc3339(&raw.13)
                .map_err(|e| KnowledgeError::Validation(e.to_string()))?
                .with_timezone(&chrono::Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&raw.14)
                .map_err(|e| KnowledgeError::Validation(e.to_string()))?
                .with_timezone(&chrono::Utc),
        }))
    }

    async fn list_items(&self) -> KnowledgeResult<Vec<KnowledgeItem>> {
        let id_strs = {
            let connection = self.lock()?;
            let mut statement = connection.prepare(
                "SELECT id FROM knowledge_item ORDER BY updated_at DESC, id ASC",
            )?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let mut items = Vec::new();
        for id_str in id_strs {
            let id = Uuid::parse_str(&id_str)
                .map_err(|_| KnowledgeError::Validation("invalid id".into()))?;
            if let Some(item) = self.find_item(id).await? {
                items.push(item);
            }
        }
        Ok(items)
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: KnowledgeStatus,
        version: u64,
        actor: &str,
    ) -> KnowledgeResult<()> {
        if actor.trim().is_empty() {
            return Err(KnowledgeError::Validation("actor must not be empty".into()));
        }
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let changed = connection.execute(
            "UPDATE knowledge_item SET status = ?1, version = ?2, updated_at = ?3,
             update_time = ?3, update_user = ?4
             WHERE id = ?5 AND version = ?6",
            params![
                status.as_str(),
                (version + 1) as i64,
                now,
                actor,
                id.to_string(),
                version as i64,
            ],
        )?;
        if changed != 1 {
            return Err(KnowledgeError::Conflict(format!(
                "knowledge item {} version conflict",
                id
            )));
        }
        Ok(())
    }

    async fn delete_item(&self, id: Uuid, actor: &str) -> KnowledgeResult<()> {
        if actor.trim().is_empty() {
            return Err(KnowledgeError::Validation("actor must not be empty".into()));
        }
        let connection = self.lock()?;
        let changed = connection.execute(
            "DELETE FROM knowledge_item WHERE id = ?1",
            params![id.to_string()],
        )?;
        if changed != 1 {
            return Err(KnowledgeError::NotFound(id.to_string()));
        }
        Ok(())
    }

    async fn save_category(&self, category: &KnowledgeCategory, actor: &str) -> KnowledgeResult<()> {
        if actor.trim().is_empty() {
            return Err(KnowledgeError::Validation("actor must not be empty".into()));
        }
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT OR REPLACE INTO knowledge_category (
                id, name, parent_id, description, actor, created_at, updated_at,
                create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?7, ?8, ?8)",
            params![
                category.id.to_string(),
                category.name,
                category.parent_id.map(|id| id.to_string()),
                category.description,
                category.actor,
                category.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn list_categories(&self) -> KnowledgeResult<Vec<KnowledgeCategory>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, name, parent_id, description, actor, created_at, updated_at
             FROM knowledge_category ORDER BY name, id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, name, parent_id, desc, actor, created, updated)| KnowledgeCategory {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                name,
                parent_id: parent_id.and_then(|p| Uuid::parse_str(&p).ok()),
                description: desc,
                actor,
                created_at: chrono::DateTime::parse_from_rfc3339(&created)
                    .map(|t| t.with_timezone(&chrono::Utc))
                    .unwrap_or_default(),
                updated_at: chrono::DateTime::parse_from_rfc3339(&updated)
                    .map(|t| t.with_timezone(&chrono::Utc))
                    .unwrap_or_default(),
            })
            .collect())
    }
}

use crate::domain::KnowledgeSourceKind;
use crate::domain::KnowledgeKind;