use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `file.copy` — Copy a file or directory.
pub struct FileCopyTool;

#[async_trait]
impl Tool for FileCopyTool {
    fn key(&self) -> &str {
        "builtin/file.copy@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let source = request.parameters["source"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("source is required".into()))?;
        let dest = request.parameters["dest"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("dest is required".into()))?;

        if source.is_empty() || dest.is_empty() {
            return Err(ToolError::InvalidArgument("source and dest must not be empty".into()));
        }

        // Create parent directories for destination
        if let Some(parent) = std::path::Path::new(dest).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| ToolError::execution("file.copy", format!("failed to create parent dirs: {e}"), false))?;
        }

        tokio::fs::copy(source, dest)
            .await
            .map_err(|e| ToolError::execution("file.copy", format!("{e}"), false))?;

        Ok(RawToolOutput::text(format!("Copied {source} → {dest}")))
    }
}

pub fn file_copy_tool() -> Arc<dyn Tool> {
    Arc::new(FileCopyTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn copies_file() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("original.txt");
        let dst = dir.path().join("copy.txt");
        tokio::fs::write(&src, "content").await.unwrap();

        let tool = FileCopyTool;
        let request = ToolRequest::new(
            "builtin/file.copy@1.0.0",
            serde_json::json!({"source": src.to_string_lossy(), "dest": dst.to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Copied"));

        assert!(src.exists());
        assert!(dst.exists());
        assert_eq!(tokio::fs::read_to_string(&dst).await.unwrap(), "content");
    }
}