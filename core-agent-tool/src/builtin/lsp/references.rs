use std::sync::Arc;

use async_trait::async_trait;
use regex::Regex;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `lsp.references` — Find references to a symbol.
///
/// Uses grep-based search across the project to find all usages of a symbol.
pub struct LspReferencesTool;

#[async_trait]
impl Tool for LspReferencesTool {
    fn key(&self) -> &str { "builtin/lsp.references@1.0.0" }

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

        // Use word boundary regex to find all references
        let pattern = format!(r"\b{symbol}\b");
        let regex = Regex::new(&pattern)
            .map_err(|e| ToolError::InvalidArgument(format!("invalid symbol: {e}")))?;

        let mut results = Vec::new();
        let mut ref_count = 0u64;
        let mut file_count = 0u64;

        let walk_pattern = format!("{}/**/*", path.trim_end_matches('/'));
        let glob_iter = glob::glob(&walk_pattern)
            .map_err(|e| ToolError::execution("lsp.references", format!("glob error: {e}"), false))?;

        for entry in glob_iter.flatten() {
            if !entry.is_file() { continue; }
            if is_binary(&entry) { continue; }

            if let Ok(content) = tokio::fs::read_to_string(&entry).await {
                let mut file_refs = Vec::new();
                for (line_num, line) in content.lines().enumerate() {
                    if regex.is_match(line) {
                        ref_count += 1;
                        file_refs.push(format!("  {}:{}: {}", entry.display(), line_num + 1, line.trim()));
                    }
                }
                if !file_refs.is_empty() {
                    file_count += 1;
                    results.push(format!("{}:\n{}", entry.display(), file_refs.join("\n")));
                }
            }
        }

        if results.is_empty() {
            return Ok(RawToolOutput::text(format!("No references found for '{symbol}'.")));
        }

        Ok(RawToolOutput::text(format!(
            "Found {ref_count} reference(s) to '{symbol}' in {file_count} file(s):\n\n{}",
            results.join("\n\n"),
        )))
    }
}

fn is_binary(path: &std::path::Path) -> bool {
    let extensions = ["png", "jpg", "jpeg", "gif", "bmp", "ico", "woff", "woff2",
                      "ttf", "eot", "otf", "pdf", "zip", "tar", "gz", "bz2",
                      "exe", "dll", "so", "dylib", "class", "jar", "pyc", "pyo",
                      "o", "a", "lib", "obj", "node", "map", "lock", "svg"];
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| extensions.contains(&e))
        .unwrap_or(false)
}

pub fn lsp_references_tool() -> Arc<dyn Tool> { Arc::new(LspReferencesTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn finds_references() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("main.rs"),
            "use helper::Helper;\n\nfn main() {\n    let h = Helper::new();\n}"
        ).await.unwrap();

        let result = LspReferencesTool.execute(&ToolRequest::new(
            "builtin/lsp.references@1.0.0",
            serde_json::json!({"symbol": "Helper", "path": dir.path().to_string_lossy()}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Helper"));
    }
}