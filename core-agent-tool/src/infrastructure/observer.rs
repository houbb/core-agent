use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolStage {
    Created,
    Validated,
    PermissionChecked,
    Ready,
    Running,
    Mapping,
    AuditFailed,
    Success,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolObservation {
    pub request_id: Uuid,
    pub tool_key: String,
    pub provider_key: String,
    pub stage: ToolStage,
    pub duration_ms: u64,
    pub error_kind: Option<String>,
}

pub trait ToolObserver: Send + Sync {
    fn on_observation(&self, observation: &ToolObservation);
}
