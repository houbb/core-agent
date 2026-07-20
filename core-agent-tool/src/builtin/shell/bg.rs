use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `shell.bg` — Execute a command in the background.
pub struct ShellBgTool;

#[async_trait]
impl Tool for ShellBgTool {
    fn key(&self) -> &str {
        "builtin/shell.bg@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let command = request.parameters["command"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("command is required".into()))?;
        if command.is_empty() {
            return Err(ToolError::InvalidArgument("command must not be empty".into()));
        }

        let shell = if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "sh"
        };
        let flag = if cfg!(target_os = "windows") { "/C" } else { "-c" };

        let mut child = tokio::process::Command::new(shell);
        child.arg(flag).arg(command);

        if let Some(dir) = request.parameters["working_dir"].as_str() {
            child.current_dir(dir);
        }

        // Spawn in background
        match child.spawn() {
            Ok(_) => Ok(RawToolOutput::text(format!(
                "Background command started: {command}"
            ))),
            Err(e) => Err(ToolError::execution("shell.bg", format!("{e}"), true)),
        }
    }
}

pub fn shell_bg_tool() -> Arc<dyn Tool> {
    Arc::new(ShellBgTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn starts_background_command() {
        let tool = ShellBgTool;
        let request = ToolRequest::new(
            "builtin/shell.bg@1.0.0",
            serde_json::json!({"command": "echo bg_test"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Background command started"));
    }
}