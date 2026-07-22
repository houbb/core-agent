use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use uuid::Uuid;

use crate::domain::{
    validate_actor, Document, DocumentChunk, DocumentStatus, EmbeddingStatus,
};
use crate::error::{DocumentError, DocumentResult};
use crate::infrastructure::DocumentStore;

use super::schema::SCHEMA_SQL;

pub struct SqliteDocumentStore {
    connection: Mutex<Connection>,
}

impl SqliteDocumentStore {
    pub fn new(path: impl AsRef<Path>) -> DocumentResult<Self> {
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

    fn lock(&self) -> DocumentResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| DocumentError::Internal("document database lock poisoned".into()))
    }
}

#[async_trait]
impl DocumentStore for SqliteDocumentStore {
    async fn save_document(&self, document: &Document, actor: &str) -> DocumentResult<()> {
        validate_actor(actor)?;
        document.validate()?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO document (
                id, name, doc_type, source, status, content, chunk_count,
                embedding_status, metadata_json, actor, created_at, updated_at,
                create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, ?12, ?12, ?13, ?13)",
            params![
                document.id.to_string(),
                document.name,
                document.doc_type.as_str(),
                document.source.as_str(),
                document.status.as_str(),
                document.content,
                document.chunk_count,
                document.embedding_status.as_str(),
                serde_json::to_string(&document.metadata)?,
                document.actor,
                document.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn find_document(&self, id: Uuid) -> DocumentResult<Option<Document>> {
        let connection = self.lock()?;
        let raw = connection
            .query_row(
                "SELECT id, name, doc_type, source, status, content, chunk_count,
                        embedding_status, metadata_json, actor, created_at, updated_at
                 FROM document WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, u32>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, String>(11)?,
                    ))
                },
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let metadata = serde_json::from_str(&raw.8).unwrap_or_default();
        Ok(Some(Document {
            id: parse_uuid("document id", &raw.0)?,
            name: raw.1,
            doc_type: DocumentType_parse(&raw.2)?,
            source: DocumentSourceKind_parse(&raw.3)?,
            content: raw.5,
            metadata,
            status: DocumentStatus_parse(&raw.4)?,
            chunk_count: raw.6,
            embedding_status: EmbeddingStatus_parse(&raw.7)?,
            actor: raw.9,
            created_at: parse_time(&raw.10)?,
            updated_at: parse_time(&raw.11)?,
        }))
    }

    async fn list_documents(&self, _namespace: &str) -> DocumentResult<Vec<Document>> {
        let id_strs = {
            let connection = self.lock()?;
            let mut statement = connection.prepare(
                "SELECT id FROM document ORDER BY updated_at DESC, id ASC",
            )?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        let mut docs = Vec::new();
        for id_str in id_strs {
            let id = parse_uuid("document id", &id_str)?;
            if let Some(doc) = self.find_document(id).await? {
                docs.push(doc);
            }
        }
        Ok(docs)
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: DocumentStatus,
        chunk_count: u32,
        embedding_status: EmbeddingStatus,
        actor: &str,
    ) -> DocumentResult<()> {
        validate_actor(actor)?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        let changed = connection.execute(
            "UPDATE document SET status = ?1, chunk_count = ?2, embedding_status = ?3,
             updated_at = ?4, update_time = ?4, update_user = ?5
             WHERE id = ?6",
            params![
                status.as_str(),
                chunk_count,
                embedding_status.as_str(),
                now,
                actor,
                id.to_string(),
            ],
        )?;
        if changed != 1 {
            return Err(DocumentError::NotFound(id.to_string()));
        }
        Ok(())
    }

    async fn delete_document(&self, id: Uuid, actor: &str) -> DocumentResult<()> {
        validate_actor(actor)?;
        let connection = self.lock()?;
        let changed = connection.execute(
            "DELETE FROM document WHERE id = ?1",
            params![id.to_string()],
        )?;
        if changed != 1 {
            return Err(DocumentError::NotFound(id.to_string()));
        }
        Ok(())
    }

    async fn save_chunks(&self, chunks: &[DocumentChunk], actor: &str) -> DocumentResult<()> {
        validate_actor(actor)?;
        if chunks.is_empty() {
            return Ok(());
        }
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let now = Utc::now().to_rfc3339();
        for chunk in chunks {
            chunk.validate()?;
            transaction.execute(
                "INSERT INTO document_chunk (
                    id, document_id, chunk_index, content, metadata_json,
                    token_count, hash, actor, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, ?10, ?10, ?11, ?11)",
                params![
                    chunk.id.to_string(),
                    chunk.document_id.to_string(),
                    chunk.index,
                    chunk.content,
                    serde_json::to_string(&chunk.metadata)?,
                    chunk.token_count,
                    chunk.hash,
                    chunk.actor,
                    chunk.created_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    async fn find_chunks(&self, document_id: Uuid) -> DocumentResult<Vec<DocumentChunk>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, document_id, chunk_index, content, metadata_json,
                    token_count, hash, actor, created_at, updated_at
             FROM document_chunk WHERE document_id = ?1 ORDER BY chunk_index",
        )?;
        let raw_chunks: Vec<(String, String, u32, String, String, u32, String, String, String, String)> = statement
            .query_map(params![document_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u32>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, u32>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(raw_chunks
            .into_iter()
            .map(|(id_str, doc_id_str, idx, content, meta_json, tok_count, hash, act, created, updated)| {
                DocumentChunk {
                    id: parse_uuid("chunk id", &id_str).unwrap_or_default(),
                    document_id: parse_uuid("document id", &doc_id_str).unwrap_or_default(),
                    index: idx,
                    content,
                    metadata: serde_json::from_str(&meta_json).unwrap_or_default(),
                    token_count: tok_count,
                    hash,
                    actor: act,
                    created_at: parse_time(&created).unwrap_or_default(),
                    updated_at: parse_time(&updated).unwrap_or_default(),
                }
            })
            .collect())
    }

    async fn delete_chunks(&self, document_id: Uuid, actor: &str) -> DocumentResult<()> {
        validate_actor(actor)?;
        let connection = self.lock()?;
        connection.execute(
            "DELETE FROM document_chunk WHERE document_id = ?1",
            params![document_id.to_string()],
        )?;
        Ok(())
    }
}

fn parse_uuid(label: &str, value: &str) -> DocumentResult<Uuid> {
    Uuid::parse_str(value)
        .map_err(|_| DocumentError::Validation(format!("{label} is not a valid UUID")))
}

fn parse_time(value: &str) -> DocumentResult<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|t| t.with_timezone(&chrono::Utc))
        .map_err(|_| DocumentError::Validation("invalid timestamp".into()))
}

fn DocumentType_parse(value: &str) -> DocumentResult<DocumentType> {
    crate::domain::DocumentType::parse(value)
        .ok_or_else(|| DocumentError::Validation(format!("unknown document type: {value}")))
}

fn DocumentSourceKind_parse(value: &str) -> DocumentResult<DocumentSourceKind> {
    crate::domain::DocumentSourceKind::parse(value)
        .ok_or_else(|| DocumentError::Validation(format!("unknown document source: {value}")))
}

fn DocumentStatus_parse(value: &str) -> DocumentResult<DocumentStatus> {
    crate::domain::DocumentStatus::parse(value)
        .ok_or_else(|| DocumentError::Validation(format!("unknown document status: {value}")))
}

fn EmbeddingStatus_parse(value: &str) -> DocumentResult<EmbeddingStatus> {
    crate::domain::EmbeddingStatus::parse(value)
        .ok_or_else(|| DocumentError::Validation(format!("unknown embedding status: {value}")))
}

use crate::domain::{DocumentSourceKind, DocumentType};