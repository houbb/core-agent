use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `file.info` — Get metadata about a file or directory.
pub struct FileInfoTool;

#[async_trait]
impl Tool for FileInfoTool {
    fn key(&self) -> &str {
        "builtin/file.info@1.0.0"
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

        let metadata = tokio::fs::metadata(path)
            .await
            .map_err(|e| ToolError::execution("file.info", format!("{e}"), false))?;

        let file_type = if metadata.is_dir() { "directory" } else { "file" };
        let size = metadata.len();
        let modified = metadata
            .modified()
            .ok()
            .map(|t| {
                let duration = t
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                let secs = duration.as_secs();
                chrono::DateTime::from_timestamp(secs as i64, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "unknown".into())
            })
            .unwrap_or_else(|| "unknown".into());
        #[cfg(unix)]
        let permissions = {
            use std::os::unix::fs::PermissionsExt;
            format!("{:o}", metadata.permissions().mode() & 0o777)
        };
        #[cfg(not(unix))]
        let permissions = {
            if metadata.permissions().readonly() {
                "r--r--r--"
            } else {
                "rw-rw-rw-"
            }
            .to_string()
        };

        Ok(RawToolOutput::text(format!(
            "Path: {path}\nType: {file_type}\nSize: {size} bytes\nModified: {modified}\nPermissions: {permissions}"
        )))
    }
}

pub fn file_info_tool() -> Arc<dyn Tool> {
    Arc::new(FileInfoTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn returns_file_info() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("info.txt");
        tokio::fs::write(&path, "content").await.unwrap();

        let tool = FileInfoTool;
        let request = ToolRequest::new(
            "builtin/file.info@1.0.0",
            serde_json::json!({"path": path.to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Type: file"));
        assert!(text.contains("Size: 7 bytes"));
    }
}