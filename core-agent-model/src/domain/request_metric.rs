use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ModelError, ModelResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RequestStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}

impl RequestStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
            Self::Interrupted => "INTERRUPTED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "RUNNING" => Some(Self::Running),
            "COMPLETED" => Some(Self::Completed),
            "FAILED" => Some(Self::Failed),
            "CANCELLED" => Some(Self::Cancelled),
            "INTERRUPTED" => Some(Self::Interrupted),
            _ => None,
        }
    }
}

/// Content-free metric for one user-visible Agent request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequestMetric {
    pub id: Uuid,
    pub workspace_key: String,
    pub session_id: Option<Uuid>,
    pub entrypoint: String,
    pub model_name: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub wall_duration_ms: u64,
    pub active_duration_ms: u64,
    pub approval_wait_ms: u64,
    pub context_duration_ms: u64,
    pub model_duration_ms: u64,
    pub tool_duration_ms: u64,
    pub context_tokens: u64,
    pub status: RequestStatus,
    pub error_kind: Option<String>,
}

impl AgentRequestMetric {
    pub fn running(
        id: Uuid,
        workspace_key: impl Into<String>,
        session_id: Option<Uuid>,
        entrypoint: impl Into<String>,
        model_name: impl Into<String>,
        started_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            workspace_key: workspace_key.into(),
            session_id,
            entrypoint: entrypoint.into(),
            model_name: model_name.into(),
            started_at,
            completed_at: None,
            wall_duration_ms: 0,
            active_duration_ms: 0,
            approval_wait_ms: 0,
            context_duration_ms: 0,
            model_duration_ms: 0,
            tool_duration_ms: 0,
            context_tokens: 0,
            status: RequestStatus::Running,
            error_kind: None,
        }
    }

    pub fn validate(&self) -> ModelResult<()> {
        if self.workspace_key.trim().is_empty()
            || self.workspace_key.len() > 256
            || self.entrypoint.trim().is_empty()
            || self.entrypoint.len() > 32
            || self.model_name.trim().is_empty()
            || self.model_name.len() > 256
            || self.active_duration_ms > self.wall_duration_ms
            || self.approval_wait_ms > self.wall_duration_ms
            || self
                .error_kind
                .as_ref()
                .is_some_and(|value| value.len() > 128 || value.chars().any(char::is_control))
            || (self.status == RequestStatus::Running && self.completed_at.is_some())
            || (self.status != RequestStatus::Running && self.completed_at.is_none())
        {
            return Err(ModelError::InvalidArgument(
                "agent request metric is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageBucket {
    pub day: String,
    pub model_name: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub cache_tokens: u64,
    pub total_tokens: u64,
    pub model_calls: u64,
}
