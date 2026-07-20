use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `lsp.symbols` — Search workspace symbols.
///
/// Searches for symbols (classes, functions, types) across the workspace.
/// Uses grep-based pattern matching with language-aware filtering.
pub struct LspSymbolsTool;

#[async_trait]
impl Tool for LspSymbolsTool {
    fn key(&self) -> &str { "builtin/lsp.symbols@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let query = request.parameters["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("query is required".into()))?;
        if query.is_empty() {
            return Err(ToolError::InvalidArgument("query must not be empty".into()));
        }

        let path = request.parameters["path"].as_str().filter(|p| !p.is_empty()).unwrap_or(".");

        let search_dir = std::path::Path::new(path);
        if !search_dir.is_dir() {
            return Err(ToolError::InvalidArgument(format!("{path} is not a directory")));
        }

        // Pattern for symbol definitions across languages
        let patterns = vec![
            format!(r"(?:pub\s+)?(?:class|struct|enum|trait|interface)\s+(\w*{query}\w*)"),
            format!(r"(?:pub\s+)?(?:fn|def|fun|func|function)\s+(\w*{query}\w*)"),
            format!(r"(?:public|private|protected)\s+(?:static\s+)?(?:\w+\s+)+(\w*{query}\w*)\s*\("),
            format!(r"type\s+(\w*{query}\w*)"),
            format!(r"(?:const|let|var)\s+(\w*{query}\w*)"),
        ];

        let mut results = Vec::new();
        let mut total_count = 0u64;

        for pattern in &patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                let walk_pattern = format!("{}/**/*", path.trim_end_matches('/'));
                if let Ok(glob_iter) = glob::glob(&walk_pattern) {
                    for entry in glob_iter.flatten() {
                        if !entry.is_file() { continue; }
                        let ext = entry.extension().and_then(|e| e.to_str()).unwrap_or("");
                        if ["png","jpg","jpeg","gif","class","jar","pyc","node","lock","svg"].contains(&ext) {
                            continue;
                        }

                        if let Ok(content) = tokio::fs::read_to_string(&entry).await {
                            for cap in re.captures_iter(&content) {
                                if let Some(name) = cap.get(1) {
                                    let line_num = content[..name.start()].lines().count();
                                    results.push(format!("  {}:{}: {}", entry.display(), line_num, name.as_str()));
                                    total_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        results.sort();
        results.dedup();

        if results.is_empty() {
            return Ok(RawToolOutput::text(format!("No symbols found matching '{query}'.")));
        }

        Ok(RawToolOutput::text(format!(
            "Found {total_count} symbol(s) matching '{query}':\n{}",
            results.join("\n"),
        )))
    }
}

pub fn lsp_symbols_tool() -> Arc<dyn Tool> { Arc::new(LspSymbolsTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn searches_symbols() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("lib.rs"),
            "pub fn hello_world() {}\npub struct MyStruct {}"
        ).await.unwrap();

        let result = LspSymbolsTool.execute(&ToolRequest::new(
            "builtin/lsp.symbols@1.0.0",
            serde_json::json!({"query": "hello", "path": dir.path().to_string_lossy()}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("hello_world"));
    }
}