use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `agent.spawn` — Create a sub-agent for a task.
pub struct AgentSpawnTool;

#[async_trait]
impl Tool for AgentSpawnTool {
    fn key(&self) -> &str { "builtin/agent.spawn@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let task = request.parameters["task"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("task is required".into()))?;
        if task.is_empty() {
            return Err(ToolError::InvalidArgument("task must not be empty".into()));
        }
        Ok(RawToolOutput::text(format!("[AGENT_SPAWN] Sub-agent created for: {task}")))
    }
}

pub fn agent_spawn_tool() -> Arc<dyn Tool> { Arc::new(AgentSpawnTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn spawns_agent() {
        let tool = AgentSpawnTool;
        let result = tool.execute(&ToolRequest::new(
            "builtin/agent.spawn@1.0.0",
            serde_json::json!({"task": "analyze logs"}),
        ), &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("analyze logs"));
    }
}