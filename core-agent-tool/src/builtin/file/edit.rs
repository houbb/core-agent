use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `file.edit` — Replace exact text in a file (old_string → new_string).
pub struct FileEditTool;

#[async_trait]
impl Tool for FileEditTool {
    fn key(&self) -> &str {
        "builtin/file.edit@1.0.0"
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

        let old_string = request.parameters["old_string"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("old_string is required".into()))?;
        if old_string.is_empty() {
            return Err(ToolError::InvalidArgument("old_string must not be empty".into()));
        }

        let new_string = request.parameters["new_string"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("new_string is required".into()))?;

        let replace_all = request.parameters["replace_all"].as_bool().unwrap_or(false);

        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ToolError::execution("file.edit", format!("failed to read: {e}"), false))?;

        let new_content = if replace_all {
            content.replace(old_string, new_string)
        } else {
            if !content.contains(old_string) {
                return Err(ToolError::execution(
                    "file.edit",
                    format!("old_string not found in {path}"),
                    false,
                ));
            }
            content.replacen(old_string, new_string, 1)
        };

        if new_content == content {
            return Err(ToolError::execution(
                "file.edit",
                String::from("no changes made — old_string not found"),
                false,
            ));
        }

        tokio::fs::write(path, &new_content)
            .await
            .map_err(|e| ToolError::execution("file.edit", format!("failed to write: {e}"), false))?;

        let diff_count = if replace_all {
            content.matches(old_string).count()
        } else {
            1
        };

        Ok(RawToolOutput::text(format!(
            "Applied {diff_count} replacement(s) in {path}"
        )))
    }
}

pub fn file_edit_tool() -> Arc<dyn Tool> {
    Arc::new(FileEditTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn replaces_text_in_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("edit.txt");
        tokio::fs::write(&path, "Hello {name}").await.unwrap();

        let tool = FileEditTool;
        let request = ToolRequest::new(
            "builtin/file.edit@1.0.0",
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "{name}",
                "new_string": "World"
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("1 replacement(s)"));

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "Hello World");
    }

    #[tokio::test]
    async fn replace_all_works() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("replace_all.txt");
        tokio::fs::write(&path, "a b a b a").await.unwrap();

        let tool = FileEditTool;
        let request = ToolRequest::new(
            "builtin/file.edit@1.0.0",
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "a",
                "new_string": "x",
                "replace_all": true
            }),
        );
        tool.execute(&request, &ToolContext::default()).await.unwrap();

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "x b x b x");
    }

    #[tokio::test]
    async fn errors_when_old_string_not_found() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.txt");
        tokio::fs::write(&path, "original content").await.unwrap();

        let tool = FileEditTool;
        let request = ToolRequest::new(
            "builtin/file.edit@1.0.0",
            serde_json::json!({
                "path": path.to_string_lossy(),
                "old_string": "NONEXISTENT",
                "new_string": "x"
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}