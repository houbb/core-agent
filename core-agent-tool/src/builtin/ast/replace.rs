use std::sync::Arc;

use async_trait::async_trait;
use regex::Regex;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

use super::search::language_extensions;

/// `ast.replace` — Replace code patterns using AST-aware matching.
///
/// Finds all matches of a regex pattern and replaces them with new text.
/// Supports language filtering and optional dry-run mode.
pub struct AstReplaceTool;

#[async_trait]
impl Tool for AstReplaceTool {
    fn key(&self) -> &str {
        "builtin/ast.replace@1.0.0"
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

        let rewrite = request.parameters["rewrite"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("rewrite is required".into()))?;

        let language = request.parameters["language"].as_str().unwrap_or("all");
        let search_path = request.parameters["path"]
            .as_str()
            .filter(|p| !p.is_empty())
            .unwrap_or(".");
        let dry_run = request.parameters["dry_run"].as_bool().unwrap_or(false);

        let regex = Regex::new(pattern)
            .map_err(|e| ToolError::InvalidArgument(format!("invalid regex pattern: {e}")))?;

        let extensions = language_extensions(language);

        let search_dir = std::path::Path::new(search_path);
        if !search_dir.is_dir() {
            return Err(ToolError::InvalidArgument(format!("{search_path} is not a directory")));
        }

        let mut replacements = Vec::new();
        let mut total_replacements = 0u64;
        let mut file_count = 0u64;

        let walk_pattern = format!("{}/**/*", search_path.trim_end_matches('/'));
        let glob_iter = glob::glob(&walk_pattern)
            .map_err(|e| ToolError::execution("ast.replace", format!("glob error: {e}"), false))?;

        for entry in glob_iter.flatten() {
            if !entry.is_file() {
                continue;
            }

            // Language filter
            if let Some(exts) = &extensions {
                if !exts.iter().any(|ext| {
                    entry.extension().and_then(|e| e.to_str()) == Some(*ext)
                }) {
                    continue;
                }
            }

            // Skip binary files
            if is_binary(&entry) {
                continue;
            }

            let content = match tokio::fs::read_to_string(&entry).await {
                Ok(c) => c,
                Err(_) => continue,
            };

            let new_content = regex.replace_all(&content, rewrite).to_string();
            if new_content != content {
                let count = regex.find_iter(&content).count();
                total_replacements += count as u64;
                file_count += 1;

                let display_path = entry.to_string_lossy();

                if dry_run {
                    replacements.push(format!("  [DRY RUN] {display_path}: {count} replacement(s)"));
                } else {
                    tokio::fs::write(&entry, &new_content)
                        .await
                        .map_err(|e| ToolError::execution("ast.replace", format!("write failed: {e}"), false))?;
                    replacements.push(format!("  {display_path}: {count} replacement(s) applied"));
                }
            }
        }

        if replacements.is_empty() {
            return Ok(RawToolOutput::text("No matches found for replacement."));
        }

        let mode = if dry_run { " (dry run)" } else { "" };
        Ok(RawToolOutput::text(format!(
            "Found {total_replacements} replacement(s) in {file_count} file(s){mode}:\n{}",
            replacements.join("\n"),
        )))
    }
}

fn is_binary(path: &std::path::Path) -> bool {
    let extensions = ["png", "jpg", "jpeg", "gif", "bmp", "ico", "woff", "woff2",
                      "ttf", "eot", "otf", "pdf", "zip", "tar", "gz", "bz2",
                      "exe", "dll", "so", "dylib", "class", "jar", "pyc", "pyo",
                      "o", "a", "lib", "obj", "pdb", "idb", "pch", "pcm",
                      "node", "map", "lock", "svg"];
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| extensions.contains(&e))
        .unwrap_or(false)
}

pub fn ast_replace_tool() -> Arc<dyn Tool> {
    Arc::new(AstReplaceTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn replaces_code_pattern() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("app.rs");
        tokio::fs::write(&path, "let old_name = 42;").await.unwrap();

        let tool = AstReplaceTool;
        let request = ToolRequest::new(
            "builtin/ast.replace@1.0.0",
            serde_json::json!({
                "pattern": "old_name",
                "rewrite": "new_name",
                "path": dir.path().to_string_lossy(),
                "language": "rust"
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("1 replacement(s)"));

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "let new_name = 42;");
    }

    #[tokio::test]
    async fn dry_run_does_not_modify_files() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.rs");
        tokio::fs::write(&path, "let x = 1;").await.unwrap();

        let tool = AstReplaceTool;
        let request = ToolRequest::new(
            "builtin/ast.replace@1.0.0",
            serde_json::json!({
                "pattern": "x",
                "rewrite": "y",
                "path": dir.path().to_string_lossy(),
                "language": "rust",
                "dry_run": true
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("DRY RUN"));

        let content = tokio::fs::read_to_string(&path).await.unwrap();
        assert_eq!(content, "let x = 1;");
    }
}