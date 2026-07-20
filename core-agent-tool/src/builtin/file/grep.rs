use std::sync::Arc;

use async_trait::async_trait;
use regex::Regex;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `file.grep` — Search file contents using regex patterns.
pub struct FileGrepTool;

#[async_trait]
impl Tool for FileGrepTool {
    fn key(&self) -> &str {
        "builtin/file.grep@1.0.0"
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

        let case_insensitive = request.parameters["-i"].as_bool().unwrap_or(false);
        let output_mode = request.parameters["output_mode"]
            .as_str()
            .unwrap_or("content");
        let context_lines = request.parameters["context"].as_u64().unwrap_or(0) as usize;
        let glob_filter = request.parameters["glob"].as_str();
        let search_path = request.parameters["path"]
            .as_str()
            .and_then(|p| if p.is_empty() { None } else { Some(p) })
            .unwrap_or(".");

        let regex = if case_insensitive {
            Regex::new(&format!("(?i){pattern}"))
                .map_err(|e| ToolError::InvalidArgument(format!("invalid regex: {e}")))?
        } else {
            Regex::new(pattern)
                .map_err(|e| ToolError::InvalidArgument(format!("invalid regex: {e}")))?
        };

        let search_dir = std::path::Path::new(search_path);
        if !search_dir.is_dir() {
            return Err(ToolError::InvalidArgument(format!("{search_path} is not a directory")));
        }

        let mut matches = Vec::new();
        let mut file_count = 0u64;
        let mut match_count = 0u64;

        let walk_pattern = if let Some(glob) = glob_filter {
            format!("{}/{}", search_path.trim_end_matches('/'), glob)
        } else {
            format!("{}/**/*", search_path.trim_end_matches('/'))
        };

        let glob_iter = glob::glob(&walk_pattern)
            .map_err(|e| ToolError::execution("file.grep", format!("glob error: {e}"), false))?;

        for entry in glob_iter.flatten() {
            if !entry.is_file() {
                continue;
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
            let mut line_matches = Vec::new();

            for (line_num, line) in content.lines().enumerate() {
                if regex.is_match(line) {
                    match_count += 1;
                    if output_mode == "content" {
                        let prefix = format!("{}:{}", display_path, line_num + 1);
                        if context_lines > 0 {
                            line_matches.push(format!("{prefix}: {line}"));
                        } else {
                            line_matches.push(format!("{prefix}: {line}"));
                        }
                    }
                }
            }

            if !line_matches.is_empty() {
                file_count += 1;
                matches.push(format!("{}:\n  {}", display_path, line_matches.join("\n  ")));
            }
        }

        let result = match output_mode {
            "count" => Ok(RawToolOutput::text(format!(
                "{} matches in {} files", match_count, file_count
            ))),
            "files_with_matches" => Ok(RawToolOutput::text(format!(
                "{} file(s) matched:\n{}", file_count,
                matches.iter().map(|m| m.split(':').next().unwrap_or(m)).collect::<Vec<_>>().join("\n")
            ))),
            _ => {
                if matches.is_empty() {
                    Ok(RawToolOutput::text("No matches found."))
                } else {
                    Ok(RawToolOutput::text(matches.join("\n\n")))
                }
            }
        };

        result
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

pub fn file_grep_tool() -> Arc<dyn Tool> {
    Arc::new(FileGrepTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn finds_matching_lines() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("test.txt"), "hello world\nfoo bar\nhello again").await.unwrap();

        let tool = FileGrepTool;
        let request = ToolRequest::new(
            "builtin/file.grep@1.0.0",
            serde_json::json!({"pattern": "hello", "path": dir.path().to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("hello"));
        assert!(text.contains("test.txt"));
    }

    #[tokio::test]
    async fn count_mode_works() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("test.txt"), "hello world\nhello again").await.unwrap();

        let tool = FileGrepTool;
        let request = ToolRequest::new(
            "builtin/file.grep@1.0.0",
            serde_json::json!({"pattern": "hello", "path": dir.path().to_string_lossy(), "output_mode": "count"}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("2 matches"));
    }

    #[tokio::test]
    async fn case_insensitive_search() {
        let dir = tempdir().unwrap();
        tokio::fs::write(dir.path().join("test.txt"), "Hello World\nGoodbye").await.unwrap();

        let tool = FileGrepTool;
        let request = ToolRequest::new(
            "builtin/file.grep@1.0.0",
            serde_json::json!({"pattern": "hello", "path": dir.path().to_string_lossy(), "-i": true}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("Hello"));
    }
}