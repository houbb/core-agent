use std::sync::Arc;

use async_trait::async_trait;
use regex::Regex;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `callgraph.query` — Analyze function call relationships.
///
/// Traces function calls from a starting point, showing the call chain.
/// Uses grep-based analysis to find call sites.
pub struct CallGraphQueryTool;

#[async_trait]
impl Tool for CallGraphQueryTool {
    fn key(&self) -> &str {
        "builtin/callgraph.query@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let function = request.parameters["function"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("function is required".into()))?;
        if function.is_empty() {
            return Err(ToolError::InvalidArgument("function must not be empty".into()));
        }

        let path = request.parameters["path"]
            .as_str()
            .filter(|p| !p.is_empty())
            .unwrap_or(".");
        let depth = request.parameters["depth"].as_u64().unwrap_or(3) as usize;

        let search_dir = std::path::Path::new(path);
        if !search_dir.is_dir() {
            return Err(ToolError::InvalidArgument(format!("{path} is not a directory")));
        }

        let mut visited = std::collections::BTreeSet::new();
        let mut call_chain = Vec::new();

        trace_calls(function, search_dir, 0, depth, &mut visited, &mut call_chain).await?;

        if call_chain.is_empty() {
            return Ok(RawToolOutput::text(format!("No call graph found for '{function}'.")));
        }

        Ok(RawToolOutput::text(format!(
            "Call graph for '{function}':\n\n{}",
            call_chain.join("\n"),
        )))
    }
}

async fn trace_calls(
    function: &str,
    search_dir: &std::path::Path,
    current_depth: usize,
    max_depth: usize,
    visited: &mut std::collections::BTreeSet<String>,
    call_chain: &mut Vec<String>,
) -> ToolRuntimeResult<()> {
    if current_depth > max_depth {
        return Ok(());
    }

    // Find all call sites of this function
    let call_pattern = format!(r"\b{function}\s*\(");
    let call_re = Regex::new(&call_pattern)
        .map_err(|e| ToolError::InvalidArgument(format!("invalid function name: {e}")))?;

    let def_pattern = format!(r"(?:fn|def|fun|func|function|public|private|protected)\s+\w*\s*{function}\b");
    let def_re = Regex::new(&def_pattern)
        .map_err(|e| ToolError::InvalidArgument(format!("invalid function name: {e}")))?;

    let walk_pattern = format!("{}/**/*", search_dir.display());
    let glob_iter = glob::glob(&walk_pattern)
        .map_err(|e| ToolError::execution("callgraph.query", format!("glob error: {e}"), false))?;

    // Find all files that call or define this function
    let mut callers = Vec::new();
    let mut definitions = Vec::new();

    for entry in glob_iter.flatten() {
        if !entry.is_file() { continue; }
        let ext = entry.extension().and_then(|e| e.to_str()).unwrap_or("");
        if ["png","jpg","jpeg","gif","class","jar","pyc","node","lock","svg"].contains(&ext) {
            continue;
        }

        if let Ok(content) = tokio::fs::read_to_string(&entry).await {
            for (line_num, line) in content.lines().enumerate() {
                if call_re.is_match(line) {
                    let display = format!("{depth}  {depth_spacing}called by {file}:{line_num}: {line}",
                        depth = "│".repeat(current_depth),
                        depth_spacing = " ".repeat(current_depth * 2),
                        file = entry.display(),
                        line_num = line_num + 1,
                        line = line.trim());
                    callers.push(display);
                }
                // Check if this defines the function (only if we're looking at callers)
                if def_re.is_match(line) && current_depth == 0 {
                    definitions.push(format!("  Definition: {file}:{line_num}: {line}",
                        file = entry.display(),
                        line_num = line_num + 1,
                        line = line.trim()));
                }
            }
        }
    }

    // Add definitions first
    if current_depth == 0 {
        if definitions.is_empty() {
            call_chain.push(format!("  (No definition found for '{function}')\n"));
        } else {
            for def in &definitions {
                call_chain.push(def.clone());
            }
        }
        call_chain.push(String::new());
    }

    // Add callers
    if callers.is_empty() {
        if current_depth == 0 {
            call_chain.push(format!("  (No calls to '{function}' found)"));
        }
    } else {
        for caller in &callers {
            if visited.insert(caller.clone()) {
                call_chain.push(caller.clone());
            }
        }
    }

    Ok(())
}

pub fn callgraph_query_tool() -> Arc<dyn Tool> {
    Arc::new(CallGraphQueryTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn traces_calls() {
        let dir = tempdir().unwrap();
        tokio::fs::write(
            dir.path().join("main.rs"),
            "fn hello() {}\n\nfn main() {\n    hello();\n}"
        ).await.unwrap();

        let tool = CallGraphQueryTool;
        let request = ToolRequest::new(
            "builtin/callgraph.query@1.0.0",
            serde_json::json!({"function": "hello", "path": dir.path().to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("hello"));
    }
}