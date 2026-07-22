use thiserror::Error;

#[derive(Debug, Error)]
pub enum RagError {
    #[error("RAG validation failed: {0}")]
    Validation(String),
    #[error("RAG retrieval failed: {0}")]
    RetrievalFailed(String),
    #[error("RAG context too large: {0}")]
    ContextTooLarge(String),
    #[error("RAG vector error: {0}")]
    VectorError(#[from] core_agent_vector::VectorError),
    #[error("RAG document error: {0}")]
    DocumentError(#[from] core_agent_document::DocumentError),
    #[error("RAG internal failure: {0}")]
    Internal(String),
}

impl RagError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::RetrievalFailed(_) => "RETRIEVAL_FAILED",
            Self::ContextTooLarge(_) => "CONTEXT_TOO_LARGE",
            Self::VectorError(_) => "VECTOR_ERROR",
            Self::DocumentError(_) => "DOCUMENT_ERROR",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type RagResult<T> = Result<T, RagError>;