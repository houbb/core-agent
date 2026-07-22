use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::VectorResult;

pub type VectorMetadata = BTreeMap<String, Value>;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }
            pub fn parse(value: &str) -> Option<Self> {
                match value { $($value => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EmbeddingStatus {
    Pending,
    Embedding,
    Embedded,
    Failed,
}
string_enum!(EmbeddingStatus {
    Pending => "PENDING",
    Embedding => "EMBEDDING",
    Embedded => "EMBEDDED",
    Failed => "FAILED",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub model_name: String,
    pub dimension: usize,
    pub batch_size: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_name: "default".into(),
            dimension: 384,
            batch_size: 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorRecord {
    pub id: Uuid,
    pub content: String,
    pub embedding: Vec<f32>,
    pub metadata: VectorMetadata,
    pub source: String,
    pub document_id: Option<Uuid>,
    pub chunk_id: Option<Uuid>,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl VectorRecord {
    pub fn new(
        content: impl Into<String>,
        embedding: Vec<f32>,
        source: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            content: content.into(),
            embedding,
            metadata: BTreeMap::new(),
            source: source.into(),
            document_id: None,
            chunk_id: None,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> VectorResult<()> {
        if self.content.is_empty() || self.content.len() > 256 * 1024 {
            return Err(crate::error::VectorError::Validation(
                "vector content must be 1..=256 KiB".into(),
            ));
        }
        if self.embedding.is_empty() {
            return Err(crate::error::VectorError::Validation(
                "embedding vector must not be empty".into(),
            ));
        }
        Ok(())
    }

    pub fn dimension(&self) -> usize {
        self.embedding.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorQuery {
    pub text: Option<String>,
    pub embedding: Option<Vec<f32>>,
    pub top_k: usize,
    pub min_score: f64,
    pub metadata_filters: BTreeMap<String, Value>,
    pub keyword_filters: Vec<String>,
    pub source: Option<String>,
}

impl VectorQuery {
    pub fn new(text: Option<String>, embedding: Option<Vec<f32>>) -> Self {
        Self {
            text,
            embedding,
            top_k: 10,
            min_score: 0.0,
            metadata_filters: BTreeMap::new(),
            keyword_filters: Vec::new(),
            source: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub record: VectorRecord,
    pub score: f64,
    pub matched_by: Vec<String>,
}

impl SearchResult {
    pub fn new(record: VectorRecord, score: f64, matched_by: Vec<String>) -> Self {
        Self {
            record,
            score,
            matched_by,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_record_validation_works() {
        let record = VectorRecord::new("content", vec![0.1, 0.2, 0.3], "test", "tester");
        assert!(record.validate().is_ok());
        assert_eq!(record.dimension(), 3);
    }

    #[test]
    fn vector_record_rejects_empty_embedding() {
        let record = VectorRecord::new("content", vec![], "test", "tester");
        assert!(record.validate().is_err());
    }
}