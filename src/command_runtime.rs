use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::{mpsc, Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::enterprise::resolve_workspace_resource;
use crate::{
    FunctionTool, PermissionDecision, RawToolOutput, ToolContent, ToolDefinition, ToolError,
    ToolRegistration,
};

const DEFAULT_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_COMMAND_BYTES: usize = 16 * 1024;
const STREAM_CHUNK_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRequest {
    pub command: String,
    #[serde(default = "default_working_directory", alias = "working_directory")]
    pub working_directory: String,
    #[serde(default = "default_timeout_ms", alias = "timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_max_output_bytes", alias = "max_output_bytes")]
    pub max_output_bytes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdin: Option<String>,
}

impl CommandRequest {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            working_directory: default_working_directory(),
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            stdin: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandOutcome {
    pub exit_code: Option<i32>,
    pub success: bool,
    pub timed_out: bool,
    pub cancelled: bool,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxRequirement {
    Disabled,
    BestEffort,
    Required,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxNetworkPolicy {
    Deny,
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSandboxPolicy {
    pub requirement: SandboxRequirement,
    pub network: SandboxNetworkPolicy,
}

impl Default for CommandSandboxPolicy {
    fn default() -> Self {
        Self {
            requirement: SandboxRequirement::BestEffort,
            network: SandboxNetworkPolicy::Deny,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSandboxStatus {
    pub backend: String,
    pub available: bool,
    pub enforced: bool,
    pub network_isolated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandEvent {
    Started {
        command_bytes: usize,
        working_directory: String,
        timeout_ms: u64,
    },
    Output {
        stream: CommandStream,
        chunk: String,
    },
    Finished {
        exit_code: Option<i32>,
        timed_out: bool,
        cancelled: bool,
        output_truncated: bool,
        duration_ms: u64,
    },
}

pub trait CommandObserver: Send + Sync {
    fn observe(&self, event: &CommandEvent);
}

#[derive(Default)]
pub struct NoopCommandObserver;

impl CommandObserver for NoopCommandObserver {
    fn observe(&self, _event: &CommandEvent) {}
}

#[derive(Debug, thiserror::Error)]
pub enum CommandRunError {
    #[error("command request is invalid: {0}")]
    Invalid(String),
    #[error("command denied by hard safety policy")]
    Denied,
    #[error("command was cancelled")]
    Cancelled,
    #[error("required command sandbox is unavailable")]
    SandboxUnavailable,
    #[error("command process failed: {0}")]
    Process(#[from] std::io::Error),
}

pub type CommandRunResult<T> = Result<T, CommandRunError>;

#[async_trait]
pub trait CommandRunner: Send + Sync {
    async fn execute(
        &self,
        request: CommandRequest,
        cancellation: CancellationToken,
    ) -> CommandRunResult<CommandOutcome>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundCommandStatus {
    Queued,
    Running,
    Completed,
    Failed,
    TimedOut,
    Cancelled,
}

impl BackgroundCommandStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::TimedOut | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundCommandSnapshot {
    pub id: uuid::Uuid,
    pub status: BackgroundCommandStatus,
    pub working_directory: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub outcome: Option<CommandOutcome>,
    pub error: Option<String>,
}

struct BackgroundTask {
    cancellation: CancellationToken,
    snapshot: Mutex<BackgroundCommandSnapshot>,
}

pub struct BackgroundCommandManager {
    runner: Arc<dyn CommandRunner>,
    tasks: RwLock<BTreeMap<uuid::Uuid, Arc<BackgroundTask>>>,
    max_tasks: usize,
}

impl BackgroundCommandManager {
    pub fn new(runner: Arc<dyn CommandRunner>) -> Arc<Self> {
        Arc::new(Self {
            runner,
            tasks: RwLock::new(BTreeMap::new()),
            max_tasks: 64,
        })
    }

    pub async fn start(
        self: &Arc<Self>,
        request: CommandRequest,
    ) -> CommandRunResult<BackgroundCommandSnapshot> {
        let id = uuid::Uuid::new_v4();
        let snapshot = BackgroundCommandSnapshot {
            id,
            status: BackgroundCommandStatus::Queued,
            working_directory: request.working_directory.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
            outcome: None,
            error: None,
        };
        let task = Arc::new(BackgroundTask {
            cancellation: CancellationToken::new(),
            snapshot: Mutex::new(snapshot.clone()),
        });
        {
            let mut tasks = self.tasks.write().await;
            if tasks.len() >= self.max_tasks {
                let completed = tasks.iter().find_map(|(id, task)| {
                    task.snapshot
                        .try_lock()
                        .ok()
                        .and_then(|snapshot| snapshot.status.is_terminal().then_some(*id))
                });
                if let Some(completed) = completed {
                    tasks.remove(&completed);
                }
            }
            if tasks.len() >= self.max_tasks {
                return Err(CommandRunError::Invalid(
                    "background command limit of 64 active tasks was reached".into(),
                ));
            }
            tasks.insert(id, task.clone());
        }
        let manager = self.clone();
        tokio::spawn(async move {
            {
                let mut state = task.snapshot.lock().await;
                state.status = BackgroundCommandStatus::Running;
                state.started_at = Some(chrono::Utc::now().to_rfc3339());
            }
            let result = manager
                .runner
                .execute(request, task.cancellation.clone())
                .await;
            let mut state = task.snapshot.lock().await;
            state.completed_at = Some(chrono::Utc::now().to_rfc3339());
            match result {
                Ok(outcome) => {
                    state.status = if outcome.cancelled {
                        BackgroundCommandStatus::Cancelled
                    } else if outcome.timed_out {
                        BackgroundCommandStatus::TimedOut
                    } else if outcome.success {
                        BackgroundCommandStatus::Completed
                    } else {
                        BackgroundCommandStatus::Failed
                    };
                    state.outcome = Some(outcome);
                }
                Err(CommandRunError::Cancelled) => {
                    state.status = BackgroundCommandStatus::Cancelled;
                    state.error = Some("command was cancelled".into());
                }
                Err(error) => {
                    state.status = BackgroundCommandStatus::Failed;
                    state.error = Some(error.to_string());
                }
            }
        });
        Ok(snapshot)
    }

    pub async fn poll(&self, id: uuid::Uuid) -> Option<BackgroundCommandSnapshot> {
        let task = self.tasks.read().await.get(&id).cloned()?;
        let snapshot = task.snapshot.lock().await.clone();
        Some(snapshot)
    }

    pub async fn cancel(&self, id: uuid::Uuid) -> bool {
        let Some(task) = self.tasks.read().await.get(&id).cloned() else {
            return false;
        };
        if task.snapshot.lock().await.status.is_terminal() {
            return false;
        }
        task.cancellation.cancel();
        true
    }
}

pub struct LocalCommandRunner {
    workspace: PathBuf,
    observer: Arc<dyn CommandObserver>,
    denied_fragments: BTreeSet<String>,
    sandbox_policy: CommandSandboxPolicy,
    sandbox_backend: Option<PathBuf>,
}

impl LocalCommandRunner {
    pub fn new(workspace: &Path) -> CommandRunResult<Self> {
        let workspace = std::fs::canonicalize(workspace)?;
        if !workspace.is_dir() {
            return Err(CommandRunError::Invalid(
                "workspace must be an existing directory".into(),
            ));
        }
        Ok(Self {
            workspace,
            observer: Arc::new(NoopCommandObserver),
            denied_fragments: default_denied_fragments(),
            sandbox_policy: CommandSandboxPolicy::default(),
            sandbox_backend: detect_sandbox_backend(),
        })
    }

    pub fn with_observer(mut self, observer: Arc<dyn CommandObserver>) -> Self {
        self.observer = observer;
        self
    }

    pub fn with_denied_fragments(mut self, fragments: impl IntoIterator<Item = String>) -> Self {
        self.denied_fragments.extend(
            fragments
                .into_iter()
                .map(|value| value.trim().to_ascii_lowercase())
                .filter(|value| !value.is_empty()),
        );
        self
    }

    pub fn with_sandbox_policy(mut self, policy: CommandSandboxPolicy) -> Self {
        self.sandbox_policy = policy;
        self
    }

    pub fn sandbox_status(&self) -> CommandSandboxStatus {
        let available = self.sandbox_backend.is_some();
        let enforced = available && self.sandbox_policy.requirement != SandboxRequirement::Disabled;
        CommandSandboxStatus {
            backend: self
                .sandbox_backend
                .as_ref()
                .map(|_| "bubblewrap")
                .unwrap_or("none")
                .into(),
            available,
            enforced,
            network_isolated: enforced && self.sandbox_policy.network == SandboxNetworkPolicy::Deny,
        }
    }

    fn validate(&self, request: &mut CommandRequest) -> CommandRunResult<PathBuf> {
        if request.command.trim().is_empty()
            || request.command.len() > MAX_COMMAND_BYTES
            || request.command.contains('\0')
        {
            return Err(CommandRunError::Invalid(
                "command must contain 1..=16384 bytes and no NUL".into(),
            ));
        }
        let normalized = request.command.to_ascii_lowercase();
        if self
            .denied_fragments
            .iter()
            .any(|fragment| normalized.contains(fragment))
        {
            return Err(CommandRunError::Denied);
        }
        if !(1_000..=DEFAULT_TIMEOUT_MS).contains(&request.timeout_ms) {
            return Err(CommandRunError::Invalid(
                "timeout_ms must be between 1000 and 120000".into(),
            ));
        }
        if request
            .stdin
            .as_ref()
            .is_some_and(|value| value.len() > 256 * 1024 || value.contains('\0'))
        {
            return Err(CommandRunError::Invalid(
                "stdin must contain at most 256 KiB and no NUL".into(),
            ));
        }
        request.max_output_bytes = request
            .max_output_bytes
            .clamp(STREAM_CHUNK_BYTES, DEFAULT_MAX_OUTPUT_BYTES);
        if self.sandbox_policy.requirement == SandboxRequirement::Required
            && self.sandbox_backend.is_none()
        {
            return Err(CommandRunError::SandboxUnavailable);
        }
        resolve_workspace_resource(&self.workspace, &request.working_directory)
            .map_err(|error| CommandRunError::Invalid(error.to_string()))
            .and_then(|path| {
                path.is_dir().then_some(path).ok_or_else(|| {
                    CommandRunError::Invalid("working_directory must be a directory".into())
                })
            })
    }
}

#[async_trait]
impl CommandRunner for LocalCommandRunner {
    async fn execute(
        &self,
        mut request: CommandRequest,
        cancellation: CancellationToken,
    ) -> CommandRunResult<CommandOutcome> {
        let working_directory = self.validate(&mut request)?;
        if cancellation.is_cancelled() {
            return Err(CommandRunError::Cancelled);
        }
        let started = Instant::now();
        self.observer.observe(&CommandEvent::Started {
            command_bytes: request.command.len(),
            working_directory: relative_display(&self.workspace, &working_directory),
            timeout_ms: request.timeout_ms,
        });

        let mut command = shell_command(
            &request.command,
            &self.workspace,
            &working_directory,
            &self.sandbox_policy,
            self.sandbox_backend.as_deref(),
        );
        #[cfg(unix)]
        command.process_group(0);
        let has_stdin = request.stdin.is_some();
        command
            .current_dir(&working_directory)
            .kill_on_drop(true)
            .stdin(if has_stdin {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .envs(filtered_environment());
        let mut child = command.spawn()?;
        let stdin_task = request.stdin.take().and_then(|input| {
            child.stdin.take().map(|mut stdin| {
                tokio::spawn(async move {
                    let _ = stdin.write_all(input.as_bytes()).await;
                    let _ = stdin.shutdown().await;
                })
            })
        });
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| CommandRunError::Invalid("child stdout pipe was unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| CommandRunError::Invalid("child stderr pipe was unavailable".into()))?;
        let (sender, mut receiver) = mpsc::channel::<(CommandStream, Vec<u8>)>(32);
        let stdout_task = tokio::spawn(read_stream(stdout, CommandStream::Stdout, sender.clone()));
        let stderr_task = tokio::spawn(read_stream(stderr, CommandStream::Stderr, sender.clone()));
        drop(sender);

        enum Completion {
            Exited(std::process::ExitStatus),
            TimedOut,
            Cancelled,
        }

        let mut stdout_bytes = Vec::new();
        let mut stderr_bytes = Vec::new();
        let mut output_truncated = false;
        let deadline = tokio::time::sleep(Duration::from_millis(request.timeout_ms));
        tokio::pin!(deadline);
        let completion = {
            let wait = child.wait();
            tokio::pin!(wait);
            loop {
                tokio::select! {
                    status = &mut wait => break Completion::Exited(status?),
                    _ = &mut deadline => break Completion::TimedOut,
                    _ = cancellation.cancelled() => break Completion::Cancelled,
                    chunk = receiver.recv() => {
                        if let Some((stream, bytes)) = chunk {
                            self.observer.observe(&CommandEvent::Output {
                                stream,
                                chunk: String::from_utf8_lossy(&bytes).into_owned(),
                            });
                            append_bounded(
                                stream,
                                &bytes,
                                request.max_output_bytes,
                                &mut stdout_bytes,
                                &mut stderr_bytes,
                                &mut output_truncated,
                            );
                        }
                    }
                }
            }
        };

        let (status, timed_out, cancelled) = match completion {
            Completion::Exited(status) => (Some(status), false, false),
            Completion::TimedOut => {
                terminate_child_tree(&mut child).await;
                (child.wait().await.ok(), true, false)
            }
            Completion::Cancelled => {
                terminate_child_tree(&mut child).await;
                (child.wait().await.ok(), false, true)
            }
        };

        while let Some((stream, bytes)) = receiver.recv().await {
            self.observer.observe(&CommandEvent::Output {
                stream,
                chunk: String::from_utf8_lossy(&bytes).into_owned(),
            });
            append_bounded(
                stream,
                &bytes,
                request.max_output_bytes,
                &mut stdout_bytes,
                &mut stderr_bytes,
                &mut output_truncated,
            );
        }
        let _ = stdout_task.await;
        let _ = stderr_task.await;
        if let Some(stdin_task) = stdin_task {
            let _ = stdin_task.await;
        }

        let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        let exit_code = status.and_then(|value| value.code());
        self.observer.observe(&CommandEvent::Finished {
            exit_code,
            timed_out,
            cancelled,
            output_truncated,
            duration_ms,
        });
        Ok(CommandOutcome {
            exit_code,
            success: status.is_some_and(|value| value.success()) && !timed_out && !cancelled,
            timed_out,
            cancelled,
            stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            output_truncated,
            duration_ms,
        })
    }
}

pub(crate) fn registration(runner: Arc<dyn CommandRunner>) -> ToolRegistration {
    let mut definition = ToolDefinition::new(
        "workspace",
        "run_command",
        "2.0.0",
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {"type": "string", "description": "Command executed by the platform shell"},
                "working_directory": {"type": "string", "description": "Relative workspace directory, default ."},
                "timeout_ms": {"type": "integer", "minimum": 1000, "maximum": 120000},
                "max_output_bytes": {"type": "integer", "minimum": 8192, "maximum": 1048576}
            },
            "additionalProperties": false
        }),
    );
    definition.description = "Run a foreground command inside the opened workspace with bounded streaming output, cancellation, timeout, secret-stripped environment and structured exit status.".into();
    definition.category = "process.execute".into();
    definition.default_permission = PermissionDecision::Ask;
    definition.timeout_ms = 125_000;
    let key = definition.key.clone();
    let tool = Arc::new(FunctionTool::new(key, move |request, context| {
        let runner = runner.clone();
        async move {
            let command_request: CommandRequest = serde_json::from_value(request.parameters)
                .map_err(|error| ToolError::InvalidArgument(error.to_string()))?;
            let outcome = runner
                .execute(command_request, context.cancellation)
                .await
                .map_err(command_tool_error)?;
            Ok(RawToolOutput {
                content: vec![ToolContent::Json(
                    serde_json::to_value(outcome)
                        .map_err(|error| ToolError::Serialization(error.to_string()))?,
                )],
                ..RawToolOutput::default()
            })
        }
    }));
    ToolRegistration::new(definition, tool)
}

pub(crate) fn background_registrations(
    manager: Arc<BackgroundCommandManager>,
) -> Vec<ToolRegistration> {
    let mut start_definition = ToolDefinition::new(
        "workspace",
        "start_command",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {"type": "string"},
                "working_directory": {"type": "string", "description": "Relative workspace directory, default ."},
                "timeout_ms": {"type": "integer", "minimum": 1000, "maximum": 120000},
                "max_output_bytes": {"type": "integer", "minimum": 8192, "maximum": 1048576}
            },
            "additionalProperties": false
        }),
    );
    start_definition.description = "Start a governed background command and return a task id immediately. Use poll_command to retrieve structured completion output.".into();
    start_definition.category = "process.execute".into();
    start_definition.default_permission = PermissionDecision::Ask;
    let start_key = start_definition.key.clone();
    let start_manager = manager.clone();
    let start_tool = Arc::new(FunctionTool::new(start_key, move |request, context| {
        let manager = start_manager.clone();
        async move {
            if context.is_cancelled() {
                return Err(ToolError::Cancelled(request.id.to_string()));
            }
            let command: CommandRequest = serde_json::from_value(request.parameters)
                .map_err(|error| ToolError::InvalidArgument(error.to_string()))?;
            let snapshot = manager.start(command).await.map_err(command_tool_error)?;
            Ok(RawToolOutput {
                content: vec![ToolContent::Json(
                    serde_json::to_value(snapshot)
                        .map_err(|error| ToolError::Serialization(error.to_string()))?,
                )],
                ..RawToolOutput::default()
            })
        }
    }));

    let mut poll_definition = ToolDefinition::new(
        "workspace",
        "poll_command",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["id"],
            "properties": {"id": {"type": "string", "format": "uuid"}},
            "additionalProperties": false
        }),
    );
    poll_definition.description =
        "Poll a background command by task id and return status plus bounded final output.".into();
    poll_definition.category = "process.read".into();
    poll_definition.default_permission = PermissionDecision::Allow;
    let poll_key = poll_definition.key.clone();
    let poll_manager = manager.clone();
    let poll_tool = Arc::new(FunctionTool::new(poll_key, move |request, _| {
        let manager = poll_manager.clone();
        async move {
            let id = command_task_id(&request.parameters)?;
            let snapshot = manager
                .poll(id)
                .await
                .ok_or_else(|| ToolError::InvalidArgument(format!("unknown command task: {id}")))?;
            Ok(RawToolOutput {
                content: vec![ToolContent::Json(
                    serde_json::to_value(snapshot)
                        .map_err(|error| ToolError::Serialization(error.to_string()))?,
                )],
                ..RawToolOutput::default()
            })
        }
    }));

    let mut cancel_definition = ToolDefinition::new(
        "workspace",
        "cancel_command",
        "1.0.0",
        json!({
            "type": "object",
            "required": ["id"],
            "properties": {"id": {"type": "string", "format": "uuid"}},
            "additionalProperties": false
        }),
    );
    cancel_definition.description =
        "Cancel one active background command and terminate its process tree.".into();
    cancel_definition.category = "process.cancel".into();
    cancel_definition.default_permission = PermissionDecision::Ask;
    let cancel_key = cancel_definition.key.clone();
    let cancel_tool = Arc::new(FunctionTool::new(cancel_key, move |request, _| {
        let manager = manager.clone();
        async move {
            let id = command_task_id(&request.parameters)?;
            Ok(RawToolOutput {
                content: vec![ToolContent::Json(json!({
                    "id": id,
                    "cancelRequested": manager.cancel(id).await
                }))],
                ..RawToolOutput::default()
            })
        }
    }));

    vec![
        ToolRegistration::new(start_definition, start_tool),
        ToolRegistration::new(poll_definition, poll_tool),
        ToolRegistration::new(cancel_definition, cancel_tool),
    ]
}

fn command_task_id(parameters: &serde_json::Value) -> Result<uuid::Uuid, ToolError> {
    let id = parameters
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ToolError::InvalidArgument("command task id is required".into()))?;
    uuid::Uuid::parse_str(id)
        .map_err(|error| ToolError::InvalidArgument(format!("invalid command task id: {error}")))
}

fn default_working_directory() -> String {
    ".".into()
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

fn default_max_output_bytes() -> usize {
    DEFAULT_MAX_OUTPUT_BYTES
}

#[cfg(windows)]
fn shell_command(
    command: &str,
    _workspace: &Path,
    _working_directory: &Path,
    _policy: &CommandSandboxPolicy,
    _backend: Option<&Path>,
) -> Command {
    let mut process = Command::new("powershell");
    process.args([
        "-NoLogo",
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        command,
    ]);
    process
}

#[cfg(not(windows))]
fn shell_command(
    command: &str,
    workspace: &Path,
    working_directory: &Path,
    policy: &CommandSandboxPolicy,
    backend: Option<&Path>,
) -> Command {
    if policy.requirement != SandboxRequirement::Disabled {
        if let Some(backend) = backend {
            let mut process = Command::new(backend);
            process.args([
                "--die-with-parent",
                "--new-session",
                "--unshare-pid",
                "--unshare-uts",
                "--unshare-ipc",
            ]);
            if policy.network == SandboxNetworkPolicy::Deny {
                process.arg("--unshare-net");
            }
            process
                .args(["--ro-bind", "/", "/", "--bind"])
                .arg(workspace)
                .arg(workspace)
                .args([
                    "--dev", "/dev", "--proc", "/proc", "--tmpfs", "/tmp", "--chdir",
                ])
                .arg(working_directory)
                .args(["sh", "-lc", command]);
            return process;
        }
    }
    let mut process = Command::new("sh");
    process.args(["-lc", command]);
    process
}

#[cfg(windows)]
fn detect_sandbox_backend() -> Option<PathBuf> {
    None
}

#[cfg(not(windows))]
fn detect_sandbox_backend() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join("bwrap"))
            .find(|candidate| candidate.is_file())
    })
}

async fn read_stream<R>(
    mut reader: R,
    stream: CommandStream,
    sender: mpsc::Sender<(CommandStream, Vec<u8>)>,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buffer = vec![0_u8; STREAM_CHUNK_BYTES];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                if sender
                    .send((stream, buffer[..read].to_vec()))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        }
    }
}

fn append_bounded(
    stream: CommandStream,
    bytes: &[u8],
    limit: usize,
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
    truncated: &mut bool,
) {
    let used = stdout.len().saturating_add(stderr.len());
    let available = limit.saturating_sub(used);
    let take = available.min(bytes.len());
    match stream {
        CommandStream::Stdout => stdout.extend_from_slice(&bytes[..take]),
        CommandStream::Stderr => stderr.extend_from_slice(&bytes[..take]),
    }
    *truncated |= take < bytes.len() || stdout.len().saturating_add(stderr.len()) >= limit;
}

fn filtered_environment() -> BTreeMap<std::ffi::OsString, std::ffi::OsString> {
    std::env::vars_os()
        .filter(|(name, _)| !sensitive_environment_name(&name.to_string_lossy()))
        .collect()
}

fn sensitive_environment_name(name: &str) -> bool {
    let normalized = name.to_ascii_uppercase();
    [
        "API_KEY",
        "APIKEY",
        "AUTH_TOKEN",
        "ACCESS_TOKEN",
        "REFRESH_TOKEN",
        "CLIENT_SECRET",
        "PRIVATE_KEY",
        "PASSWORD",
        "CREDENTIAL",
    ]
    .iter()
    .any(|fragment| normalized.contains(fragment))
}

fn default_denied_fragments() -> BTreeSet<String> {
    [
        "format ",
        "diskpart",
        "shutdown ",
        "restart-computer",
        "stop-computer",
        "reg delete",
        "remove-item env:",
        "git reset --hard",
        "git clean -f",
        "rm -rf /",
        "rm -rf ~",
        "mkfs.",
        ":(){:|:&};:",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

async fn terminate_child_tree(child: &mut tokio::process::Child) {
    #[cfg(windows)]
    if let Some(process_id) = child.id() {
        let process_id = process_id.to_string();
        let _ = Command::new("taskkill")
            .args(["/PID", process_id.as_str(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    #[cfg(unix)]
    if let Some(process_id) = child.id() {
        let process_group = format!("-{process_id}");
        let _ = Command::new("kill")
            .args(["-TERM", process_group.as_str()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    let _ = child.kill().await;
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn command_tool_error(error: CommandRunError) -> ToolError {
    match error {
        CommandRunError::Invalid(message) => ToolError::InvalidArgument(message),
        CommandRunError::Denied => {
            ToolError::PermissionDenied("command matched the hard safety policy".into())
        }
        CommandRunError::Cancelled => ToolError::Cancelled("run_command".into()),
        CommandRunError::SandboxUnavailable => ToolError::PolicyDenied(
            "run_command requires an OS sandbox but no supported backend is available".into(),
        ),
        CommandRunError::Process(error) => {
            ToolError::execution("run_command", error.to_string(), true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_environment_detection_is_conservative() {
        assert!(sensitive_environment_name("OPENAI_API_KEY"));
        assert!(sensitive_environment_name("database_password"));
        assert!(!sensitive_environment_name("PATH"));
        assert!(!sensitive_environment_name("RUST_LOG"));
    }

    #[tokio::test]
    async fn runner_returns_structured_stdout_stderr_and_exit_code() {
        let temp = tempfile::tempdir().unwrap();
        let runner = LocalCommandRunner::new(temp.path()).unwrap();
        #[cfg(windows)]
        let command = "[Console]::Out.Write('out'); [Console]::Error.Write('err'); exit 7";
        #[cfg(not(windows))]
        let command = "printf out; printf err >&2; exit 7";
        let outcome = runner
            .execute(CommandRequest::new(command), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(outcome.exit_code, Some(7));
        assert!(!outcome.success);
        assert_eq!(outcome.stdout, "out");
        assert_eq!(outcome.stderr, "err");
        assert!(!outcome.timed_out);
    }

    #[tokio::test]
    async fn runner_times_out_with_bounded_partial_output() {
        let temp = tempfile::tempdir().unwrap();
        let runner = LocalCommandRunner::new(temp.path()).unwrap();
        #[cfg(windows)]
        let command = "$chunk = 'x' * 20000; [Console]::Out.Write($chunk); Start-Sleep -Seconds 10";
        #[cfg(not(windows))]
        let command = "head -c 20000 /dev/zero | tr '\\0' x; sleep 10";
        let mut request = CommandRequest::new(command);
        request.timeout_ms = 1_000;
        request.max_output_bytes = STREAM_CHUNK_BYTES;
        let outcome = runner
            .execute(request, CancellationToken::new())
            .await
            .unwrap();
        assert!(outcome.timed_out);
        assert!(outcome.stdout.len() <= STREAM_CHUNK_BYTES);
    }

    #[tokio::test]
    async fn runner_truncates_large_completed_output() {
        let temp = tempfile::tempdir().unwrap();
        let runner = LocalCommandRunner::new(temp.path()).unwrap();
        #[cfg(windows)]
        let command = "[Console]::Out.Write(('x' * 20000)); [Console]::Out.Flush()";
        #[cfg(not(windows))]
        let command = "head -c 20000 /dev/zero | tr '\\0' x";
        let mut request = CommandRequest::new(command);
        request.max_output_bytes = STREAM_CHUNK_BYTES;
        let outcome = runner
            .execute(request, CancellationToken::new())
            .await
            .unwrap();
        assert!(outcome.success);
        assert!(outcome.output_truncated);
        assert_eq!(outcome.stdout.len(), STREAM_CHUNK_BYTES);
    }

    #[tokio::test]
    async fn runner_rejects_destructive_commands_before_spawn() {
        let temp = tempfile::tempdir().unwrap();
        let runner = LocalCommandRunner::new(temp.path()).unwrap();
        let error = runner
            .execute(
                CommandRequest::new("git reset --hard"),
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, CommandRunError::Denied));
    }

    #[tokio::test]
    async fn background_manager_starts_polls_and_completes_a_real_command() {
        let temp = tempfile::tempdir().unwrap();
        let runner: Arc<dyn CommandRunner> =
            Arc::new(LocalCommandRunner::new(temp.path()).unwrap());
        let manager = BackgroundCommandManager::new(runner);
        #[cfg(windows)]
        let command = "[Console]::Out.Write('background')";
        #[cfg(not(windows))]
        let command = "printf background";
        let started = manager.start(CommandRequest::new(command)).await.unwrap();
        let completed = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let snapshot = manager.poll(started.id).await.unwrap();
                if snapshot.status.is_terminal() {
                    break snapshot;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(completed.status, BackgroundCommandStatus::Completed);
        assert_eq!(completed.outcome.unwrap().stdout, "background");
    }
}
