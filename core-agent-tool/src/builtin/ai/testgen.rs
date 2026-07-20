use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `test.generate` — Generate unit/integration tests for given code.
/// Stub — requires LLM or test generation tool configuration.
pub struct TestGenerateTool;

#[async_trait]
impl Tool for TestGenerateTool {
    fn key(&self) -> &str { "builtin/test.generate@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let path = request.parameters["path"].as_str().unwrap_or("");
        let framework = request.parameters["framework"].as_str().unwrap_or("auto");
        Ok(RawToolOutput::text(format!(
            "[TEST_GENERATOR] Path: {path}, Framework: {framework}\n\nStatus: Test generation requires LLM or test generation tool configuration.\n\nTo configure:\n  1. Set TEST_GENERATOR_ENGINE (llm | diffblue | evosuite)\n  2. Provide API endpoint and credentials"
        )))
    }
}

pub fn test_generate_tool() -> Arc<dyn Tool> { Arc::new(TestGenerateTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = TestGenerateTool.execute(&ToolRequest::new("builtin/test.generate@1.0.0", serde_json::json!({"path": "src/main/java"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("TEST_GENERATOR")), _ => panic!("expected text") };
    }
}