use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRequest {
    pub session_id: Option<Uuid>,
    pub message: String,
    pub workspace: String,
}

impl AgentRequest {
    pub fn validate(&self) -> crate::CliResult<()> {
        if self.message.trim().is_empty()
            || self.message.len() > 64 * 1024
            || self.message.chars().any(|character| character == '\0')
        {
            return Err(crate::CliError::InvalidArgument(
                "goal must contain at most 64 KiB of text".into(),
            ));
        }
        if self.workspace.trim().is_empty() || self.workspace.len() > 4096 {
            return Err(crate::CliError::InvalidArgument(
                "workspace path is invalid".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Submission {
    pub session_id: Uuid,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEvent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub data: Value,
}

impl AgentEvent {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.kind.as_str(),
            "execution_finished" | "execution_failed" | "cancelled"
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub session_id: Uuid,
    pub state: String,
    pub model: Option<String>,
    pub memory_items: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub session_id: Uuid,
    pub state: String,
    pub title: Option<String>,
}
