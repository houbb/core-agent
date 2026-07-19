use thiserror::Error;

#[derive(Debug, Error)]
pub enum WorkflowError {
    #[error("workflow validation failed: {0}")]
    Validation(String),
    #[error("workflow entity not found: {0}")]
    NotFound(String),
    #[error("workflow version conflict: {0}")]
    Conflict(String),
    #[error("invalid workflow lifecycle transition: {0}")]
    InvalidState(String),
    #[error("workflow policy denied operation: {0}")]
    PolicyDenied(String),
    #[error("workflow outcome is unknown: {0}")]
    OutcomeUnknown(String),
    #[error("workflow engine failed: {0}")]
    Engine(String),
    #[error("workflow extension failed: {0}")]
    Extension(String),
    #[error("workflow database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("workflow serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("workflow internal failure: {0}")]
    Internal(String),
}

impl WorkflowError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::InvalidState(_) => "INVALID_STATE",
            Self::PolicyDenied(_) => "POLICY_DENIED",
            Self::OutcomeUnknown(_) => "OUTCOME_UNKNOWN",
            Self::Engine(_) => "ENGINE",
            Self::Extension(_) => "EXTENSION",
            Self::Database(_) => "DATABASE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type WorkflowResult<T> = Result<T, WorkflowError>;
