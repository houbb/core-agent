use uuid::Uuid;

use core_agent_message::MessageError;
use core_agent_subagent::SubAgentError;

pub type OrchestratorResult<T> = Result<T, OrchestratorError>;

#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    #[error("orchestration validation failed: {0}")]
    Validation(String),

    #[error("orchestration not found: {0}")]
    NotFound(String),

    #[error("orchestration version conflict: {0}")]
    Conflict(String),

    #[error("invalid orchestration state transition: {0}")]
    InvalidState(String),

    #[error("orchestration strategy execution failed: {0}")]
    StrategyExecution(String),

    #[error("orchestration result aggregation failed: {0}")]
    Aggregation(String),

    #[error("orchestration subagent error: {0}")]
    SubAgent(#[from] SubAgentError),

    #[error("orchestration message error: {0}")]
    Message(#[from] MessageError),

    #[error("orchestration database failed: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("orchestration serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("orchestration internal failure: {0}")]
    Internal(String),
}

impl OrchestratorError {
    pub fn not_found(id: Uuid) -> Self {
        Self::NotFound(id.to_string())
    }
}