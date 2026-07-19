use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use core_agent_plan::{Action, ActionKind, Plan, PlanStatus, ReviewDecision};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{ExecutionError, ExecutionResult};

const MAX_TEXT: usize = 4096;
const MAX_METADATA: usize = 64;
const MAX_EXECUTION_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionStatus {
    Pending,
    Ready,
    Running,
    Waiting,
    Retrying,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

impl ExecutionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Ready => "READY",
            Self::Running => "RUNNING",
            Self::Waiting => "WAITING",
            Self::Retrying => "RETRYING",
            Self::Paused => "PAUSED",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "PENDING" => Some(Self::Pending),
            "READY" => Some(Self::Ready),
            "RUNNING" => Some(Self::Running),
            "WAITING" => Some(Self::Waiting),
            "RETRYING" => Some(Self::Retrying),
            "PAUSED" => Some(Self::Paused),
            "COMPLETED" => Some(Self::Completed),
            "FAILED" => Some(Self::Failed),
            "CANCELLED" => Some(Self::Cancelled),
            _ => None,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActionExecutionStatus {
    Pending,
    Running,
    Retrying,
    Completed,
    Failed,
    Cancelled,
    RolledBack,
    RollbackFailed,
}

impl ActionExecutionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Running => "RUNNING",
            Self::Retrying => "RETRYING",
            Self::Completed => "COMPLETED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
            Self::RolledBack => "ROLLED_BACK",
            Self::RollbackFailed => "ROLLBACK_FAILED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepExecution {
    pub task_id: Uuid,
    pub step_id: Uuid,
    pub action_id: Uuid,
    pub status: ActionExecutionStatus,
    pub attempts: u32,
    pub command_id: Option<Uuid>,
    pub result: Option<CommandResult>,
    pub error: Option<CommandFailure>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Execution {
    pub id: Uuid,
    pub plan_id: Uuid,
    pub plan_version: u64,
    pub plan_hash: String,
    pub plan: Plan,
    pub status: ExecutionStatus,
    pub steps: BTreeMap<Uuid, StepExecution>,
    pub completed_order: Vec<Uuid>,
    pub current_task_id: Option<Uuid>,
    pub current_step_id: Option<Uuid>,
    pub version: u64,
    pub metadata: BTreeMap<String, String>,
    pub actor: String,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Execution {
    pub fn new(plan: Plan, request: ExecuteRequest) -> ExecutionResult<Self> {
        plan.validate()
            .map_err(|error| ExecutionError::Validation(error.to_string()))?;
        if plan.status != PlanStatus::Ready
            || plan.review.as_ref().map(|review| review.decision) != Some(ReviewDecision::Approved)
        {
            return Err(ExecutionError::InvalidState(
                "only an approved READY plan can execute".into(),
            ));
        }
        validate_actor(&request.actor)?;
        validate_metadata(&request.metadata)?;
        let now = Utc::now();
        let mut steps = BTreeMap::new();
        for task in plan.tasks.values() {
            for step in task.steps.values() {
                steps.insert(
                    step.id,
                    StepExecution {
                        task_id: task.id,
                        step_id: step.id,
                        action_id: step.action.id,
                        status: ActionExecutionStatus::Pending,
                        attempts: 0,
                        command_id: None,
                        result: None,
                        error: None,
                        started_at: None,
                        completed_at: None,
                    },
                );
            }
        }
        let plan_hash = semantic_hash(&plan)?;
        let execution = Self {
            id: Uuid::new_v4(),
            plan_id: plan.id,
            plan_version: plan.version,
            plan_hash,
            plan,
            status: ExecutionStatus::Pending,
            steps,
            completed_order: Vec::new(),
            current_task_id: None,
            current_step_id: None,
            version: 1,
            metadata: request.metadata,
            actor: request.actor,
            started_at: None,
            completed_at: None,
            created_at: now,
            updated_at: now,
        };
        execution.validate()?;
        Ok(execution)
    }

    pub fn validate(&self) -> ExecutionResult<()> {
        if self.plan_id != self.plan.id
            || self.plan_version != self.plan.version
            || self.plan_hash != semantic_hash(&self.plan)?
        {
            return Err(ExecutionError::Validation(
                "execution plan identity or hash mismatch".into(),
            ));
        }
        let expected = self
            .plan
            .tasks
            .values()
            .flat_map(|task| task.steps.values().map(move |step| (task, step)))
            .map(|(task, step)| (step.id, (task.id, step.action.id)))
            .collect::<BTreeMap<_, _>>();
        if self.steps.len() != expected.len()
            || self.steps.iter().any(|(id, progress)| {
                expected.get(id) != Some(&(progress.task_id, progress.action_id))
                    || progress.step_id != *id
                    || progress.attempts > 100
            })
        {
            return Err(ExecutionError::Validation(
                "execution progress does not match immutable plan".into(),
            ));
        }
        let completed = self
            .completed_order
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if completed.len() != self.completed_order.len()
            || completed.iter().any(|id| {
                self.steps.get(id).is_none_or(|value| {
                    !matches!(
                        value.status,
                        ActionExecutionStatus::Completed
                            | ActionExecutionStatus::RolledBack
                            | ActionExecutionStatus::RollbackFailed
                    )
                })
            })
        {
            return Err(ExecutionError::Validation(
                "execution completion order is inconsistent".into(),
            ));
        }
        if self.current_step_id.is_some() != self.current_task_id.is_some()
            || self.current_step_id.is_some_and(|step_id| {
                self.steps
                    .get(&step_id)
                    .is_none_or(|step| Some(step.task_id) != self.current_task_id)
            })
            || self.updated_at < self.created_at
            || self.status == ExecutionStatus::Running && self.started_at.is_none()
            || self.status.is_terminal() != self.completed_at.is_some()
            || self.status.is_terminal()
                && (self.current_task_id.is_some() || self.current_step_id.is_some())
            || self.status == ExecutionStatus::Completed
                && self
                    .steps
                    .values()
                    .any(|step| step.status != ActionExecutionStatus::Completed)
            || matches!(
                self.status,
                ExecutionStatus::Paused | ExecutionStatus::Cancelled
            ) && self.steps.values().any(|step| {
                matches!(
                    step.status,
                    ActionExecutionStatus::Running | ActionExecutionStatus::Retrying
                )
            })
            || self.version == 0
        {
            return Err(ExecutionError::Validation(
                "execution lifecycle fields are inconsistent".into(),
            ));
        }
        validate_actor(&self.actor)?;
        validate_metadata(&self.metadata)?;
        if serde_json::to_vec(self)?.len() > MAX_EXECUTION_BYTES {
            return Err(ExecutionError::Validation(
                "serialized execution exceeds 16 MiB".into(),
            ));
        }
        Ok(())
    }

    pub fn has_uncertain_action(&self) -> bool {
        self.steps
            .values()
            .any(|step| step.status == ActionExecutionStatus::Running)
    }
}

#[derive(Debug, Clone)]
pub struct ExecuteRequest {
    pub actor: String,
    pub metadata: BTreeMap<String, String>,
}

impl Default for ExecuteRequest {
    fn default() -> Self {
        Self::new("system")
    }
}

impl ExecuteRequest {
    pub fn new(actor: impl Into<String>) -> Self {
        Self {
            actor: actor.into(),
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CommandKind {
    Tool,
    Builtin,
}

impl CommandKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "TOOL",
            Self::Builtin => "BUILTIN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionCommand {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub task_id: Uuid,
    pub step_id: Uuid,
    pub action_id: Uuid,
    pub attempt: u32,
    pub kind: CommandKind,
    pub action_kind: ActionKind,
    pub tool_key: Option<String>,
    pub capability: Option<String>,
    pub target_uri: Option<String>,
    pub parameters: Value,
}

impl ExecutionCommand {
    pub fn from_action(
        execution_id: Uuid,
        task_id: Uuid,
        step_id: Uuid,
        action: &Action,
        attempt: u32,
    ) -> ExecutionResult<Self> {
        if attempt == 0 {
            return Err(ExecutionError::Validation(
                "command attempt must be greater than zero".into(),
            ));
        }
        let identity = format!("{}:{attempt}", action.id);
        let command = Self {
            id: Uuid::new_v5(&execution_id, identity.as_bytes()),
            execution_id,
            task_id,
            step_id,
            action_id: action.id,
            attempt,
            kind: if action.kind == ActionKind::InvokeTool {
                CommandKind::Tool
            } else {
                CommandKind::Builtin
            },
            action_kind: action.kind,
            tool_key: action.tool_key.clone(),
            capability: action.capability.clone(),
            target_uri: action.target_uri.clone(),
            parameters: action.parameters.clone(),
        };
        command.validate()?;
        Ok(command)
    }

    pub fn validate(&self) -> ExecutionResult<()> {
        if self.attempt == 0
            || (self.kind == CommandKind::Tool) != self.tool_key.is_some()
            || (self.kind == CommandKind::Tool) != (self.action_kind == ActionKind::InvokeTool)
        {
            return Err(ExecutionError::Validation(
                "command kind, action and Tool binding are inconsistent".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResult {
    pub summary: String,
    pub duration_ms: u64,
    pub output_bytes: u64,
}

impl CommandResult {
    pub fn acknowledged(kind: ActionKind) -> Self {
        Self {
            summary: format!("{} marker acknowledged", kind.as_str()),
            duration_ms: 0,
            output_bytes: 0,
        }
    }

    pub fn validate(&self) -> Result<(), CommandFailure> {
        if self.summary.trim().is_empty()
            || self.summary.chars().count() > 1024
            || self.summary.chars().any(char::is_control)
        {
            return Err(CommandFailure::new(
                "INVALID_COMMAND_RESULT",
                "command result summary must contain 1..=1024 safe characters",
                false,
            ));
        }
        Ok(())
    }

    pub fn bounded(mut self) -> Self {
        self.summary = bounded_text(
            &self.summary,
            1024,
            "command completed without a result summary",
        );
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandFailure {
    pub kind: String,
    pub message: String,
    pub retryable: bool,
    pub cancelled: bool,
}

impl CommandFailure {
    pub fn new(kind: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            retryable,
            cancelled: false,
        }
    }

    pub fn cancelled(message: impl Into<String>) -> Self {
        Self {
            kind: "CANCELLED".into(),
            message: message.into(),
            retryable: false,
            cancelled: true,
        }
    }

    pub fn bounded(mut self) -> Self {
        self.kind = bounded_text(&self.kind, 64, "COMMAND_FAILED");
        self.message = bounded_text(&self.message, 1024, "command failed without a message");
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionCheckpoint {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub sequence: u64,
    pub label: String,
    pub content: Execution,
    pub hash: String,
    pub created_at: DateTime<Utc>,
}

impl ExecutionCheckpoint {
    pub fn capture(execution: &Execution, label: impl Into<String>) -> ExecutionResult<Self> {
        let label = label.into();
        if label.trim().is_empty() || label.len() > 256 {
            return Err(ExecutionError::Validation(
                "checkpoint label must contain 1..=256 characters".into(),
            ));
        }
        Ok(Self {
            id: Uuid::new_v4(),
            execution_id: execution.id,
            sequence: execution.version,
            label,
            content: execution.clone(),
            hash: semantic_hash(execution)?,
            created_at: Utc::now(),
        })
    }

    pub fn validate(&self) -> ExecutionResult<()> {
        if self.execution_id != self.content.id
            || self.sequence != self.content.version
            || self.hash != semantic_hash(&self.content)?
        {
            return Err(ExecutionError::Validation(
                "checkpoint identity or hash mismatch".into(),
            ));
        }
        self.content.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionStateRecord {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub sequence: u64,
    pub from_status: Option<ExecutionStatus>,
    pub to_status: ExecutionStatus,
    pub reason: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RetryStatus {
    Scheduled,
    Resumed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryRecord {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub step_id: Uuid,
    pub action_id: Uuid,
    pub attempt: u32,
    pub delay_ms: u64,
    pub error_kind: String,
    pub error_message: String,
    pub status: RetryStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollbackStatus {
    Success,
    Failed,
    Skipped,
}

impl RollbackStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failed => "FAILED",
            Self::Skipped => "SKIPPED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackRecord {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub step_id: Uuid,
    pub action_id: Uuid,
    pub command_id: Uuid,
    pub status: RollbackStatus,
    pub error_kind: Option<String>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub(crate) fn semantic_hash<T: Serialize>(value: &T) -> ExecutionResult<String> {
    let bytes = serde_json::to_vec(value)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_actor(actor: &str) -> ExecutionResult<()> {
    if actor.trim().is_empty() || actor.len() > 256 || actor.chars().any(char::is_control) {
        return Err(ExecutionError::Validation(
            "execution actor must contain 1..=256 safe characters".into(),
        ));
    }
    Ok(())
}

fn validate_metadata(metadata: &BTreeMap<String, String>) -> ExecutionResult<()> {
    const SENSITIVE: [&str; 6] = [
        "token",
        "secret",
        "password",
        "api_key",
        "authorization",
        "cookie",
    ];
    if metadata.len() > MAX_METADATA
        || metadata.iter().any(|(key, value)| {
            key.trim().is_empty()
                || key.len() > 128
                || value.len() > MAX_TEXT
                || SENSITIVE
                    .iter()
                    .any(|part| key.to_ascii_lowercase().contains(part))
        })
    {
        return Err(ExecutionError::Validation(
            "execution metadata exceeds bounds or contains a sensitive key".into(),
        ));
    }
    Ok(())
}

fn bounded_text(value: &str, max: usize, fallback: &str) -> String {
    let value = value
        .chars()
        .filter(|character| !character.is_control())
        .take(max)
        .collect::<String>();
    if value.trim().is_empty() {
        fallback.into()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_identity_is_stable_per_attempt() {
        let action = Action {
            id: Uuid::new_v4(),
            kind: ActionKind::InvokeTool,
            tool_key: Some("builtin/echo@1".into()),
            capability: None,
            target_uri: None,
            parameters: serde_json::json!({"value": 1}),
        };
        let execution = Uuid::new_v4();
        let first =
            ExecutionCommand::from_action(execution, Uuid::new_v4(), Uuid::new_v4(), &action, 1)
                .unwrap();
        let same =
            ExecutionCommand::from_action(execution, first.task_id, first.step_id, &action, 1)
                .unwrap();
        let retry =
            ExecutionCommand::from_action(execution, first.task_id, first.step_id, &action, 2)
                .unwrap();
        assert_eq!(first.id, same.id);
        assert_ne!(first.id, retry.id);
    }

    #[test]
    fn state_parser_is_strict() {
        assert_eq!(
            ExecutionStatus::parse("RUNNING"),
            Some(ExecutionStatus::Running)
        );
        assert_eq!(ExecutionStatus::parse("running"), None);
    }
}
