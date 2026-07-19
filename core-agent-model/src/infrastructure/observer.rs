use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::ModelOperation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelStage {
    Routed,
    AttemptStarted,
    RetryScheduled,
    Fallback,
    Streaming,
    UsageFailed,
    Completed,
    Failed,
}

/// Content-free observation event suitable for tracing and metrics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelObservation {
    pub request_id: Uuid,
    pub operation: ModelOperation,
    pub stage: ModelStage,
    pub provider: String,
    pub model: String,
    pub profile: String,
    pub attempt: u32,
    pub duration_ms: u64,
    pub error_kind: Option<String>,
}

pub trait ModelObserver: Send + Sync {
    fn on_observation(&self, observation: &ModelObservation);
}
