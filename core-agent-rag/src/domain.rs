use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::RagResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagQuery {
    pub question: String,
    pub namespace: String,
    pub top_k: usize,
    pub min_score: f64,
    pub max_context_tokens: usize,
}

impl RagQuery {
    pub fn new(question: impl Into<String>) -> Self {
        Self {
            question: question.into(),
            namespace: "default".into(),
            top_k: 5,
            min_score: 0.0,
            max_context_tokens: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalResult {
    pub content: String,
    pub score: f64,
    pub source: String,
    pub document_id: Option<Uuid>,
    pub chunk_id: Option<Uuid>,
    pub metadata: serde_json::Value,
    pub matched_by: Vec<String>,
}

impl RetrievalResult {
    pub fn new(
        content: impl Into<String>,
        score: f64,
        source: impl Into<String>,
    ) -> Self {
        Self {
            content: content.into(),
            score,
            source: source.into(),
            document_id: None,
            chunk_id: None,
            metadata: serde_json::Value::Null,
            matched_by: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagContext {
    pub results: Vec<RetrievalResult>,
    pub context_text: String,
    pub total_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagAnswer {
    pub answer: String,
    pub sources: Vec<RetrievalResult>,
    pub confidence: f64,
    pub processing_time_ms: u64,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagConfig {
    pub max_results: usize,
    pub min_score: f64,
    pub vector_weight: f64,
    pub keyword_weight: f64,
    pub max_context_tokens: usize,
    pub include_sources: bool,
}

impl Default for RagConfig {
    fn default() -> Self {
        Self {
            max_results: 5,
            min_score: 0.0,
            vector_weight: 0.7,
            keyword_weight: 0.3,
            max_context_tokens: 4096,
            include_sources: true,
        }
    }
}