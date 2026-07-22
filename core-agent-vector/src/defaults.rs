use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::domain::{SearchResult, VectorQuery, VectorRecord};
use crate::error::{VectorError, VectorResult};
use crate::infrastructure::{cosine_similarity, EmbeddingModel, VectorStore};

// ── SimpleEmbeddingModel (MVP) ──

pub struct SimpleEmbeddingModel;

impl EmbeddingModel for SimpleEmbeddingModel {
    fn embed(&self, text: &str) -> VectorResult<Vec<f32>> {
        // MVP: simple bag-of-characters embedding for testing
        // In production, this would call an embedding API
        let mut vec = vec![0.0f32; 384];
        for (i, byte) in text.bytes().enumerate() {
            vec[i % 384] += byte as f32 / 255.0;
        }
        // Normalize
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }
        Ok(vec)
    }

    fn embed_batch(&self, texts: &[&str]) -> VectorResult<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}

// ── InMemoryVectorStore ──

#[derive(Clone, Default)]
struct InMemoryState {
    records: HashMap<Uuid, VectorRecord>,
    document_index: HashMap<Uuid, Vec<Uuid>>, // document_id -> vec of record ids
}

#[derive(Default)]
pub struct InMemoryVectorStore {
    state: RwLock<InMemoryState>,
}

impl InMemoryVectorStore {
    fn read(&self) -> VectorResult<std::sync::RwLockReadGuard<'_, InMemoryState>> {
        self.state
            .read()
            .map_err(|_| VectorError::Internal("store lock poisoned".into()))
    }

    fn write(&self) -> VectorResult<std::sync::RwLockWriteGuard<'_, InMemoryState>> {
        self.state
            .write()
            .map_err(|_| VectorError::Internal("store lock poisoned".into()))
    }
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn insert(&self, record: &VectorRecord, actor: &str) -> VectorResult<()> {
        if actor.trim().is_empty() {
            return Err(VectorError::Validation("actor must not be empty".into()));
        }
        record.validate()?;
        let mut state = self.write()?;
        state.records.insert(record.id, record.clone());
        if let Some(doc_id) = record.document_id {
            state
                .document_index
                .entry(doc_id)
                .or_default()
                .push(record.id);
        }
        Ok(())
    }

    async fn batch_insert(&self, records: &[VectorRecord], actor: &str) -> VectorResult<()> {
        for record in records {
            self.insert(record, actor).await?;
        }
        Ok(())
    }

    async fn search_by_vector(
        &self,
        embedding: &[f32],
        top_k: usize,
    ) -> VectorResult<Vec<SearchResult>> {
        let state = self.read()?;
        let mut results: Vec<SearchResult> = state
            .records
            .values()
            .map(|r| {
                let score = cosine_similarity(embedding, &r.embedding);
                SearchResult::new(r.clone(), score, vec!["vector".into()])
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
        let state = self.read()?;
        let needle = query.to_lowercase();
        let mut results: Vec<SearchResult> = state
            .records
            .values()
            .filter(|r| {
                r.content.to_lowercase().contains(&needle)
                    || r.source.to_lowercase().contains(&needle)
            })
            .map(|r| {
                SearchResult::new(r.clone(), 1.0, vec!["keyword".into()])
            })
            .collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        Ok(results)
    }

    async fn hybrid_search(&self, query: &VectorQuery) -> VectorResult<Vec<SearchResult>> {
        if query.embedding.is_none() && query.text.is_none() {
            return Err(VectorError::Validation(
                "query must have text or embedding".into(),
            ));
        }

        // Vector search
        let vector_results = if let Some(emb) = &query.embedding {
            self.search_by_vector(emb, query.top_k * 2).await?
        } else {
            Vec::new()
        };

        // Keyword search
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
        Ok(self.read()?.records.get(&id).cloned())
    }

    async fn delete(&self, id: Uuid, actor: &str) -> VectorResult<()> {
        if actor.trim().is_empty() {
            return Err(VectorError::Validation("actor must not be empty".into()));
        }
        let mut state = self.write()?;
        state.records.remove(&id);
        state.document_index.retain(|_, ids| {
            ids.retain(|i| *i != id);
            !ids.is_empty()
        });
        Ok(())
    }

    async fn list_by_document(&self, document_id: Uuid) -> VectorResult<Vec<VectorRecord>> {
        let state = self.read()?;
        Ok(state
            .document_index
            .get(&document_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| state.records.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default())
    }
}