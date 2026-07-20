use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `agent.send` — Send a message to another agent.
pub struct AgentSendTool;

#[async_trait]
impl Tool for AgentSendTool {
    fn key(&self) -> &str { "builtin/agent.send@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let agent_id = request.parameters["agent_id"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("agent_id is required".into()))?;
        let message = request.parameters["message"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("message is required".into()))?;
        Ok(RawToolOutput::text(format!("[AGENT_SEND] → {agent_id}: {message}")))
    }
}

pub fn agent_send_tool() -> Arc<dyn Tool> { Arc::new(AgentSendTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn sends_message() {
        let tool = AgentSendTool;
        let result = tool.execute(&ToolRequest::new(
            "builtin/agent.send@1.0.0",
            serde_json::json!({"agent_id": "agent-1", "message": "hello"}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("agent-1") && text.contains("hello"));
    }
}