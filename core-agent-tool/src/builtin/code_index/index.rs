use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use regex::Regex;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `code_index.index` — Scan a directory and extract symbols (classes, methods, fields).
///
/// Builds a lightweight symbol index using regex-based parsing.
/// The index is returned as structured text and can be queried via `code_index.query`.
pub struct CodeIndexIndexTool;

#[async_trait]
impl Tool for CodeIndexIndexTool {
    fn key(&self) -> &str {
        "builtin/code_index.index@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
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
        let patterns = language_patterns(language);
        let mut symbols: Vec<String> = Vec::new();
        let mut file_count = 0u64;

        let walk_pattern = format!("{}/**/*", path.trim_end_matches('/'));
        let glob_iter = glob::glob(&walk_pattern)
            .map_err(|e| ToolError::execution("code_index.index", format!("glob error: {e}"), false))?;

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
            file_count += 1;

            // Extract symbols using each pattern
            for (kind, pattern_str) in &patterns {
                if let Ok(re) = Regex::new(pattern_str) {
                    for cap in re.captures_iter(&content) {
                        if let Some(name) = cap.get(1) {
                            let name_str = name.as_str();
                            let line_num = content[..name.start()].lines().count();
                            symbols.push(format!("  {kind:8} {name_str:30} {display_path}:{line_num}"));
                        }
                    }
                }
            }
        }

        symbols.sort();

        if symbols.is_empty() {
            return Ok(RawToolOutput::text(format!(
                "No symbols found in {file_count} file(s) for language '{language}'."
            )));
        }

        Ok(RawToolOutput::text(format!(
            "Found {} symbol(s) in {} file(s):\n\n{}",
            symbols.len(),
            file_count,
            symbols.join("\n"),
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
        "cpp" | "c++" => Some(vec!["cpp", "hpp", "cc"]),
        "ruby" | "rb" => Some(vec!["rb"]),
        "php" => Some(vec!["php"]),
        "swift" => Some(vec!["swift"]),
        "scala" => Some(vec!["scala"]),
        _ => None,
    }
}

fn language_patterns(language: &str) -> Vec<(&'static str, &'static str)> {
    match language {
        "java" => vec![
            ("class", r"(?:public\s+|private\s+|protected\s+)?(?:abstract\s+|final\s+)?(?:class|interface|enum|@interface)\s+(\w+)"),
            ("method", r"(?:public|private|protected|static|final|synchronized|abstract)\s+(?:\w+(?:<[^>]*>)?\s+)?(\w+)\s*\([^)]*\)\s*(?:throws\s+\w+(?:\s*,\s*\w+)*)?\s*\{?"),
            ("field", r"(?:public|private|protected|static|final|volatile|transient)\s+(?:\w+(?:<[^>]*>)?\s+)+(\w+)\s*(?:=\s*[^;]+)?;"),
            ("annotation", r"@(\w+)"),
        ],
        "rust" => vec![
            ("struct", r"(?:pub\s+)?struct\s+(\w+)"),
            ("enum", r"(?:pub\s+)?enum\s+(\w+)"),
            ("fn", r"(?:pub\s+)?(?:unsafe\s+)?(?:async\s+)?fn\s+(\w+)"),
            ("trait", r"(?:pub\s+)?(?:unsafe\s+)?trait\s+(\w+)"),
            ("impl", r"(?:pub\s+)?(?:unsafe\s+)?impl\s+(\w+)"),
            ("const", r"(?:pub\s+)?const\s+(\w+)"),
            ("type", r"(?:pub\s+)?type\s+(\w+)"),
            ("macro", r"(?:macro_rules!\s*\(\s*(\w+))"),
        ],
        "python" => vec![
            ("class", r"class\s+(\w+)"),
            ("function", r"(?:async\s+)?def\s+(\w+)"),
            ("decorator", r"@(\w+)"),
        ],
        "typescript" | "javascript" | "ts" | "js" => vec![
            ("class", r"(?:export\s+(?:default\s+)?)?(?:abstract\s+)?class\s+(\w+)"),
            ("function", r"(?:export\s+(?:default\s+)?)?(?:async\s+)?function\s+(\w+)"),
            ("interface", r"(?:export\s+)?interface\s+(\w+)"),
            ("type", r"(?:export\s+)?type\s+(\w+)\s*="),
            ("enum", r"(?:export\s+)?enum\s+(\w+)"),
            ("const", r"(?:export\s+)?(?:const|let|var)\s+(\w+)\s*(?:=\s*(?:async\s+)?(?:function|\(|\w+))"),
        ],
        "go" => vec![
            ("struct", r"type\s+(\w+)\s+struct"),
            ("interface", r"type\s+(\w+)\s+interface"),
            ("func", r"func\s+(?:\([^)]*\)\s+)?(\w+)"),
            ("type", r"type\s+(\w+)\s+(?:\[\])?\w+"),
        ],
        "kotlin" => vec![
            ("class", r"(?:data\s+|sealed\s+|open\s+|abstract\s+)?class\s+(\w+)"),
            ("fun", r"(?:suspend\s+)?(?:inline\s+)?(?:private|public|protected|internal\s+)?fun\s+(\w+)"),
            ("object", r"(?:data\s+)?object\s+(\w+)"),
            ("interface", r"interface\s+(\w+)"),
        ],
        "csharp" => vec![
            ("class", r"(?:public|private|protected|internal\s+)?(?:abstract\s+|sealed\s+|static\s+)?class\s+(\w+)"),
            ("method", r"(?:public|private|protected|internal|static|virtual|override|async|unsafe)\s+(?:\w+\s+)?(\w+)\s*\([^)]*\)"),
            ("interface", r"(?:public|private|protected|internal\s+)?interface\s+(\w+)"),
            ("enum", r"(?:public|private|protected|internal\s+)?enum\s+(\w+)"),
            ("struct", r"(?:public|private|protected|internal\s+)?(?:readonly\s+)?struct\s+(\w+)"),
        ],
        "all" => vec![
            ("class", r"(?:class|struct|trait|interface|enum|object|type)\s+(\w+)"),
            ("function", r"(?:fn|def|fun|func|function)\s+(\w+)"),
        ],
        _ => vec![
            ("class", r"(?:class|struct|trait|interface|enum|object|type)\s+(\w+)"),
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

pub fn code_index_index_tool() -> Arc<dyn Tool> {
    Arc::new(CodeIndexIndexTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn extracts_java_symbols() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("UserService.java"),
            "public class UserService {\n    public void login() {}\n    private String name;\n}"
        ).await.unwrap();

        let tool = CodeIndexIndexTool;
        let request = ToolRequest::new(
            "builtin/code_index.index@1.0.0",
            serde_json::json!({
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
        assert!(text.contains("login"));
        assert!(text.contains("name"));
    }

    #[tokio::test]
    async fn extracts_rust_symbols() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("lib.rs"),
            "pub struct Config {\n    pub name: String,\n}\n\npub fn run() {}"
        ).await.unwrap();

        let tool = CodeIndexIndexTool;
        let request = ToolRequest::new(
            "builtin/code_index.index@1.0.0",
            serde_json::json!({
                "path": dir.path().to_string_lossy(),
                "language": "rust"
            }),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Config"));
        assert!(text.contains("run"));
    }
}