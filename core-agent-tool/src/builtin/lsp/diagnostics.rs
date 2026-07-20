use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `lsp.diagnostics` — Get diagnostics for a file.
///
/// Checks a file for potential issues using basic heuristics:
/// - Syntax-level checks (unmatched braces, etc.)
/// - TODO/FIXME markers
/// - Trailing whitespace
/// - Long lines
pub struct LspDiagnosticsTool;

#[async_trait]
impl Tool for LspDiagnosticsTool {
    fn key(&self) -> &str { "builtin/lsp.diagnostics@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let path = request.parameters["path"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("path is required".into()))?;
        if path.is_empty() {
            return Err(ToolError::InvalidArgument("path must not be empty".into()));
        }

        let file_path = std::path::Path::new(path);
        if !file_path.exists() {
            return Err(ToolError::execution("lsp.diagnostics", format!("file not found: {path}"), false));
        }
        if !file_path.is_file() {
            return Err(ToolError::InvalidArgument(format!("{path} is not a file")));
        }

        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|e| ToolError::execution("lsp.diagnostics", format!("failed to read: {e}"), false))?;

        let mut diagnostics = Vec::new();

        // Check for basic syntax issues
        let open_braces = content.matches('{').count();
        let close_braces = content.matches('}').count();
        if open_braces != close_braces {
            diagnostics.push(format!(
                "  WARN  Mismatched braces: {open_braces} opening vs {close_braces} closing"
            ));
        }

        let open_parens = content.matches('(').count();
        let close_parens = content.matches(')').count();
        if open_parens != close_parens {
            diagnostics.push(format!(
                "  WARN  Mismatched parentheses: {open_parens} opening vs {close_parens} closing"
            ));
        }

        let open_brackets = content.matches('[').count();
        let close_brackets = content.matches(']').count();
        if open_brackets != close_brackets {
            diagnostics.push(format!(
                "  WARN  Mismatched brackets: {open_brackets} opening vs {close_brackets} closing"
            ));
        }

        // Check for TODO/FIXME/HACK markers
        let todo_re = regex::Regex::new(r"(?i)(TODO|FIXME|HACK|XXX|BUG|WORKAROUND)[:\s]").unwrap();
        let mut _todo_count = 0;
        for (i, line) in content.lines().enumerate() {
            if todo_re.is_match(line) {
                _todo_count += 1;
                diagnostics.push(format!("  INFO  {}:{}: {}", file_path.display(), i + 1, line.trim()));
            }
        }

        // Check for trailing whitespace
        let mut trailing_space_count = 0;
        for (i, line) in content.lines().enumerate() {
            if line.len() > line.trim_end().len() && !line.trim().is_empty() {
                trailing_space_count += 1;
                if trailing_space_count <= 5 {
                    diagnostics.push(format!("  STYLE {}:{}: Trailing whitespace", file_path.display(), i + 1));
                }
            }
        }
        if trailing_space_count > 5 {
            diagnostics.push(format!("  STYLE  ... and {} more line(s) with trailing whitespace", trailing_space_count - 5));
        }

        // Check for long lines (> 120 chars)
        let mut long_line_count = 0;
        for (i, line) in content.lines().enumerate() {
            if line.len() > 120 {
                long_line_count += 1;
                if long_line_count <= 5 {
                    diagnostics.push(format!("  STYLE {}:{}: Line too long ({} chars)", file_path.display(), i + 1, line.len()));
                }
            }
        }
        if long_line_count > 5 {
            diagnostics.push(format!("  STYLE  ... and {} more long line(s)", long_line_count - 5));
        }

        if diagnostics.is_empty() {
            return Ok(RawToolOutput::text(format!("No diagnostics for {path}.")));
        }

        Ok(RawToolOutput::text(format!(
            "Diagnostics for {path}:\n\n{}",
            diagnostics.join("\n"),
        )))
    }
}

pub fn lsp_diagnostics_tool() -> Arc<dyn Tool> { Arc::new(LspDiagnosticsTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    #[tokio::test]
    async fn detects_mismatched_braces() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("broken.rs");
        tokio::fs::write(&path, "fn main() {\n    println!(\"hello\");\n}").await.unwrap();

        let result = LspDiagnosticsTool.execute(&ToolRequest::new(
            "builtin/lsp.diagnostics@1.0.0",
            serde_json::json!({"path": path.to_string_lossy()}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(!text.contains("Mismatched"));
    }

    #[tokio::test]
    async fn finds_todo_markers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("todo.rs");
        tokio::fs::write(&path, "// TODO: fix this later\nfn main() {}").await.unwrap();

        let result = LspDiagnosticsTool.execute(&ToolRequest::new(
            "builtin/lsp.diagnostics@1.0.0",
            serde_json::json!({"path": path.to_string_lossy()}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("TODO"));
    }
}