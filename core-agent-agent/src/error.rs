use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("agent validation failed: {0}")]
    Validation(String),
    #[error("agent entity not found: {0}")]
    NotFound(String),
    #[error("agent version conflict: {0}")]
    Conflict(String),
    #[error("invalid agent lifecycle transition: {0}")]
    InvalidState(String),
    #[error("agent policy denied operation: {0}")]
    PolicyDenied(String),
    #[error("agent coordination failed: {0}")]
    Coordination(String),
    #[error("agent coordination failed during {stage}: {message}")]
    PartialCoordination {
        stage: String,
        goal_id: Option<Uuid>,
        plan_id: Option<Uuid>,
        execution_id: Option<Uuid>,
        message: String,
    },
    #[error("agent database failed: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("agent serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("agent planning failed: {0}")]
    Planning(#[from] core_agent_plan::PlanError),
    #[error("agent execution failed: {0}")]
    Execution(#[from] core_agent_execution::ExecutionError),
    #[error("agent extension failed: {0}")]
    Extension(String),
    #[error("agent internal failure: {0}")]
    Internal(String),
}

impl AgentError {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Validation(_) => "VALIDATION",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Conflict(_) => "CONFLICT",
            Self::InvalidState(_) => "INVALID_STATE",
            Self::PolicyDenied(_) => "POLICY_DENIED",
            Self::Coordination(_) | Self::PartialCoordination { .. } => "COORDINATION",
            Self::Database(_) => "DATABASE",
            Self::Serialization(_) => "SERIALIZATION",
            Self::Planning(_) => "PLANNING",
            Self::Execution(_) => "EXECUTION",
            Self::Extension(_) => "EXTENSION",
            Self::Internal(_) => "INTERNAL",
        }
    }

    pub fn partial_reference(&self) -> Option<(Option<Uuid>, Option<Uuid>, Option<Uuid>)> {
        match self {
            Self::PartialCoordination {
                goal_id,
                plan_id,
                execution_id,
                ..
            } => Some((*goal_id, *plan_id, *execution_id)),
            _ => None,
        }
    }
}

pub type AgentResult<T> = Result<T, AgentError>;
