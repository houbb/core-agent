use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `shell.script` — Execute a script file. (Default: Deny for security)
pub struct ShellScriptTool;

#[async_trait]
impl Tool for ShellScriptTool {
    fn key(&self) -> &str {
        "builtin/shell.script@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let path = request.parameters["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("path is required".into()))?;
        if path.is_empty() {
            return Err(ToolError::InvalidArgument("path must not be empty".into()));
        }

        let script = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ToolError::execution("shell.script", format!("failed to read script: {e}"), false))?;

        let shell = if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "sh"
        };
        let flag = if cfg!(target_os = "windows") { "/C" } else { "-c" };

        let output = tokio::process::Command::new(shell)
            .arg(flag)
            .arg(&script)
            .output()
            .await
            .map_err(|e| ToolError::execution("shell.script", format!("failed to execute: {e}"), true))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let exit_code = output.status.code().unwrap_or(-1);

        let result = if exit_code == 0 {
            stdout
        } else {
            format!("Exit code: {exit_code}\nstdout:\n{stdout}\nstderr:\n{stderr}")
        };

        Ok(RawToolOutput::text(result.trim_end().to_string()))
    }
}

pub fn shell_script_tool() -> Arc<dyn Tool> {
    Arc::new(ShellScriptTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn executes_script_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sh");
        tokio::fs::write(&path, "echo script_executed").await.unwrap();

        let tool = ShellScriptTool;
        let request = ToolRequest::new(
            "builtin/shell.script@1.0.0",
            serde_json::json!({"path": path.to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("script_executed"));
    }
}