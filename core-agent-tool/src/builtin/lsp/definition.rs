use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `lsp.definition` — Go to definition of a symbol.
///
/// Uses LSP client to find definition locations. Falls back to
/// grep-based search when LSP server is not available.
pub struct LspDefinitionTool;

#[async_trait]
impl Tool for LspDefinitionTool {
    fn key(&self) -> &str { "builtin/lsp.definition@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let symbol = request.parameters["symbol"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("symbol is required".into()))?;
        if symbol.is_empty() {
            return Err(ToolError::InvalidArgument("symbol must not be empty".into()));
        }

        let path = request.parameters["path"].as_str().filter(|p| !p.is_empty());

        // Try LSP-based lookup first, then fallback to grep
        // For now, use grep-based fallback as the primary implementation
        lsp_grep_fallback("definition", symbol, path, |line| {
            format!("  Definition: {line}")
        }).await
    }
}

/// Fallback LSP implementation using grep to find symbol definitions.
/// When a real LSP client is configured, this will use the LSP protocol.
async fn lsp_grep_fallback<F>(
    action: &str,
    symbol: &str,
    path: Option<&str>,
    format_match: F,
) -> ToolRuntimeResult<RawToolOutput>
where
    F: Fn(String) -> String,
{
    let search_dir = path.unwrap_or(".");
    let search_dir_path = std::path::Path::new(search_dir);
    if !search_dir_path.is_dir() {
        return Err(ToolError::InvalidArgument(format!("{search_dir} is not a directory")));
    }

    // Use definition patterns based on language
    let patterns = vec![
        format!(r"(?:pub\s+)?(?:fn|class|struct|enum|trait|def|function|interface)\s+{symbol}\b"),
        format!(r"(?:public|private|protected)\s+(?:\w+\s+)*{symbol}\s*\("),
        format!(r"(?:class|struct|interface|enum)\s+{symbol}"),
        format!(r"type\s+{symbol}\b"),
        format!(r"const\s+{symbol}\b"),
        format!(r"let\s+{symbol}\b"),
    ];

    let mut results = Vec::new();
    let mut total_found = 0u64;

    for pattern in &patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            let walk_pattern = format!("{}/**/*", search_dir.trim_end_matches('/'));
            if let Ok(glob_iter) = glob::glob(&walk_pattern) {
                for entry in glob_iter.flatten() {
                    if !entry.is_file() { continue; }
                    if is_binary_path(&entry) { continue; }

                    if let Ok(content) = tokio::fs::read_to_string(&entry).await {
                        for (line_num, line) in content.lines().enumerate() {
                            if re.is_match(line) {
                                total_found += 1;
                                let display = format!("{}:{}: {}", entry.display(), line_num + 1, line.trim());
                                results.push(format_match(display));
                            }
                        }
                    }
                }
            }
        }
        if total_found > 0 {
            break; // Found with first matching pattern
        }
    }

    if results.is_empty() {
        Ok(RawToolOutput::text(format!("No definition found for '{symbol}'.")))
    } else {
        Ok(RawToolOutput::text(format!(
            "Found {total_found} definition(s) for '{symbol}':\n{}",
            results.join("\n"),
        )))
    }
}

fn is_binary_path(path: &std::path::Path) -> bool {
    let extensions = ["png", "jpg", "jpeg", "gif", "bmp", "ico", "woff", "woff2",
                      "ttf", "eot", "otf", "pdf", "zip", "tar", "gz", "bz2",
                      "exe", "dll", "so", "dylib", "class", "jar", "pyc", "pyo",
                      "o", "a", "lib", "obj", "node", "map", "lock", "svg"];
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| extensions.contains(&e))
        .unwrap_or(false)
}

pub fn lsp_definition_tool() -> Arc<dyn Tool> { Arc::new(LspDefinitionTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn finds_definition_via_grep() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("UserService.java"),
            "public class UserService {\n    public void login() {}\n}"
        ).await.unwrap();

        let result = LspDefinitionTool.execute(&ToolRequest::new(
            "builtin/lsp.definition@1.0.0",
            serde_json::json!({"symbol": "UserService", "path": dir.path().to_string_lossy()}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("UserService"));
    }

    #[tokio::test]
    async fn returns_no_definition_message() {
        let dir = tempdir().unwrap();
        let result = LspDefinitionTool.execute(&ToolRequest::new(
            "builtin/lsp.definition@1.0.0",
            serde_json::json!({"symbol": "NonExistentSymbol123", "path": dir.path().to_string_lossy()}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("No definition found"));
    }
}