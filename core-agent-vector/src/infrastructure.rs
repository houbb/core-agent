use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{SearchResult, VectorQuery, VectorRecord};
use crate::error::VectorResult;

// ── EmbeddingModel ──

pub trait EmbeddingModel: Send + Sync {
    fn embed(&self, text: &str) -> VectorResult<Vec<f32>>;
    fn embed_batch(&self, texts: &[&str]) -> VectorResult<Vec<Vec<f32>>>;
}

// ── VectorStore ──

#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn insert(&self, record: &VectorRecord, actor: &str) -> VectorResult<()>;
    async fn batch_insert(&self, records: &[VectorRecord], actor: &str) -> VectorResult<()>;
    async fn search_by_vector(
        &self,
        embedding: &[f32],
        top_k: usize,
    ) -> VectorResult<Vec<SearchResult>>;
    async fn search_by_keyword(
        &self,
        query: &str,
        top_k: usize,
    ) -> VectorResult<Vec<SearchResult>>;
    async fn hybrid_search(&self, query: &VectorQuery) -> VectorResult<Vec<SearchResult>>;
    async fn find_by_id(&self, id: Uuid) -> VectorResult<Option<VectorRecord>>;
    async fn delete(&self, id: Uuid, actor: &str) -> VectorResult<()>;
    async fn list_by_document(&self, document_id: Uuid) -> VectorResult<Vec<VectorRecord>>;
}

pub type SharedVectorStore = Arc<dyn VectorStore>;

// ── Cosine similarity helper ──

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    (dot / (norm_a * norm_b)) as f64
}