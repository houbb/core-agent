use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `file.glob` — Find files matching a glob pattern.
pub struct FileGlobTool;

#[async_trait]
impl Tool for FileGlobTool {
    fn key(&self) -> &str {
        "builtin/file.glob@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let pattern = request.parameters["pattern"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("pattern is required".into()))?;
        if pattern.is_empty() {
            return Err(ToolError::InvalidArgument("pattern must not be empty".into()));
        }

        let base_dir = request.parameters["path"]
            .as_str()
            .and_then(|p| if p.is_empty() { None } else { Some(p) })
            .unwrap_or(".");

        let glob_pattern = if pattern.starts_with("**") || pattern.starts_with('*') {
            format!("{base_dir}/{pattern}")
        } else {
            pattern.to_string()
        };

        let glob_path = std::path::Path::new(&glob_pattern);
        let base_path = std::path::Path::new(base_dir);

        let mut entries = Vec::new();
        let glob_iter = glob::glob(&glob_pattern)
            .map_err(|e| ToolError::execution("file.glob", format!("invalid glob pattern: {e}"), false))?;

        for entry in glob_iter {
            match entry {
                Ok(path) => {
                    // Normalize to relative path if possible
                    let display = if let Ok(relative) = path.strip_prefix(base_path) {
                        relative.to_string_lossy().into_owned()
                    } else {
                        path.to_string_lossy().into_owned()
                    };
                    entries.push(display);
                }
                Err(e) => {
                    return Err(ToolError::execution("file.glob", format!("glob error: {e}"), false));
                }
            }
        }

        entries.sort();

        if entries.is_empty() {
            return Ok(RawToolOutput::text("No files matched the pattern."));
        }

        Ok(RawToolOutput::text(format!(
            "Found {} file(s):\n{}",
            entries.len(),
            entries.join("\n"),
        )))
    }
}

pub fn file_glob_tool() -> Arc<dyn Tool> {
    Arc::new(FileGlobTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn finds_files_by_pattern() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.rs"), "").await.unwrap();
        tokio::fs::write(dir.path().join("b.rs"), "").await.unwrap();
        tokio::fs::write(dir.path().join("c.md"), "").await.unwrap();

        let tool = FileGlobTool;
        let pattern = format!("{}/*.rs", dir.path().to_string_lossy());
        let request = ToolRequest::new(
            "builtin/file.glob@1.0.0",
            serde_json::json!({"pattern": pattern}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Found 2 file(s)"));
        assert!(text.contains("a.rs"));
        assert!(text.contains("b.rs"));
    }

    #[tokio::test]
    async fn reports_no_matches() {
        let dir = tempdir().unwrap();
        let tool = FileGlobTool;
        let pattern = format!("{}/*.nonexistent", dir.path().to_string_lossy());
        let request = ToolRequest::new(
            "builtin/file.glob@1.0.0",
            serde_json::json!({"pattern": pattern}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("No files matched"));
    }
}