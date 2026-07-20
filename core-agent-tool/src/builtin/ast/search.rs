use std::sync::Arc;

use async_trait::async_trait;
use regex::Regex;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `ast.search` — Search code using AST-aware patterns.
///
/// Supports language-filtered, structure-aware code search.
/// Patterns use regex with additional language context filtering.
pub struct AstSearchTool;

#[async_trait]
impl Tool for AstSearchTool {
    fn key(&self) -> &str {
        "builtin/ast.search@1.0.0"
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

        let language = request.parameters["language"].as_str().unwrap_or("all");
        let search_path = request.parameters["path"]
            .as_str()
            .filter(|p| !p.is_empty())
            .unwrap_or(".");

        let regex = Regex::new(pattern)
            .map_err(|e| ToolError::InvalidArgument(format!("invalid regex pattern: {e}")))?;

        let extensions = language_extensions(language);

        let search_dir = std::path::Path::new(search_path);
        if !search_dir.is_dir() {
            return Err(ToolError::InvalidArgument(format!("{search_path} is not a directory")));
        }

        let mut results = Vec::new();
        let mut match_count = 0u64;
        let mut file_count = 0u64;

        let walk_pattern = format!("{}/**/*", search_path.trim_end_matches('/'));
        let glob_iter = glob::glob(&walk_pattern)
            .map_err(|e| ToolError::execution("ast.search", format!("glob error: {e}"), false))?;

        for entry in glob_iter.flatten() {
            if !entry.is_file() {
                continue;
            }

            // Language filter: check extension
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

            let mut line_matches = Vec::new();
            for (line_num, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    match_count += 1;
                    let display_path = entry.to_string_lossy();
                    line_matches.push(format!("{}:{}: {}", display_path, line_num + 1, line.trim()));
                }
            }

            if !line_matches.is_empty() {
                file_count += 1;
                results.push(line_matches.join("\n"));
            }
        }

        if results.is_empty() {
            return Ok(RawToolOutput::text("No matches found."));
        }

        Ok(RawToolOutput::text(format!(
            "Found {} matches in {} file(s):\n\n{}",
            match_count,
            file_count,
            results.join("\n\n"),
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

pub(crate) fn language_extensions(language: &str) -> Option<Vec<&'static str>> {
    match language {
        "all" => None,
        "java" => Some(vec!["java"]),
        "rust" | "rs" => Some(vec!["rs"]),
        "python" | "py" => Some(vec!["py"]),
        "typescript" | "ts" => Some(vec!["ts", "tsx"]),
        "javascript" | "js" => Some(vec!["js", "jsx", "mjs"]),
        "go" => Some(vec!["go"]),
        "kotlin" | "kt" => Some(vec!["kt", "kts"]),
        "scala" => Some(vec!["scala"]),
        "ruby" | "rb" => Some(vec!["rb"]),
        "php" => Some(vec!["php"]),
        "c" => Some(vec!["c", "h"]),
        "cpp" | "c++" => Some(vec!["cpp", "hpp", "cc", "cxx"]),
        "csharp" | "cs" => Some(vec!["cs"]),
        "swift" => Some(vec!["swift"]),
        "vue" => Some(vec!["vue"]),
        "html" => Some(vec!["html", "htm"]),
        "css" => Some(vec!["css", "scss", "less"]),
        "yaml" | "yml" => Some(vec!["yaml", "yml"]),
        "toml" => Some(vec!["toml"]),
        "json" => Some(vec!["json"]),
        "markdown" | "md" => Some(vec!["md"]),
        "sql" => Some(vec!["sql"]),
        "shell" | "sh" => Some(vec!["sh", "bash"]),
        _ => None,
    }
}

pub fn ast_search_tool() -> Arc<dyn Tool> {
    Arc::new(AstSearchTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn finds_matching_code_patterns() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("UserService.java"),
            "public class UserService {\n    public void login() {}\n}"
        ).await.unwrap();

        let tool = AstSearchTool;
        let request = ToolRequest::new(
            "builtin/ast.search@1.0.0",
            serde_json::json!({
                "pattern": "class\\s+\\w+",
                "path": dir.path().to_string_lossy(),
                "language": "java"
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("UserService"));
    }

    #[tokio::test]
    async fn language_filter_works() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("a.rs"), "fn hello() {}").await.unwrap();
        tokio::fs::write(dir.path().join("b.py"), "def hello(): pass").await.unwrap();

        let tool = AstSearchTool;
        let request = ToolRequest::new(
            "builtin/ast.search@1.0.0",
            serde_json::json!({
                "pattern": "hello",
                "path": dir.path().to_string_lossy(),
                "language": "rust"
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("a.rs"));
        assert!(!text.contains("b.py"));
    }

    #[tokio::test]
    async fn rejects_empty_pattern() {
        let tool = AstSearchTool;
        let request = ToolRequest::new(
            "builtin/ast.search@1.0.0",
            serde_json::json!({"pattern": ""}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}