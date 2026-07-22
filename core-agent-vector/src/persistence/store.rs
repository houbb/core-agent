use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::domain::{SearchResult, VectorQuery, VectorRecord};
use crate::error::{VectorError, VectorResult};
use crate::infrastructure::{cosine_similarity, VectorStore};

use super::schema::SCHEMA_SQL;

/// Convert Vec<f32> to binary blob (little-endian f32 bytes)
fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    embedding
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

/// Convert binary blob back to Vec<f32>
fn blob_to_embedding(blob: &[u8]) -> VectorResult<Vec<f32>> {
    if blob.len() % 4 != 0 {
        return Err(VectorError::Validation("invalid embedding blob size".into()));
    }
    Ok(blob
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

pub struct SqliteVectorStore {
    connection: Mutex<Connection>,
}

impl SqliteVectorStore {
    pub fn new(path: impl AsRef<Path>) -> VectorResult<Self> {
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

    fn lock(&self) -> VectorResult<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| VectorError::Internal("vector database lock poisoned".into()))
    }
}

#[async_trait]
impl VectorStore for SqliteVectorStore {
    async fn insert(&self, record: &VectorRecord, actor: &str) -> VectorResult<()> {
        record.validate()?;
        let connection = self.lock()?;
        let now = Utc::now().to_rfc3339();
        connection.execute(
            "INSERT INTO vector_record (
                id, content, embedding, dimension, metadata_json, source,
                document_id, chunk_id, actor, created_at, updated_at,
                create_time, update_time, create_user, update_user
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?11, ?12, ?12)",
            params![
                record.id.to_string(),
                record.content,
                embedding_to_blob(&record.embedding),
                record.dimension() as i64,
                serde_json::to_string(&record.metadata)?,
                record.source,
                record.document_id.map(|id| id.to_string()),
                record.chunk_id.map(|id| id.to_string()),
                record.actor,
                record.created_at.to_rfc3339(),
                now,
                actor,
            ],
        )?;
        Ok(())
    }

    async fn batch_insert(&self, records: &[VectorRecord], actor: &str) -> VectorResult<()> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        let now = Utc::now().to_rfc3339();
        for record in records {
            record.validate()?;
            transaction.execute(
                "INSERT INTO vector_record (
                    id, content, embedding, dimension, metadata_json, source,
                    document_id, chunk_id, actor, created_at, updated_at,
                    create_time, update_time, create_user, update_user
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10, ?11, ?11, ?12, ?12)",
                params![
                    record.id.to_string(),
                    record.content,
                    embedding_to_blob(&record.embedding),
                    record.dimension() as i64,
                    serde_json::to_string(&record.metadata)?,
                    record.source,
                    record.document_id.map(|id| id.to_string()),
                    record.chunk_id.map(|id| id.to_string()),
                    record.actor,
                    record.created_at.to_rfc3339(),
                    now,
                    actor,
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    async fn search_by_vector(
        &self,
        embedding: &[f32],
        top_k: usize,
    ) -> VectorResult<Vec<SearchResult>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, content, embedding, dimension, metadata_json, source,
                    document_id, chunk_id, actor, created_at, updated_at
             FROM vector_record ORDER BY created_at DESC",
        )?;
        let mut results: Vec<SearchResult> = statement
            .query_map([], |row| {
                let blob: Vec<u8> = row.get(2)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    blob_to_embedding(&blob).unwrap_or_default(),
                    row.get::<_, i64>(3)? as usize,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .map(|(id_str, content, emb, _dim, meta_json, source, doc_id, chunk_id, actor, created, updated)| {
                let score = cosine_similarity(embedding, &emb);
                let mut record = VectorRecord::new(&content, emb, &source, &actor);
                record.id = Uuid::parse_str(&id_str).unwrap_or_default();
                record.metadata = serde_json::from_str(&meta_json).unwrap_or_default();
                record.document_id = doc_id.and_then(|d| Uuid::parse_str(&d).ok());
                record.chunk_id = chunk_id.and_then(|c| Uuid::parse_str(&c).ok());
                record.created_at = chrono::DateTime::parse_from_rfc3339(&created)
                    .map(|t| t.with_timezone(&chrono::Utc))
                    .unwrap_or_default();
                record.updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                    .map(|t| t.with_timezone(&chrono::Utc))
                    .unwrap_or_default();
                SearchResult::new(record, score, vec!["vector".into()])
            })
            .collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        Ok(results)
    }

    async fn search_by_keyword(
        &self,
        query: &str,
        top_k: usize,
    ) -> VectorResult<Vec<SearchResult>> {
        let connection = self.lock()?;
        let fts_query = query
            .split_whitespace()
            .map(|w| format!("\"{w}\""))
            .collect::<Vec<_>>()
            .join(" AND ");
        if fts_query.is_empty() {
            return Ok(Vec::new());
        }
        // Use FTS5 MATCH via the content sync table
        let mut statement = connection.prepare(
            "SELECT v.id, v.content, v.embedding, v.dimension, v.metadata_json, v.source,
                    v.document_id, v.chunk_id, v.actor, v.created_at, v.updated_at
             FROM vector_record v
             INNER JOIN vector_record_fts fts ON v.rowid = fts.rowid
             WHERE vector_record_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;
        let results: Vec<SearchResult> = statement
            .query_map(params![fts_query, top_k as i64], |row| {
                let blob: Vec<u8> = row.get(2)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    blob_to_embedding(&blob).unwrap_or_default(),
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .map(
                |(id_str, content, emb, meta_json, source, doc_id, chunk_id, actor, created, updated)| {
                    let mut record = VectorRecord::new(&content, emb, &source, &actor);
                    record.id = Uuid::parse_str(&id_str).unwrap_or_default();
                    record.metadata = serde_json::from_str(&meta_json).unwrap_or_default();
                    record.document_id = doc_id.and_then(|d| Uuid::parse_str(&d).ok());
                    record.chunk_id = chunk_id.and_then(|c| Uuid::parse_str(&c).ok());
                    record.created_at = chrono::DateTime::parse_from_rfc3339(&created)
                        .map(|t| t.with_timezone(&chrono::Utc))
                        .unwrap_or_default();
                    record.updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                        .map(|t| t.with_timezone(&chrono::Utc))
                        .unwrap_or_default();
                    SearchResult::new(record, 1.0, vec!["keyword".into()])
                },
            )
            .collect();
        Ok(results)
    }

    async fn hybrid_search(&self, query: &VectorQuery) -> VectorResult<Vec<SearchResult>> {
        if query.embedding.is_none() && query.text.is_none() {
            return Err(VectorError::Validation(
                "query must have text or embedding".into(),
            ));
        }

        let vector_results = if let Some(emb) = &query.embedding {
            self.search_by_vector(emb, query.top_k * 2).await?
        } else {
            Vec::new()
        };

        let keyword_results = if let Some(text) = &query.text {
            self.search_by_keyword(text, query.top_k * 2).await?
        } else {
            Vec::new()
        };

        // Combine with hybrid scoring
        let mut scored: HashMap<Uuid, (VectorRecord, f64, Vec<String>)> = HashMap::new();
        let max_vector = vector_results
            .first()
            .map(|r| r.score)
            .unwrap_or(1.0)
            .max(0.01);
        for r in vector_results {
            let normalized = r.score / max_vector;
            let entry = scored
                .entry(r.record.id)
                .or_insert_with(|| (r.record.clone(), 0.0, Vec::new()));
            entry.1 += 0.7 * normalized;
            entry.2.extend(r.matched_by);
        }

        let max_keyword = keyword_results
            .first()
            .map(|r| r.score)
            .unwrap_or(1.0)
            .max(0.01);
        for r in keyword_results {
            let normalized = r.score / max_keyword;
            let entry = scored
                .entry(r.record.id)
                .or_insert_with(|| (r.record.clone(), 0.0, Vec::new()));
            entry.1 += 0.3 * normalized;
            entry.2.extend(r.matched_by);
        }

        // Apply metadata filters
        let mut results: Vec<SearchResult> = scored
            .into_values()
            .filter(|(r, _, _)| {
                query
                    .metadata_filters
                    .iter()
                    .all(|(k, v)| r.metadata.get(k) == Some(v))
            })
            .map(|(r, score, matched_by)| SearchResult::new(r, score, matched_by))
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(query.top_k);
        Ok(results)
    }

    async fn find_by_id(&self, id: Uuid) -> VectorResult<Option<VectorRecord>> {
        let connection = self.lock()?;
        let raw = connection
            .query_row(
                "SELECT id, content, embedding, dimension, metadata_json, source,
                        document_id, chunk_id, actor, created_at, updated_at
                 FROM vector_record WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    let blob: Vec<u8> = row.get(2)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        blob_to_embedding(&blob).unwrap_or_default(),
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                    ))
                },
            )
            .optional()?;
        let Some(raw) = raw else { return Ok(None) };
        let mut record = VectorRecord::new(&raw.1, raw.2, &raw.4, &raw.7);
        record.id = Uuid::parse_str(&raw.0).map_err(|_| VectorError::Validation("invalid id".into()))?;
        record.metadata = serde_json::from_str(&raw.3).unwrap_or_default();
        record.document_id = raw.5.and_then(|d| Uuid::parse_str(&d).ok());
        record.chunk_id = raw.6.and_then(|c| Uuid::parse_str(&c).ok());
        record.created_at = chrono::DateTime::parse_from_rfc3339(&raw.8)
            .map(|t| t.with_timezone(&chrono::Utc))
            .unwrap_or_default();
        record.updated_at = chrono::DateTime::parse_from_rfc3339(&raw.9)
            .map(|t| t.with_timezone(&chrono::Utc))
            .unwrap_or_default();
        Ok(Some(record))
    }

    async fn delete(&self, id: Uuid, actor: &str) -> VectorResult<()> {
        if actor.trim().is_empty() {
            return Err(VectorError::Validation("actor must not be empty".into()));
        }
        let connection = self.lock()?;
        let changed = connection.execute(
            "DELETE FROM vector_record WHERE id = ?1",
            params![id.to_string()],
        )?;
        if changed != 1 {
            return Err(VectorError::NotFound(id.to_string()));
        }
        Ok(())
    }

    async fn list_by_document(&self, document_id: Uuid) -> VectorResult<Vec<VectorRecord>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare(
            "SELECT id, content, embedding, dimension, metadata_json, source,
                    document_id, chunk_id, actor, created_at, updated_at
             FROM vector_record WHERE document_id = ?1 ORDER BY created_at",
        )?;
        let records = statement
            .query_map(params![document_id.to_string()], |row| {
                let blob: Vec<u8> = row.get(2)?;
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    blob_to_embedding(&blob).unwrap_or_default(),
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                ))
            })?
            .filter_map(|r| r.ok())
            .map(
                |(id_str, content, emb, meta_json, source, doc_id, chunk_id, actor, created, updated)| {
                    let mut record = VectorRecord::new(&content, emb, &source, &actor);
                    record.id = Uuid::parse_str(&id_str).unwrap_or_default();
                    record.metadata = serde_json::from_str(&meta_json).unwrap_or_default();
                    record.document_id = doc_id.and_then(|d| Uuid::parse_str(&d).ok());
                    record.chunk_id = chunk_id.and_then(|c| Uuid::parse_str(&c).ok());
                    record.created_at = chrono::DateTime::parse_from_rfc3339(&created)
                        .map(|t| t.with_timezone(&chrono::Utc))
                        .unwrap_or_default();
                    record.updated_at = chrono::DateTime::parse_from_rfc3339(&updated)
                        .map(|t| t.with_timezone(&chrono::Utc))
                        .unwrap_or_default();
                    record
                },
            )
            .collect();
        Ok(records)
    }
}