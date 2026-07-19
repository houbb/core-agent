use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::command_runtime::{CommandOutcome, CommandRequest, CommandRunner};

const MAX_HOOK_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_HOOKS: usize = 64;
const MAX_HOOK_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    AgentStart,
    AgentFinish,
    BeforeTool,
    AfterTool,
    ToolFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookFailurePolicy {
    FailOpen,
    FailClosed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookRule {
    pub name: String,
    pub event: HookEvent,
    pub command: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default = "default_hook_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_failure_policy")]
    pub failure_policy: HookFailurePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HookFile {
    version: u32,
    #[serde(default)]
    hooks: Vec<HookRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookInvocation {
    pub event: HookEvent,
    pub session_id: Option<uuid::Uuid>,
    pub tool: Option<String>,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookResult {
    pub hook: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub output: String,
    pub duration_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum HookRuntimeError {
    #[error("hook configuration is invalid: {0}")]
    Invalid(String),
    #[error("hook I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("hook serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("hook {hook} failed closed: {message}")]
    FailedClosed { hook: String, message: String },
    #[error("hook command failed: {0}")]
    Command(String),
}

pub type HookRuntimeResult<T> = Result<T, HookRuntimeError>;

pub struct HookRuntime {
    workspace: PathBuf,
    runner: Arc<dyn CommandRunner>,
    hooks: Vec<HookRule>,
}

impl HookRuntime {
    pub fn new(
        workspace: &Path,
        runner: Arc<dyn CommandRunner>,
        hooks: Vec<HookRule>,
    ) -> HookRuntimeResult<Self> {
        let workspace = std::fs::canonicalize(workspace)?;
        validate_hooks(&hooks)?;
        Ok(Self {
            workspace,
            runner,
            hooks,
        })
    }

    pub fn discover(
        workspace: &Path,
        global_directory: Option<&Path>,
        runner: Arc<dyn CommandRunner>,
    ) -> HookRuntimeResult<Option<Self>> {
        if std::env::var("CORE_AGENT_ENABLE_HOOKS").as_deref() != Ok("1") {
            return Ok(None);
        }
        let workspace = std::fs::canonicalize(workspace)?;
        let mut hooks = Vec::new();
        if let Some(directory) = global_directory {
            let path = directory.join("hooks.json");
            if path.exists() {
                hooks.extend(read_hook_file(&path, None)?);
            }
        }
        let project_path = workspace.join(".core-agent").join("hooks.json");
        if project_path.exists() {
            hooks.extend(read_hook_file(&project_path, Some(&workspace))?);
        }
        if hooks.is_empty() {
            return Ok(None);
        }
        Self::new(&workspace, runner, hooks).map(Some)
    }

    pub fn hooks(&self) -> &[HookRule] {
        &self.hooks
    }

    pub async fn run(
        &self,
        invocation: HookInvocation,
        cancellation: CancellationToken,
    ) -> HookRuntimeResult<Vec<HookResult>> {
        let input = serde_json::to_string(&invocation)?;
        let mut results = Vec::new();
        for hook in self
            .hooks
            .iter()
            .filter(|hook| hook_matches(hook, &invocation))
        {
            let mut request = CommandRequest::new(hook.command.clone());
            request.working_directory = ".".into();
            request.timeout_ms = hook.timeout_ms;
            request.max_output_bytes = MAX_HOOK_OUTPUT_BYTES;
            request.stdin = Some(input.clone());
            let outcome = self
                .runner
                .execute(request, cancellation.child_token())
                .await
                .map_err(|error| HookRuntimeError::Command(error.to_string()))?;
            let result = hook_result(hook, &outcome);
            if !outcome.success && hook.failure_policy == HookFailurePolicy::FailClosed {
                return Err(HookRuntimeError::FailedClosed {
                    hook: hook.name.clone(),
                    message: failure_message(&outcome),
                });
            }
            results.push(result);
        }
        Ok(results)
    }

    pub fn workspace(&self) -> &Path {
        &self.workspace
    }
}

fn read_hook_file(path: &Path, required_root: Option<&Path>) -> HookRuntimeResult<Vec<HookRule>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_HOOK_CONFIG_BYTES
    {
        return Err(HookRuntimeError::Invalid(format!(
            "{} must be a regular file no larger than 64 KiB",
            path.display()
        )));
    }
    let canonical = std::fs::canonicalize(path)?;
    if required_root.is_some_and(|root| !canonical.starts_with(root)) {
        return Err(HookRuntimeError::Invalid(
            "project hook configuration escaped the workspace".into(),
        ));
    }
    let file: HookFile = serde_json::from_slice(&std::fs::read(canonical)?)?;
    if file.version != 1 {
        return Err(HookRuntimeError::Invalid(
            "hook configuration version must be 1".into(),
        ));
    }
    validate_hooks(&file.hooks)?;
    Ok(file.hooks)
}

fn validate_hooks(hooks: &[HookRule]) -> HookRuntimeResult<()> {
    if hooks.len() > MAX_HOOKS {
        return Err(HookRuntimeError::Invalid(
            "hook configuration exceeds 64 rules".into(),
        ));
    }
    let mut names = std::collections::BTreeSet::new();
    for hook in hooks {
        if hook.name.trim().is_empty()
            || hook.name.len() > 128
            || !names.insert(hook.name.clone())
            || hook.command.trim().is_empty()
            || hook.command.len() > 16 * 1024
            || !(1_000..=120_000).contains(&hook.timeout_ms)
            || hook.tool.as_ref().is_some_and(|tool| {
                tool.trim().is_empty() || tool.len() > 386 || tool.chars().any(char::is_control)
            })
        {
            return Err(HookRuntimeError::Invalid(format!(
                "hook rule is invalid or duplicated: {}",
                hook.name
            )));
        }
    }
    Ok(())
}

fn hook_matches(rule: &HookRule, invocation: &HookInvocation) -> bool {
    rule.event == invocation.event
        && rule
            .tool
            .as_deref()
            .is_none_or(|matcher| matcher == "*" || invocation.tool.as_deref() == Some(matcher))
}

fn hook_result(rule: &HookRule, outcome: &CommandOutcome) -> HookResult {
    let output = if outcome.stderr.trim().is_empty() {
        outcome.stdout.clone()
    } else if outcome.stdout.trim().is_empty() {
        outcome.stderr.clone()
    } else {
        format!("{}\n{}", outcome.stdout, outcome.stderr)
    };
    HookResult {
        hook: rule.name.clone(),
        success: outcome.success,
        exit_code: outcome.exit_code,
        output,
        duration_ms: outcome.duration_ms,
    }
}

fn failure_message(outcome: &CommandOutcome) -> String {
    if outcome.cancelled {
        "cancelled".into()
    } else if outcome.timed_out {
        "timed out".into()
    } else if !outcome.stderr.trim().is_empty() {
        outcome.stderr.clone()
    } else {
        format!("exited with {:?}", outcome.exit_code)
    }
}

fn default_hook_timeout_ms() -> u64 {
    10_000
}

fn default_failure_policy() -> HookFailurePolicy {
    HookFailurePolicy::FailOpen
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct FixedRunner {
        outcome: CommandOutcome,
    }

    #[async_trait]
    impl CommandRunner for FixedRunner {
        async fn execute(
            &self,
            _request: CommandRequest,
            _cancellation: CancellationToken,
        ) -> crate::CommandRunResult<CommandOutcome> {
            Ok(self.outcome.clone())
        }
    }

    fn outcome(success: bool) -> CommandOutcome {
        CommandOutcome {
            exit_code: Some(if success { 0 } else { 3 }),
            success,
            timed_out: false,
            cancelled: false,
            stdout: "hook output".into(),
            stderr: String::new(),
            output_truncated: false,
            duration_ms: 2,
        }
    }

    #[tokio::test]
    async fn matching_hook_runs_and_fail_closed_blocks() {
        let directory = tempfile::tempdir().unwrap();
        let rule = HookRule {
            name: "guard-write".into(),
            event: HookEvent::BeforeTool,
            command: "guard".into(),
            tool: Some("write_file".into()),
            timeout_ms: 1_000,
            failure_policy: HookFailurePolicy::FailClosed,
        };
        let runtime = HookRuntime::new(
            directory.path(),
            Arc::new(FixedRunner {
                outcome: outcome(false),
            }),
            vec![rule],
        )
        .unwrap();
        let error = runtime
            .run(
                HookInvocation {
                    event: HookEvent::BeforeTool,
                    session_id: None,
                    tool: Some("write_file".into()),
                    payload: Value::Null,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, HookRuntimeError::FailedClosed { .. }));
    }

    #[test]
    fn config_rejects_duplicate_names_and_out_of_range_timeouts() {
        let rule = HookRule {
            name: "duplicate".into(),
            event: HookEvent::AgentStart,
            command: "echo ok".into(),
            tool: None,
            timeout_ms: 999,
            failure_policy: HookFailurePolicy::FailOpen,
        };
        assert!(validate_hooks(&[rule]).is_err());
    }
}
