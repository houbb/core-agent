use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolContent, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `file.read` — Read the content of a file at the given path.
pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn key(&self) -> &str {
        "builtin/file.read@1.0.0"
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

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ToolError::execution("file.read", format!("{e}"), false))?;

        let limit = request.parameters["limit"].as_u64();
        let offset = request.parameters["offset"].as_u64().unwrap_or(0);

        let result = if let Some(limit) = limit {
            let lines: Vec<&str> = content.lines().skip(offset as usize).take(limit as usize).collect();
            lines.join("\n")
        } else if offset > 0 {
            let lines: Vec<&str> = content.lines().skip(offset as usize).collect();
            lines.join("\n")
        } else {
            content
        };

        Ok(RawToolOutput::text(result))
    }
}

pub fn file_read_tool() -> Arc<dyn Tool> {
    Arc::new(FileReadTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn reads_full_file_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");
        tokio::fs::write(&path, "hello world").await.unwrap();

        let tool = FileReadTool;
        let request = ToolRequest::new(
            "builtin/file.read@1.0.0",
            serde_json::json!({"path": path.to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        assert_eq!(
            result.content[0],
            crate::domain::ToolContent::Text("hello world".into())
        );
    }

    #[tokio::test]
    async fn reads_with_line_limit() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("lines.txt");
        tokio::fs::write(&path, "line1\nline2\nline3\nline4\nline5").await.unwrap();

        let tool = FileReadTool;
        let request = ToolRequest::new(
            "builtin/file.read@1.0.0",
            serde_json::json!({"path": path.to_string_lossy(), "limit": 3}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "line1");
    }

    #[tokio::test]
    async fn rejects_empty_path() {
        let tool = FileReadTool;
        let request = ToolRequest::new(
            "builtin/file.read@1.0.0",
            serde_json::json!({"path": ""}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rejects_missing_file() {
        let tool = FileReadTool;
        let request = ToolRequest::new(
            "builtin/file.read@1.0.0",
            serde_json::json!({"path": "/tmp/nonexistent_file_for_test_12345"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}