use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use regex::Regex;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `code_index.query` — Query symbols from the code index.
///
/// Searches through source files to find specific symbols (classes, methods, fields).
/// This is a lightweight version that performs on-the-fly search without persistent storage.
pub struct CodeIndexQueryTool;

#[async_trait]
impl Tool for CodeIndexQueryTool {
    fn key(&self) -> &str {
        "builtin/code_index.query@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let symbol = request.parameters["symbol"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("symbol is required".into()))?;
        if symbol.is_empty() {
            return Err(ToolError::InvalidArgument("symbol must not be empty".into()));
        }

        let kind = request.parameters["kind"].as_str().unwrap_or("all");
        let path = request.parameters["path"]
            .as_str()
            .filter(|p| !p.is_empty())
            .unwrap_or(".");
        let language = request.parameters["language"].as_str().unwrap_or("all");

        let search_dir = std::path::Path::new(path);
        if !search_dir.is_dir() {
            return Err(ToolError::InvalidArgument(format!("{path} is not a directory")));
        }

        let extensions = language_extensions(language);
        let patterns = if kind == "all" {
            language_patterns(language)
        } else {
            let kind_patterns = language_patterns(language);
            kind_patterns.into_iter().filter(|(k, _)| *k == kind).collect::<Vec<_>>()
        };

        let symbol_regex = Regex::new(&format!("(?i){}", regex::escape(symbol)))
            .map_err(|e| ToolError::InvalidArgument(format!("invalid symbol: {e}")))?;

        let mut results = Vec::new();
        let mut match_count = 0u64;
        let mut file_count = 0u64;

        let walk_pattern = format!("{}/**/*", path.trim_end_matches('/'));
        let glob_iter = glob::glob(&walk_pattern)
            .map_err(|e| ToolError::execution("code_index.query", format!("glob error: {e}"), false))?;

        for entry in glob_iter.flatten() {
            if !entry.is_file() {
                continue;
            }

            // Language filter
            if let Some(exts) = &extensions {
                if !exts.iter().any(|ext| {
                    entry.extension().and_then(|e| e.to_str()).map(|e| e == *ext).unwrap_or(false)
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

            let display_path = entry.to_string_lossy();
            let mut file_results = Vec::new();

            for (kind_name, pattern_str) in &patterns {
                if let Ok(re) = Regex::new(pattern_str) {
                    for cap in re.captures_iter(&content) {
                        if let Some(name) = cap.get(1) {
                            let name_str = name.as_str();
                            if symbol_regex.is_match(name_str) {
                                let line_num = content[..name.start()].lines().count();
                                file_results.push(format!("  {kind_name:8} {name_str:30} {display_path}:{line_num}"));
                                match_count += 1;
                            }
                        }
                    }
                }
            }

            if !file_results.is_empty() {
                file_count += 1;
                results.push(format!("{display_path}:\n{}", file_results.join("\n")));
            }
        }

        if results.is_empty() {
            return Ok(RawToolOutput::text(format!("No symbols found matching '{symbol}'.")));
        }

        Ok(RawToolOutput::text(format!(
            "Found {} match(es) for '{symbol}' in {} file(s):\n\n{}",
            match_count,
            file_count,
            results.join("\n\n"),
        )))
    }
}

fn language_extensions(language: &str) -> Option<Vec<&'static str>> {
    match language {
        "all" => None,
        "java" => Some(vec!["java"]),
        "rust" | "rs" => Some(vec!["rs"]),
        "python" | "py" => Some(vec!["py"]),
        "typescript" | "ts" => Some(vec!["ts", "tsx"]),
        "javascript" | "js" => Some(vec!["js", "jsx"]),
        "go" => Some(vec!["go"]),
        "kotlin" | "kt" => Some(vec!["kt", "kts"]),
        "csharp" | "cs" => Some(vec!["cs"]),
        _ => None,
    }
}

fn language_patterns(language: &str) -> Vec<(&'static str, &'static str)> {
    match language {
        "java" => vec![
            ("class", r"(?:public\s+|private\s+|protected\s+)?(?:abstract\s+|final\s+)?(?:class|interface|enum)\s+(\w+)"),
            ("method", r"(?:public|private|protected|static|final)\s+(?:\w+(?:<[^>]*>)?\s+)?(\w+)\s*\("),
            ("field", r"(?:public|private|protected|static|final)\s+(?:\w+(?:<[^>]*>)?\s+)+(\w+)\s*;"),
        ],
        "rust" => vec![
            ("struct", r"(?:pub\s+)?struct\s+(\w+)"),
            ("fn", r"(?:pub\s+)?(?:async\s+)?fn\s+(\w+)"),
            ("enum", r"(?:pub\s+)?enum\s+(\w+)"),
            ("trait", r"(?:pub\s+)?trait\s+(\w+)"),
            ("const", r"(?:pub\s+)?const\s+(\w+)"),
        ],
        "python" => vec![
            ("class", r"class\s+(\w+)"),
            ("function", r"(?:async\s+)?def\s+(\w+)"),
        ],
        "typescript" | "javascript" | "ts" | "js" => vec![
            ("class", r"(?:export\s+)?class\s+(\w+)"),
            ("function", r"(?:export\s+)?(?:async\s+)?function\s+(\w+)"),
            ("interface", r"(?:export\s+)?interface\s+(\w+)"),
        ],
        "go" => vec![
            ("struct", r"type\s+(\w+)\s+struct"),
            ("func", r"func\s+(?:\([^)]*\)\s+)?(\w+)"),
            ("interface", r"type\s+(\w+)\s+interface"),
        ],
        _ => vec![
            ("class", r"(?:class|struct|trait|interface|enum|object)\s+(\w+)"),
            ("function", r"(?:fn|def|fun|func|function)\s+(\w+)"),
        ],
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

pub fn code_index_query_tool() -> Arc<dyn Tool> {
    Arc::new(CodeIndexQueryTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn finds_symbol_by_name() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("app.java"),
            "public class AppService {\n    public void run() {}\n}"
        ).await.unwrap();

        let tool = CodeIndexQueryTool;
        let request = ToolRequest::new(
            "builtin/code_index.query@1.0.0",
            serde_json::json!({
                "symbol": "AppService",
                "path": dir.path().to_string_lossy(),
                "language": "java"
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("AppService"));
    }

    #[tokio::test]
    async fn case_insensitive_search() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("test.rs"),
            "pub fn HelloWorld() {}"
        ).await.unwrap();

        let tool = CodeIndexQueryTool;
        let request = ToolRequest::new(
            "builtin/code_index.query@1.0.0",
            serde_json::json!({
                "symbol": "helloworld",
                "path": dir.path().to_string_lossy(),
                "language": "rust"
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("HelloWorld"));
    }
}