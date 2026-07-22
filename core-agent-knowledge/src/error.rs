use thiserror::Error;

#[derive(Debug, Error)]
pub enum KnowledgeError {
    #[error("knowledge validation failed: {0}")]
    Validation(String),
    #[error("knowledge not found: {0}")]
    NotFound(String),
    #[error("knowledge conflict: {0}")]
    Conflict(String),
    #[error("knowledge database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("knowledge serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("knowledge document error: {0}")]
    DocumentError(#[from] core_agent_document::DocumentError),
    #[error("knowledge vector error: {0}")]
    VectorError(#[from] core_agent_vector::VectorError),
    #[error("knowledge RAG error: {0}")]
    RagError(#[from] core_agent_rag::RagError),
    #[error("knowledge internal failure: {0}")]
    Internal(String),
}

impl KnowledgeError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::Database(_) => "DATABASE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::DocumentError(_) => "DOCUMENT_ERROR",
            Self::VectorError(_) => "VECTOR_ERROR",
            Self::RagError(_) => "RAG_ERROR",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type KnowledgeResult<T> = Result<T, KnowledgeError>;