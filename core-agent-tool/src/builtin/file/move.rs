use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `file.move` — Move or rename a file or directory.
pub struct FileMoveTool;

#[async_trait]
impl Tool for FileMoveTool {
    fn key(&self) -> &str {
        "builtin/file.move@1.0.0"
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
                .map_err(|e| ToolError::execution("file.move", format!("failed to create parent dirs: {e}"), false))?;
        }

        tokio::fs::rename(source, dest)
            .await
            .map_err(|e| ToolError::execution("file.move", format!("{e}"), false))?;

        Ok(RawToolOutput::text(format!("Moved {source} → {dest}")))
    }
}

pub fn file_move_tool() -> Arc<dyn Tool> {
    Arc::new(FileMoveTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn moves_file() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("source.txt");
        let dst = dir.path().join("dest.txt");
        tokio::fs::write(&src, "content").await.unwrap();

        let tool = FileMoveTool;
        let request = ToolRequest::new(
            "builtin/file.move@1.0.0",
            serde_json::json!({"source": src.to_string_lossy(), "dest": dst.to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Moved"));
        assert!(!src.exists());
        assert!(dst.exists());
    }
}