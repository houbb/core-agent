use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `shell.exec` — Execute a shell command and return its output.
pub struct ShellExecTool;

#[async_trait]
impl Tool for ShellExecTool {
    fn key(&self) -> &str {
        "builtin/shell.exec@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let command = request.parameters["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("command is required".into()))?;
        if command.is_empty() {
            return Err(ToolError::InvalidArgument("command must not be empty".into()));
        }

        let working_dir = request.parameters["working_dir"].as_str();
        let timeout_ms = request.parameters["timeout_ms"].as_u64().unwrap_or(60_000);

        let shell = if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "sh"
        };
        let flag = if cfg!(target_os = "windows") { "/C" } else { "-c" };

        let mut child = tokio::process::Command::new(shell);
        child.arg(flag).arg(command);

        if let Some(dir) = working_dir {
            child.current_dir(dir);
        }

        // Collect env vars
        if let Some(env) = request.parameters["env"].as_object() {
            for (key, value) in env {
                if let Some(val) = value.as_str() {
                    child.env(key, val);
                }
            }
        }

        let output = child
            .output()
            .await
            .map_err(|e| ToolError::execution("shell.exec", format!("failed to execute: {e}"), true))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        let result = if exit_code == 0 {
            if stderr.is_empty() {
                stdout
            } else {
                format!("{stdout}\n{stderr}")
            }
        } else {
            format!(
                "Exit code: {exit_code}\nstdout:\n{stdout}\nstderr:\n{stderr}"
            )
        };

        // Trim trailing newline for cleaner output
        let result = result.trim_end().to_string();

        Ok(RawToolOutput::text(result))
    }
}

pub fn shell_exec_tool() -> Arc<dyn Tool> {
    Arc::new(ShellExecTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn executes_echo_command() {
        let tool = ShellExecTool;
        let request = ToolRequest::new(
            "builtin/shell.exec@1.0.0",
            serde_json::json!({"command": "echo hello_world"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("hello_world"));
    }

    #[tokio::test]
    async fn executes_with_working_dir() {
        let tool = ShellExecTool;
        let request = ToolRequest::new(
            "builtin/shell.exec@1.0.0",
            serde_json::json!({"command": "pwd", "working_dir": "/"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(!text.is_empty());
    }

    #[tokio::test]
    async fn captures_exit_code_on_failure() {
        let tool = ShellExecTool;
        let request = ToolRequest::new(
            "builtin/shell.exec@1.0.0",
            serde_json::json!({"command": "exit 42"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Exit code: 42"));
    }

    #[tokio::test]
    async fn rejects_empty_command() {
        let tool = ShellExecTool;
        let request = ToolRequest::new(
            "builtin/shell.exec@1.0.0",
            serde_json::json!({"command": ""}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}