use thiserror::Error;

#[derive(Debug, Error)]
pub enum CostError {
    #[error("cost validation failed: {0}")]
    Validation(String),
    #[error("cost record not found: {0}")]
    NotFound(String),
    #[error("cost state conflict: {0}")]
    Conflict(String),
    #[error("cost budget exceeded: {0}")]
    BudgetExceeded(String),
    #[error("cost database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("cost serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("cost internal failure: {0}")]
    Internal(String),
}

impl CostError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::BudgetExceeded(_) => "BUDGET_EXCEEDED",
            Self::Database(_) => "DATABASE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Internal(_) => "INTERNAL",
        }
    }
}

pub type CostResult<T> = Result<T, CostError>;