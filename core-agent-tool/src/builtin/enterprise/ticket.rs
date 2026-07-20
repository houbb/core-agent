use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `ticket.create` — Create a ticket in Jira/ServiceNow.
/// Stub — requires external system configuration.
pub struct TicketCreateTool;

#[async_trait]
impl Tool for TicketCreateTool {
    fn key(&self) -> &str { "builtin/ticket.create@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let title = request.parameters["title"].as_str().unwrap_or("");
        let description = request.parameters["description"].as_str().unwrap_or("");
        Ok(RawToolOutput::text(format!(
            "[TICKET] Title: {title}\nDescription: {description}\n\nStatus: Ticket tool requires Jira/ServiceNow configuration.\n\nTo configure:\n  1. Set TICKET_ENDPOINT\n  2. Set TICKET_TYPE (jira | servicenow)\n  3. Provide authentication credentials"
        )))
    }
}

pub fn ticket_create_tool() -> Arc<dyn Tool> { Arc::new(TicketCreateTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = TicketCreateTool.execute(&ToolRequest::new("builtin/ticket.create@1.0.0", serde_json::json!({"title": "Bug: login fails"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("TICKET")), _ => panic!("expected text") };
    }
}