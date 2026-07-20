use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `file.write` — Create or overwrite a file.
pub struct FileWriteTool;

#[async_trait]
impl Tool for FileWriteTool {
    fn key(&self) -> &str {
        "builtin/file.write@1.0.0"
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

        let content = request.parameters["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("content is required".into()))?;

        // Create parent directories if needed
        if let Some(parent) = std::path::Path::new(path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ToolError::execution("file.write", format!("failed to create parent dirs: {e}"), false))?;
        }

        tokio::fs::write(path, content)
            .await
            .map_err(|e| ToolError::execution("file.write", format!("{e}"), false))?;

        Ok(RawToolOutput::text(format!(
            "Written {} bytes to {}",
            content.len(),
            path
        )))
    }
}

pub fn file_write_tool() -> Arc<dyn Tool> {
    Arc::new(FileWriteTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn writes_file_content() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("output.txt");

        let tool = FileWriteTool;
        let request = ToolRequest::new(
            "builtin/file.write@1.0.0",
            serde_json::json!({"path": path.to_string_lossy(), "content": "hello world"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Written"));
        assert!(text.contains("11 bytes"));

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn creates_parent_directories() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("nested/deep/file.txt");

        let tool = FileWriteTool;
        let request = ToolRequest::new(
            "builtin/file.write@1.0.0",
            serde_json::json!({"path": path.to_string_lossy(), "content": "nested content"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_ok());

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "nested content");
    }

    #[tokio::test]
    async fn rejects_empty_path() {
        let tool = FileWriteTool;
        let request = ToolRequest::new(
            "builtin/file.write@1.0.0",
            serde_json::json!({"path": "", "content": "data"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}