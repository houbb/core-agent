use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `lsp.completion` — Get code completion suggestions.
///
/// Provides context-aware completion suggestions based on prefix.
/// Uses grep-based search to find matching symbols in the project.
pub struct LspCompletionTool;

#[async_trait]
impl Tool for LspCompletionTool {
    fn key(&self) -> &str { "builtin/lsp.completion@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let prefix = request.parameters["prefix"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("prefix is required".into()))?;
        if prefix.is_empty() || prefix.len() < 2 {
            return Err(ToolError::InvalidArgument("prefix must be at least 2 characters".into()));
        }

        let path = request.parameters["path"].as_str().filter(|p| !p.is_empty()).unwrap_or(".");
        let limit = request.parameters["limit"].as_u64().unwrap_or(20) as usize;

        let search_dir = std::path::Path::new(path);
        if !search_dir.is_dir() {
            return Err(ToolError::InvalidArgument(format!("{path} is not a directory")));
        }

        // Find all identifiers matching the prefix
        let pattern = format!(r"\b{prefix}[a-zA-Z_]\w*\b");
        let re = regex::Regex::new(&pattern)
            .map_err(|e| ToolError::InvalidArgument(format!("invalid prefix: {e}")))?;

        let mut suggestions = std::collections::BTreeSet::new();

        let walk_pattern = format!("{}/**/*", path.trim_end_matches('/'));
        let glob_iter = glob::glob(&walk_pattern)
            .map_err(|e| ToolError::execution("lsp.completion", format!("glob error: {e}"), false))?;

        for entry in glob_iter.flatten() {
            if !entry.is_file() { continue; }
            let ext = entry.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ["png","jpg","jpeg","gif","class","jar","pyc","node","lock","svg","woff","woff2"].contains(&ext) {
                continue;
            }

            if let Ok(content) = tokio::fs::read_to_string(&entry).await {
                for m in re.find_iter(&content) {
                    suggestions.insert(m.as_str().to_string());
                    if suggestions.len() >= limit {
                        break;
                    }
                }
            }
            if suggestions.len() >= limit {
                break;
            }
        }

        if suggestions.is_empty() {
            return Ok(RawToolOutput::text(format!("No completions found for '{prefix}'.")));
        }

        Ok(RawToolOutput::text(format!(
            "Completions for '{prefix}' ({}):\n{}",
            suggestions.len(),
            suggestions.iter().take(limit).map(|s| format!("  {s}")).collect::<Vec<_>>().join("\n"),
        )))
    }
}

pub fn lsp_completion_tool() -> Arc<dyn Tool> { Arc::new(LspCompletionTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn provides_completions() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("main.rs"),
            "let user_name = \"Alice\";\nlet user_email = \"alice@test.com\";"
        ).await.unwrap();

        let result = LspCompletionTool.execute(&ToolRequest::new(
            "builtin/lsp.completion@1.0.0",
            serde_json::json!({"prefix": "user_", "path": dir.path().to_string_lossy()}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("user_name"));
        assert!(text.contains("user_email"));
    }
}