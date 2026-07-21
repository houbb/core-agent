//! MCP Client — stdio-based JSON-RPC transport for MCP servers.
//!
//! Manages a child process, sends JSON-RPC requests, and routes responses
//! back to the caller via a pending-request map.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};
use tokio_util::sync::CancellationToken;

use crate::constants::{
    MCP_PROTOCOL_VERSION, MAX_MCP_MESSAGE_BYTES, MAX_MCP_TOOLS,
};
use crate::error::{McpRuntimeError, McpRuntimeResult};
use crate::McpServerConfig;

type PendingResponse = Result<Value, String>;

/// A connected MCP server client over stdio JSON-RPC.
pub struct McpClient {
    server_name: String,
    writer: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<PendingResponse>>>>,
    next_id: AtomicU64,
    request_timeout: std::time::Duration,
}

impl McpClient {
    /// Connect to an MCP server by spawning its process.
    pub async fn connect(
        config: &McpServerConfig,
        workspace: &Path,
    ) -> McpRuntimeResult<Arc<Self>> {
        config.validate()?;
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
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
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
        tokio::spawn(read_responses(
            config.name.clone(),
            stdout,
            pending.clone(),
        ));
        let client = Arc::new(Self {
            server_name: config.name.clone(),
            writer: Mutex::new(writer),
            child: Mutex::new(child),
            pending,
            next_id: AtomicU64::new(1),
            request_timeout: std::time::Duration::from_millis(config.request_timeout_ms),
        });
        client.initialize().await?;
        Ok(client)
    }

    /// The server name this client is connected to.
    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    /// The request timeout in milliseconds.
    pub fn request_timeout_ms(&self) -> u64 {
        self.request_timeout.as_millis() as u64
    }

    /// Send a JSON-RPC request and wait for the response.
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

    /// Send a JSON-RPC notification (no response expected).
    pub async fn notify(&self, method: &str, params: Value) -> McpRuntimeResult<()> {
        let bytes = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        }))?;
        self.write_message(&bytes).await
    }

    /// Shut down the MCP server process.
    pub async fn shutdown(&self) -> McpRuntimeResult<()> {
        self.child.lock().await.kill().await?;
        Ok(())
    }

    /// List available tools from the server (paginated).
    pub(crate) async fn list_tools(&self) -> McpRuntimeResult<Vec<Value>> {
        let mut tools = Vec::new();
        let mut cursor = None;
        for _ in 0..16 {
            let params = cursor
                .as_ref()
                .map(|cursor| json!({"cursor": cursor}))
                .unwrap_or_else(|| json!({}));
            let response = self
                .request("tools/list", params, CancellationToken::new())
                .await?;
            let page = response
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| McpRuntimeError::Remote {
                    server: self.server_name.clone(),
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

    async fn initialize(&self) -> McpRuntimeResult<()> {
        let response = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {"name": "core-agent-mcp", "version": "0.1.0"}
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

fn filtered_environment() -> std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString> {
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

/// Normalize a string to a safe identifier (lowercase alphanumeric, max 48 chars).
pub(crate) fn safe_identity(value: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_normalization_is_stable_and_bounded() {
        assert_eq!(safe_identity("GitHub Tools"), "github_tools");
        assert_eq!(safe_identity("///"), "unnamed");
        assert!(safe_identity(&"x".repeat(200)).len() <= 48);
    }
}