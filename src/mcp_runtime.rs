use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};
use tokio_util::sync::CancellationToken;

use crate::{
    FunctionTool, PermissionDecision, RawToolOutput, ToolContent, ToolDefinition, ToolError,
    ToolProvider, ToolProviderDefinition, ToolProviderKind, ToolRegistration, ToolRuntimeResult,
};

const MCP_PROTOCOL_VERSION: &str = "2025-06-18";
const MAX_MCP_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_MCP_SERVERS: usize = 32;
const MAX_MCP_TOOLS: usize = 512;
const MAX_MCP_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env_vars: Vec<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct McpConfigFile {
    version: u32,
    #[serde(default)]
    servers: Vec<McpServerConfig>,
}

#[derive(Debug, thiserror::Error)]
pub enum McpRuntimeError {
    #[error("MCP configuration is invalid: {0}")]
    Invalid(String),
    #[error("MCP I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("MCP serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("MCP server {server} returned an error: {message}")]
    Remote { server: String, message: String },
    #[error("MCP server {0} disconnected")]
    Disconnected(String),
    #[error("MCP request to {0} timed out")]
    Timeout(String),
    #[error("MCP request to {0} was cancelled")]
    Cancelled(String),
}

pub type McpRuntimeResult<T> = Result<T, McpRuntimeError>;
type PendingResponse = Result<Value, String>;

pub struct McpClient {
    server_name: String,
    writer: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<PendingResponse>>>>,
    next_id: AtomicU64,
    request_timeout: Duration,
}

impl McpClient {
    pub async fn connect(
        config: &McpServerConfig,
        workspace: &Path,
    ) -> McpRuntimeResult<Arc<Self>> {
        validate_server(config)?;
        let workspace = std::fs::canonicalize(workspace)?;
        if !workspace.is_dir() {
            return Err(McpRuntimeError::Invalid(
                "MCP workspace must be an existing directory".into(),
            ));
        }
        let mut process = Command::new(&config.command);
        let mut environment = filtered_environment();
        for name in &config.env_vars {
            if let Some(value) = std::env::var_os(name) {
                environment.insert(name.as_str().into(), value);
            }
        }
        process
            .args(&config.args)
            .current_dir(workspace)
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .envs(environment);
        let mut child = process.spawn()?;
        let writer = child.stdin.take().ok_or_else(|| {
            McpRuntimeError::Invalid("MCP server stdin pipe is unavailable".into())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            McpRuntimeError::Invalid("MCP server stdout pipe is unavailable".into())
        })?;
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(drain_stderr(stderr));
        }
        let pending = Arc::new(Mutex::new(HashMap::new()));
        tokio::spawn(read_responses(config.name.clone(), stdout, pending.clone()));
        let client = Arc::new(Self {
            server_name: config.name.clone(),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            pending,
            next_id: AtomicU64::new(1),
            request_timeout: Duration::from_millis(config.request_timeout_ms),
        });
        client.initialize().await?;
        Ok(client)
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub async fn request(
        &self,
        method: &str,
        params: Value,
        cancellation: CancellationToken,
    ) -> McpRuntimeResult<Value> {
        if method.trim().is_empty() || method.len() > 256 {
            return Err(McpRuntimeError::Invalid(
                "MCP method must contain 1..=256 bytes".into(),
            ));
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        let bytes = serde_json::to_vec(&message)?;
        if bytes.len() > MAX_MCP_MESSAGE_BYTES {
            return Err(McpRuntimeError::Invalid("MCP request exceeds 1 MiB".into()));
        }
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);
        if let Err(error) = self.write_message(&bytes).await {
            self.pending.lock().await.remove(&id);
            return Err(error);
        }
        let response = tokio::select! {
            _ = cancellation.cancelled() => {
                self.pending.lock().await.remove(&id);
                return Err(McpRuntimeError::Cancelled(self.server_name.clone()));
            },
            _ = tokio::time::sleep(self.request_timeout) => {
                self.pending.lock().await.remove(&id);
                return Err(McpRuntimeError::Timeout(self.server_name.clone()));
            },
            response = receiver => response.map_err(|_| McpRuntimeError::Disconnected(self.server_name.clone()))?,
        };
        response.map_err(|message| McpRuntimeError::Remote {
            server: self.server_name.clone(),
            message,
        })
    }

    pub async fn notify(&self, method: &str, params: Value) -> McpRuntimeResult<()> {
        let bytes = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }))?;
        self.write_message(&bytes).await
    }

    pub async fn shutdown(&self) -> McpRuntimeResult<()> {
        self.child.lock().await.kill().await?;
        Ok(())
    }

    async fn initialize(&self) -> McpRuntimeResult<()> {
        let response = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "core-agent", "version": env!("CARGO_PKG_VERSION")}
                }),
                CancellationToken::new(),
            )
            .await?;
        if response
            .get("protocolVersion")
            .and_then(Value::as_str)
            .is_none()
        {
            return Err(McpRuntimeError::Remote {
                server: self.server_name.clone(),
                message: "initialize response omitted protocolVersion".into(),
            });
        }
        self.notify("notifications/initialized", json!({})).await
    }

    async fn write_message(&self, bytes: &[u8]) -> McpRuntimeResult<()> {
        let mut writer = self.writer.lock().await;
        writer.write_all(bytes).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        Ok(())
    }
}

pub struct McpToolProvider {
    definition: ToolProviderDefinition,
    client: Arc<McpClient>,
}

impl McpToolProvider {
    pub async fn connect(config: &McpServerConfig, workspace: &Path) -> McpRuntimeResult<Self> {
        let client = McpClient::connect(config, workspace).await?;
        let key = format!("mcp-{}", safe_identity(&config.name));
        Ok(Self {
            definition: ToolProviderDefinition::new(
                key,
                format!("MCP {}", config.name),
                ToolProviderKind::Mcp,
            ),
            client,
        })
    }

    async fn list_tools(&self) -> McpRuntimeResult<Vec<Value>> {
        let mut tools = Vec::new();
        let mut cursor = None;
        for _ in 0..16 {
            let params = cursor
                .as_ref()
                .map(|cursor| json!({"cursor": cursor}))
                .unwrap_or_else(|| json!({}));
            let response = self
                .client
                .request("tools/list", params, CancellationToken::new())
                .await?;
            let page = response
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| McpRuntimeError::Remote {
                    server: self.client.server_name.clone(),
                    message: "tools/list response omitted tools".into(),
                })?;
            tools.extend(page.iter().cloned());
            if tools.len() > MAX_MCP_TOOLS {
                return Err(McpRuntimeError::Invalid(
                    "MCP server advertised more than 512 tools".into(),
                ));
            }
            cursor = response
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(tools)
    }
}

#[async_trait]
impl ToolProvider for McpToolProvider {
    fn definition(&self) -> ToolProviderDefinition {
        self.definition.clone()
    }

    async fn discover(&self) -> ToolRuntimeResult<Vec<ToolRegistration>> {
        let tools = self.list_tools().await.map_err(mcp_tool_error)?;
        let mut registrations = Vec::new();
        let mut visible_names = std::collections::BTreeSet::new();
        for advertised in tools {
            let remote_name = advertised
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| ToolError::Validation("MCP tool has no name".into()))?
                .to_owned();
            let visible_name = format!(
                "mcp_{}_{}",
                safe_identity(self.client.server_name()),
                safe_identity(&remote_name)
            );
            if !visible_names.insert(visible_name.clone()) {
                return Err(ToolError::Validation(format!(
                    "MCP tool name collision after normalization: {remote_name}"
                )));
            }
            let input_schema = advertised
                .get("inputSchema")
                .cloned()
                .unwrap_or_else(|| json!({"type": "object"}));
            let mut definition = ToolDefinition::new(
                self.definition.key.clone(),
                visible_name,
                "1.0.0",
                input_schema,
            );
            definition.description = advertised
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("Tool provided by an explicitly enabled MCP server")
                .to_owned();
            definition.category = "mcp.remote".into();
            definition.default_permission = PermissionDecision::Ask;
            definition.timeout_ms = self.client.request_timeout.as_millis() as u64 + 1_000;
            let key = definition.key.clone();
            let client = self.client.clone();
            let tool = Arc::new(FunctionTool::new(key, move |request, context| {
                let client = client.clone();
                let remote_name = remote_name.clone();
                async move {
                    let result = client
                        .request(
                            "tools/call",
                            json!({"name": remote_name, "arguments": request.parameters}),
                            context.cancellation,
                        )
                        .await
                        .map_err(mcp_tool_error)?;
                    if result.get("isError").and_then(Value::as_bool) == Some(true) {
                        return Err(ToolError::execution("mcp_tool", result.to_string(), false));
                    }
                    Ok(RawToolOutput {
                        content: vec![ToolContent::Json(result)],
                        ..RawToolOutput::default()
                    })
                }
            }));
            registrations.push(ToolRegistration::new(definition, tool));
        }
        Ok(registrations)
    }
}

pub fn discover_mcp_servers(
    workspace: &Path,
    global_directory: Option<&Path>,
) -> McpRuntimeResult<Vec<McpServerConfig>> {
    if std::env::var("CORE_AGENT_ENABLE_MCP").as_deref() != Ok("1") {
        return Ok(Vec::new());
    }
    let workspace = std::fs::canonicalize(workspace)?;
    let mut merged = BTreeMap::new();
    if let Some(directory) = global_directory {
        let path = directory.join("mcp.json");
        if path.exists() {
            for server in read_config(&path, None)? {
                merged.insert(server.name.clone(), server);
            }
        }
    }
    let project_path = workspace.join(".core-agent").join("mcp.json");
    if project_path.exists() {
        for server in read_config(&project_path, Some(&workspace))? {
            merged.insert(server.name.clone(), server);
        }
    }
    let servers = merged
        .into_values()
        .filter(|server| server.enabled)
        .collect::<Vec<_>>();
    if servers.len() > MAX_MCP_SERVERS {
        return Err(McpRuntimeError::Invalid(
            "MCP configuration exceeds 32 enabled servers".into(),
        ));
    }
    Ok(servers)
}

fn read_config(
    path: &Path,
    required_root: Option<&Path>,
) -> McpRuntimeResult<Vec<McpServerConfig>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_MCP_CONFIG_BYTES
    {
        return Err(McpRuntimeError::Invalid(format!(
            "{} must be a regular file no larger than 64 KiB",
            path.display()
        )));
    }
    let canonical = std::fs::canonicalize(path)?;
    if required_root.is_some_and(|root| !canonical.starts_with(root)) {
        return Err(McpRuntimeError::Invalid(
            "project MCP configuration escaped the workspace".into(),
        ));
    }
    let file: McpConfigFile = serde_json::from_slice(&std::fs::read(canonical)?)?;
    if file.version != 1 {
        return Err(McpRuntimeError::Invalid(
            "MCP configuration version must be 1".into(),
        ));
    }
    for server in &file.servers {
        validate_server(server)?;
    }
    Ok(file.servers)
}

fn validate_server(server: &McpServerConfig) -> McpRuntimeResult<()> {
    if server.name.trim().is_empty()
        || server.name.len() > 64
        || server.name.chars().any(char::is_control)
        || server.command.trim().is_empty()
        || server.command.len() > 4_096
        || server.command.contains('\0')
        || server.args.len() > 128
        || server
            .args
            .iter()
            .any(|arg| arg.len() > 4_096 || arg.contains('\0'))
        || server.env_vars.len() > 64
        || server.env_vars.iter().any(|name| {
            name.is_empty()
                || name.len() > 128
                || !name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        || !(1_000..=120_000).contains(&server.request_timeout_ms)
    {
        return Err(McpRuntimeError::Invalid(format!(
            "invalid MCP server configuration: {}",
            server.name
        )));
    }
    Ok(())
}

async fn read_responses(
    server: String,
    stdout: tokio::process::ChildStdout,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<PendingResponse>>>>,
) {
    let mut reader = BufReader::new(stdout);
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(_) if buffer.len() > MAX_MCP_MESSAGE_BYTES => break,
            Ok(_) => {}
        }
        let Ok(message) = serde_json::from_slice::<Value>(&buffer) else {
            continue;
        };
        let Some(id) = message.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let response = if let Some(error) = message.get("error") {
            Err(error.to_string())
        } else {
            Ok(message.get("result").cloned().unwrap_or(Value::Null))
        };
        if let Some(sender) = pending.lock().await.remove(&id) {
            let _ = sender.send(response);
        }
    }
    let mut pending = pending.lock().await;
    for (_, sender) in pending.drain() {
        let _ = sender.send(Err(format!("MCP server {server} disconnected")));
    }
}

async fn drain_stderr(mut stderr: tokio::process::ChildStderr) {
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        match stderr.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
    }
}

fn safe_identity(value: &str) -> String {
    let mut result = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    result.truncate(48);
    let result = result.trim_matches(['-', '_', '.']).to_owned();
    if result.is_empty() {
        "unnamed".into()
    } else {
        result
    }
}

fn filtered_environment() -> BTreeMap<std::ffi::OsString, std::ffi::OsString> {
    std::env::vars_os()
        .filter(|(name, _)| {
            let normalized = name.to_string_lossy().to_ascii_uppercase();
            ![
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
        })
        .collect()
}

fn mcp_tool_error(error: McpRuntimeError) -> ToolError {
    match error {
        McpRuntimeError::Invalid(message) => ToolError::Validation(message),
        McpRuntimeError::Cancelled(server) => ToolError::Cancelled(server),
        McpRuntimeError::Timeout(server) => ToolError::Timeout {
            tool: format!("mcp:{server}"),
            timeout_ms: 0,
        },
        error => ToolError::execution("mcp", error.to_string(), true),
    }
}

fn default_true() -> bool {
    true
}

fn default_request_timeout_ms() -> u64 {
    30_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_normalization_is_stable_and_bounded() {
        assert_eq!(safe_identity("GitHub Tools"), "github_tools");
        assert_eq!(safe_identity("///"), "unnamed");
        assert!(safe_identity(&"x".repeat(200)).len() <= 48);
    }

    #[test]
    fn server_validation_rejects_unbounded_or_empty_processes() {
        let invalid = McpServerConfig {
            name: "test".into(),
            command: String::new(),
            args: Vec::new(),
            env_vars: Vec::new(),
            enabled: true,
            request_timeout_ms: 30_000,
        };
        assert!(validate_server(&invalid).is_err());
    }

    #[test]
    fn project_config_requires_explicit_enablement() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(directory.path().join(".core-agent")).unwrap();
        std::fs::write(
            directory.path().join(".core-agent/mcp.json"),
            r#"{"version":1,"servers":[{"name":"x","command":"x"}]}"#,
        )
        .unwrap();
        std::env::remove_var("CORE_AGENT_ENABLE_MCP");
        assert!(discover_mcp_servers(directory.path(), None)
            .unwrap()
            .is_empty());
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn stdio_client_initializes_discovers_and_calls_a_real_process() {
        use crate::{ToolExecutionContext, ToolRequest};

        let workspace = tempfile::tempdir().unwrap();
        let script = r#"
while (($line = [Console]::In.ReadLine()) -ne $null) {
  $request = $line | ConvertFrom-Json
  if ($null -eq $request.id) { continue }
  if ($request.method -eq 'initialize') {
    $result = @{ protocolVersion = '2025-06-18'; capabilities = @{}; serverInfo = @{ name = 'fake'; version = '1' } }
  } elseif ($request.method -eq 'tools/list') {
    $result = @{ tools = @(@{ name = 'echo'; description = 'Echo'; inputSchema = @{ type = 'object'; properties = @{ message = @{ type = 'string' } } } }) }
  } elseif ($request.method -eq 'tools/call') {
    $result = @{ content = @(@{ type = 'text'; text = $request.params.arguments.message }); isError = $false }
  } else {
    $result = @{}
  }
  [Console]::Out.WriteLine((@{ jsonrpc = '2.0'; id = $request.id; result = $result } | ConvertTo-Json -Compress -Depth 12))
  [Console]::Out.Flush()
}
"#;
        let config = McpServerConfig {
            name: "fake".into(),
            command: "powershell".into(),
            args: vec![
                "-NoProfile".into(),
                "-NonInteractive".into(),
                "-Command".into(),
                script.into(),
            ],
            env_vars: Vec::new(),
            enabled: true,
            request_timeout_ms: 10_000,
        };
        let provider = McpToolProvider::connect(&config, workspace.path())
            .await
            .unwrap();
        let registrations = provider.discover().await.unwrap();
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].definition.name, "mcp_fake_echo");
        let request = ToolRequest::new(
            registrations[0].definition.key.clone(),
            json!({"message": "hello MCP"}),
        );
        let output = registrations[0]
            .tool
            .execute(
                &request,
                &ToolExecutionContext {
                    request_id: request.id,
                    cancellation: CancellationToken::new(),
                },
            )
            .await
            .unwrap();
        let ToolContent::Json(value) = &output.content[0] else {
            panic!("expected MCP JSON result")
        };
        assert!(value.to_string().contains("hello MCP"));
        provider.client.shutdown().await.unwrap();
    }
}
