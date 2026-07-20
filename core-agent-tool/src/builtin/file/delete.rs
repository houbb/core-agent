use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `file.delete` — Delete a file or empty directory.
pub struct FileDeleteTool;

#[async_trait]
impl Tool for FileDeleteTool {
    fn key(&self) -> &str {
        "builtin/file.delete@1.0.0"
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

        let meta = tokio::fs::metadata(path)
            .await
            .map_err(|e| ToolError::execution("file.delete", format!("{e}"), false))?;

        if meta.is_dir() {
            tokio::fs::remove_dir(path)
                .await
                .map_err(|e| ToolError::execution("file.delete", format!("failed to remove directory: {e}"), false))?;
            Ok(RawToolOutput::text(format!("Deleted directory: {path}")))
        } else {
            tokio::fs::remove_file(path)
                .await
                .map_err(|e| ToolError::execution("file.delete", format!("{e}"), false))?;
            Ok(RawToolOutput::text(format!("Deleted file: {path}")))
        }
    }
}

pub fn file_delete_tool() -> Arc<dyn Tool> {
    Arc::new(FileDeleteTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn deletes_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("delete.txt");
        tokio::fs::write(&path, "content").await.unwrap();

        let tool = FileDeleteTool;
        let request = ToolRequest::new(
            "builtin/file.delete@1.0.0",
            serde_json::json!({"path": path.to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Deleted"));

        assert!(!path.exists());
    }

    #[tokio::test]
    async fn errors_on_missing_file() {
        let tool = FileDeleteTool;
        let request = ToolRequest::new(
            "builtin/file.delete@1.0.0",
            serde_json::json!({"path": "/tmp/nonexistent_file_delete_test"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}