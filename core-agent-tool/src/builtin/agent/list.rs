use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `agent.list` — List active sub-agents.
pub struct AgentListTool;

#[async_trait]
impl Tool for AgentListTool {
    fn key(&self) -> &str { "builtin/agent.list@1.0.0" }

    async fn execute(&self, _request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        Ok(RawToolOutput::text("No active agents."))
    }
}

pub fn agent_list_tool() -> Arc<dyn Tool> { Arc::new(AgentListTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn lists_empty() {
        let result = AgentListTool.execute(&ToolRequest::new(
            "builtin/agent.list@1.0.0", serde_json::json!({}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("No active agents"));
    }
}