use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `file.patch` — Apply multiple edits to multiple files in one call.
pub struct FilePatchTool;

#[async_trait]
impl Tool for FilePatchTool {
    fn key(&self) -> &str {
        "builtin/file.patch@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let patches = request.parameters["patches"]
            .as_array()
            .ok_or_else(|| ToolError::InvalidArgument("patches must be an array".into()))?;

        if patches.is_empty() {
            return Err(ToolError::InvalidArgument("patches must not be empty".into()));
        }
        if patches.len() > 100 {
            return Err(ToolError::InvalidArgument("patches exceeds max of 100".into()));
        }

        let mut results = Vec::with_capacity(patches.len());

        for (i, patch) in patches.iter().enumerate() {
            let path = patch["path"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArgument(format!("patches[{i}].path is required")))?;
            let old_string = patch["old_string"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArgument(format!("patches[{i}].old_string is required")))?;
            let new_string = patch["new_string"]
                .as_str()
                .ok_or_else(|| ToolError::InvalidArgument(format!("patches[{i}].new_string is required")))?;

            if path.is_empty() || old_string.is_empty() {
                return Err(ToolError::InvalidArgument(format!("patches[{i}] has empty path or old_string")));
            }

            let content = tokio::fs::read_to_string(path)
                .await
                .map_err(|e| ToolError::execution(
                    "file.patch",
                    format!("patch[{i}] failed to read {path}: {e}"),
                    false,
                ))?;

            if !content.contains(old_string) {
                return Err(ToolError::execution(
                    "file.patch",
                    format!("patch[{i}] old_string not found in {path}"),
                    false,
                ));
            }

            let new_content = content.replacen(old_string, new_string, 1);
            tokio::fs::write(path, &new_content)
                .await
                .map_err(|e| ToolError::execution("file.patch", format!("patch[{i}] write failed: {e}"), false))?;

            results.push(format!("  [{i}] {path}: 1 replacement applied"));
        }

        Ok(RawToolOutput::text(format!(
            "Applied {} patch(es):\n{}",
            results.len(),
            results.join("\n"),
        )))
    }
}

pub fn file_patch_tool() -> Arc<dyn Tool> {
    Arc::new(FilePatchTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn applies_multiple_patches() {
        let dir = tempdir().unwrap();
        let path1 = dir.path().join("a.txt");
        let path2 = dir.path().join("b.txt");
        tokio::fs::write(&path1, "Hello A").await.unwrap();
        tokio::fs::write(&path2, "Hello B").await.unwrap();

        let tool = FilePatchTool;
        let request = ToolRequest::new(
            "builtin/file.patch@1.0.0",
            serde_json::json!({
                "patches": [
                    {"path": path1.to_string_lossy(), "old_string": "A", "new_string": "World"},
                    {"path": path2.to_string_lossy(), "old_string": "B", "new_string": "World"}
                ]
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("2 patch(es)"));

        assert_eq!(tokio::fs::read_to_string(&path1).await.unwrap(), "Hello World");
        assert_eq!(tokio::fs::read_to_string(&path2).await.unwrap(), "Hello World");
    }

    #[tokio::test]
    async fn rejects_empty_patches() {
        let tool = FilePatchTool;
        let request = ToolRequest::new(
            "builtin/file.patch@1.0.0",
            serde_json::json!({"patches": []}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rejects_too_many_patches() {
        let tool = FilePatchTool;
        let patches: Vec<serde_json::Value> = (0..101).map(|i| {
            serde_json::json!({"path": format!("/tmp/{i}"), "old_string": "a", "new_string": "b"})
        }).collect();
        let request = ToolRequest::new(
            "builtin/file.patch@1.0.0",
            serde_json::json!({"patches": patches}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}