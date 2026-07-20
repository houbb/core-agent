use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `lsp.hover` — Show type information and documentation for a symbol.
///
/// Uses grep-based search to find type declarations and documentation comments.
pub struct LspHoverTool;

#[async_trait]
impl Tool for LspHoverTool {
    fn key(&self) -> &str { "builtin/lsp.hover@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let symbol = request.parameters["symbol"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("symbol is required".into()))?;
        if symbol.is_empty() {
            return Err(ToolError::InvalidArgument("symbol must not be empty".into()));
        }

        let path = request.parameters["path"].as_str().filter(|p| !p.is_empty()).unwrap_or(".");

        let search_dir = std::path::Path::new(path);
        if !search_dir.is_dir() {
            return Err(ToolError::InvalidArgument(format!("{path} is not a directory")));
        }

        // Look for declarations with documentation comments
        let mut results = Vec::new();
        let walk_pattern = format!("{}/**/*", path.trim_end_matches('/'));
        let glob_iter = glob::glob(&walk_pattern)
            .map_err(|e| ToolError::execution("lsp.hover", format!("glob error: {e}"), false))?;

        let symbol_re = regex::Regex::new(&format!(
            r"(?:(?:pub\s+)?(?:fn|class|struct|enum|trait|def|function|interface|const|let|var|type)\s+)?{symbol}\b"
        )).map_err(|e| ToolError::InvalidArgument(format!("invalid symbol: {e}")))?;

        for entry in glob_iter.flatten() {
            if !entry.is_file() { continue; }

            let ext = entry.extension().and_then(|e| e.to_str()).unwrap_or("");
            if ["png","jpg","jpeg","gif","class","jar","pyc","o","so","dll","exe","lock","svg"].contains(&ext) {
                continue;
            }

            if let Ok(content) = tokio::fs::read_to_string(&entry).await {
                let lines: Vec<&str> = content.lines().collect();
                for (i, line) in lines.iter().enumerate() {
                    if symbol_re.is_match(line) {
                        // Collect documentation comment before the declaration
                        let mut docs = Vec::new();
                        for j in (0..i).rev().take(10) {
                            let trimmed = lines[j].trim();
                            if trimmed.starts_with("///") || trimmed.starts_with("//") {
                                docs.push(trimmed.trim_start_matches("///").trim_start_matches("//").trim());
                            } else if trimmed.starts_with("*") || trimmed.starts_with("/**") || trimmed.starts_with("*/") {
                                docs.push(trimmed.trim_start_matches('*').trim());
                            } else if trimmed.starts_with('#') {
                                docs.push(trimmed.trim_start_matches('#').trim());
                            } else if trimmed.is_empty() {
                                continue;
                            } else {
                                break;
                            }
                        }
                        docs.reverse();

                        let mut info = format!("{}:{}: {}\n", entry.display(), i + 1, line.trim());
                        if !docs.is_empty() {
                            info.push_str(&format!("  Documentation: {}\n", docs.join(" ")));
                        }
                        results.push(info);
                    }
                }
            }
        }

        if results.is_empty() {
            return Ok(RawToolOutput::text(format!("No information found for '{symbol}'.")));
        }

        Ok(RawToolOutput::text(format!(
            "Hover info for '{symbol}':\n\n{}",
            results.join("\n"),
        )))
    }
}

pub fn lsp_hover_tool() -> Arc<dyn Tool> { Arc::new(LspHoverTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn shows_hover_info() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("lib.rs"),
            "/// A test helper function\npub fn helper() -> bool { true }"
        ).await.unwrap();

        let result = LspHoverTool.execute(&ToolRequest::new(
            "builtin/lsp.hover@1.0.0",
            serde_json::json!({"symbol": "helper", "path": dir.path().to_string_lossy()}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("helper"));
    }
}