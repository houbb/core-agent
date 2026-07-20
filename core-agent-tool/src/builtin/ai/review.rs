use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `code.review` — Review code changes for quality and security issues.
/// Stub — requires LLM or static analysis tool configuration.
pub struct CodeReviewTool;

#[async_trait]
impl Tool for CodeReviewTool {
    fn key(&self) -> &str { "builtin/code.review@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let path = request.parameters["path"].as_str().unwrap_or(".");
        let diff = request.parameters["diff"].as_bool().unwrap_or(false);
        Ok(RawToolOutput::text(format!(
            "[CODE_REVIEW] Path: {path}, Diff mode: {diff}\n\nStatus: Code review requires LLM or static analysis tool configuration.\n\nTo configure:\n  1. Set CODE_REVIEW_ENGINE (llm | semgrep | sonarqube)\n  2. Provide API endpoint and credentials"
        )))
    }
}

pub fn code_review_tool() -> Arc<dyn Tool> { Arc::new(CodeReviewTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = CodeReviewTool.execute(&ToolRequest::new("builtin/code.review@1.0.0", serde_json::json!({"path": "."})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("CODE_REVIEW")), _ => panic!("expected text") };
    }
}