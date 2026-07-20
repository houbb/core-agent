use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `file.list` — List the contents of a directory.
pub struct FileListTool;

#[async_trait]
impl Tool for FileListTool {
    fn key(&self) -> &str {
        "builtin/file.list@1.0.0"
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

        let include_hidden = request.parameters["include_hidden"].as_bool().unwrap_or(false);

        let mut read_dir = tokio::fs::read_dir(path)
            .await
            .map_err(|e| ToolError::execution("file.list", format!("{e}"), false))?;

        let mut entries = Vec::new();
        let mut total_size = 0u64;

        while let Ok(Some(entry)) = read_dir.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if !include_hidden && name.starts_with('.') {
                continue;
            }

            let metadata = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };

            let file_type = if metadata.is_dir() { "dir" } else { "file" };
            let size = metadata.len();
            total_size += size;

            entries.push(format!("  {file_type:4} {size:>8} {name}"));
        }

        entries.sort();

        Ok(RawToolOutput::text(format!(
            "Directory: {path}\nTotal: {} entries ({total_size} bytes)\n---\n{}",
            entries.len(),
            entries.join("\n"),
        )))
    }
}

pub fn file_list_tool() -> Arc<dyn Tool> {
    Arc::new(FileListTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn lists_directory_contents() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.txt"), "aaa").await.unwrap();
        tokio::fs::write(dir.path().join("b.txt"), "bbb").await.unwrap();
        tokio::fs::create_dir(dir.path().join("subdir")).await.unwrap();

        let tool = FileListTool;
        let request = ToolRequest::new(
            "builtin/file.list@1.0.0",
            serde_json::json!({"path": dir.path().to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("a.txt"));
        assert!(text.contains("b.txt"));
        assert!(text.contains("subdir"));
        assert!(text.contains("3 entries"));
    }

    #[tokio::test]
    async fn errors_on_missing_directory() {
        let tool = FileListTool;
        let request = ToolRequest::new(
            "builtin/file.list@1.0.0",
            serde_json::json!({"path": "/tmp/nonexistent_dir_list_test"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}