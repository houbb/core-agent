use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::domain::{Entity, EntityType, Relation, RelationType, SemanticGraph};
use crate::error::{SemanticError, SemanticResult};
use crate::infrastructure::GraphStore;

use super::schema::SCHEMA_SQL;

pub struct SqliteGraphStore {
    connection: Mutex<Connection>,
}

impl SqliteGraphStore {
    pub fn new(path: impl AsRef<Path>) -> SemanticResult<Self> {
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

    fn lock(&self) -> SemanticResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| SemanticError::Internal("graph database lock poisoned".into()))
    }
}

#[async_trait]
impl GraphStore for SqliteGraphStore {
    async fn save_entity(&self, entity: &Entity, actor: &str) -> SemanticResult<()> {
        if actor.trim().is_empty() {
            return Err(SemanticError::Validation("actor must not be empty".into()));
        }
        entity.validate()?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT OR REPLACE INTO semantic_entity (
                id, name, entity_type, attributes_json, source_document_id, actor,
                created_at, updated_at, create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, ?8, ?9, ?9)",
            params![
                entity.id.to_string(),
                entity.name,
                entity.entity_type.as_str(),
                serde_json::to_string(&entity.attributes)?,
                entity.source_document_id.map(|id| id.to_string()),
                entity.actor,
                entity.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find_entity(&self, id: Uuid) -> SemanticResult<Option<Entity>> {
        let connection = self.lock()?;
        let raw = connection
            .query_row(
                "SELECT id, name, entity_type, attributes_json, source_document_id, actor,
                        created_at, updated_at
                 FROM semantic_entity WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                    ))
                },
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        Ok(Some(Entity {
            id: Uuid::parse_str(&raw.0).map_err(|_| SemanticError::Validation("invalid id".into()))?,
            name: raw.1,
            entity_type: EntityType::parse(&raw.2)
                .ok_or_else(|| SemanticError::Validation(format!("unknown entity type: {}", raw.2)))?,
            attributes: serde_json::from_str(&raw.3).unwrap_or_default(),
            source_document_id: raw.4.and_then(|id| Uuid::parse_str(&id).ok()),
            actor: raw.5,
            created_at: chrono::DateTime::parse_from_rfc3339(&raw.6)
                .map_err(|e| SemanticError::Validation(e.to_string()))?
                .with_timezone(&chrono::Utc),
            updated_at: chrono::DateTime::parse_from_rfc3339(&raw.7)
                .map_err(|e| SemanticError::Validation(e.to_string()))?
                .with_timezone(&chrono::Utc),
        }))
    }

    async fn search_entities(&self, name_query: &str) -> SemanticResult<Vec<Entity>> {
        let connection = self.lock()?;
        let needle = format!("%{}%", name_query);
        let mut statement = connection.prepare(
            "SELECT id, name, entity_type, attributes_json, source_document_id, actor,
                    created_at, updated_at
             FROM semantic_entity WHERE name LIKE ?1 ORDER BY name, id",
        )?;
        let rows = statement
            .query_map(params![needle], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, name, etype, attrs, src_doc, actor, created, updated)| Entity {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                name,
                entity_type: EntityType::parse(&etype).unwrap_or(EntityType::Other),
                attributes: serde_json::from_str(&attrs).unwrap_or_default(),
                source_document_id: src_doc.and_then(|d| Uuid::parse_str(&d).ok()),
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

    async fn list_entities(&self) -> SemanticResult<Vec<Entity>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, name, entity_type, attributes_json, source_document_id, actor,
                    created_at, updated_at
             FROM semantic_entity ORDER BY name, id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(id, name, etype, attrs, src_doc, actor, created, updated)| Entity {
                id: Uuid::parse_str(&id).unwrap_or_default(),
                name,
                entity_type: EntityType::parse(&etype).unwrap_or(EntityType::Other),
                attributes: serde_json::from_str(&attrs).unwrap_or_default(),
                source_document_id: src_doc.and_then(|d| Uuid::parse_str(&d).ok()),
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

    async fn save_relation(&self, relation: &Relation, actor: &str) -> SemanticResult<()> {
        if actor.trim().is_empty() {
            return Err(SemanticError::Validation("actor must not be empty".into()));
        }
        relation.validate()?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT OR REPLACE INTO semantic_relation (
                id, source_entity_id, target_entity_id, relation_type, confidence,
                attributes_json, source_document_id, actor, created_at, updated_at,
                create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?10, ?11, ?11)",
            params![
                relation.id.to_string(),
                relation.source_entity_id.to_string(),
                relation.target_entity_id.to_string(),
                relation.relation_type.as_str(),
                relation.confidence,
                serde_json::to_string(&relation.attributes)?,
                relation.source_document_id.map(|id| id.to_string()),
                relation.actor,
                relation.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find_relations(&self, entity_id: Uuid) -> SemanticResult<Vec<Relation>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, source_entity_id, target_entity_id, relation_type, confidence,
                    attributes_json, source_document_id, actor, created_at, updated_at
             FROM semantic_relation
             WHERE source_entity_id = ?1 OR target_entity_id = ?1
             ORDER BY created_at",
        )?;
        let rows = statement
            .query_map(params![entity_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(
                |(id, src, tgt, rtype, conf, attrs, src_doc, actor, created, updated)| Relation {
                    id: Uuid::parse_str(&id).unwrap_or_default(),
                    source_entity_id: Uuid::parse_str(&src).unwrap_or_default(),
                    target_entity_id: Uuid::parse_str(&tgt).unwrap_or_default(),
                    relation_type: RelationType::parse(&rtype).unwrap_or(RelationType::Other),
                    confidence: conf,
                    attributes: serde_json::from_str(&attrs).unwrap_or_default(),
                    source_document_id: src_doc.and_then(|d| Uuid::parse_str(&d).ok()),
                    actor,
                    created_at: chrono::DateTime::parse_from_rfc3339(&created)
                        .map(|t| t.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&updated)
                        .map(|t| t.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                },
            )
            .collect())
    }

    async fn list_relations(&self) -> SemanticResult<Vec<Relation>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, source_entity_id, target_entity_id, relation_type, confidence,
                    attributes_json, source_document_id, actor, created_at, updated_at
             FROM semantic_relation ORDER BY created_at",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, f64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(
                |(id, src, tgt, rtype, conf, attrs, src_doc, actor, created, updated)| Relation {
                    id: Uuid::parse_str(&id).unwrap_or_default(),
                    source_entity_id: Uuid::parse_str(&src).unwrap_or_default(),
                    target_entity_id: Uuid::parse_str(&tgt).unwrap_or_default(),
                    relation_type: RelationType::parse(&rtype).unwrap_or(RelationType::Other),
                    confidence: conf,
                    attributes: serde_json::from_str(&attrs).unwrap_or_default(),
                    source_document_id: src_doc.and_then(|d| Uuid::parse_str(&d).ok()),
                    actor,
                    created_at: chrono::DateTime::parse_from_rfc3339(&created)
                        .map(|t| t.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                    updated_at: chrono::DateTime::parse_from_rfc3339(&updated)
                        .map(|t| t.with_timezone(&chrono::Utc))
                        .unwrap_or_default(),
                },
            )
            .collect())
    }

    async fn load_graph(&self) -> SemanticResult<SemanticGraph> {
        let entities = self.list_entities().await?;
        let relations = self.list_relations().await?;
        let entity_map = entities.into_iter().map(|e| (e.id, e)).collect();
        Ok(SemanticGraph {
            entities: entity_map,
            relations,
        })
    }
}

use rusqlite::OptionalExtension;