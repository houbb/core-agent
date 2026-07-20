use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `notification.send` — Send notification via Slack/DingTalk/Email.
/// Stub — requires external system configuration.
pub struct NotificationSendTool;

#[async_trait]
impl Tool for NotificationSendTool {
    fn key(&self) -> &str { "builtin/notification.send@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let channel = request.parameters["channel"].as_str().unwrap_or("default");
        let message = request.parameters["message"].as_str().unwrap_or("");
        Ok(RawToolOutput::text(format!(
            "[NOTIFICATION] Channel: {channel}\nMessage: {message}\n\nStatus: Notification tool requires configuration.\n\nTo configure:\n  1. Set NOTIFICATION_TYPE (slack | dingtalk | email | wechat)\n  2. Set NOTIFICATION_WEBHOOK_URL or API credentials"
        )))
    }
}

pub fn notification_send_tool() -> Arc<dyn Tool> { Arc::new(NotificationSendTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = NotificationSendTool.execute(&ToolRequest::new("builtin/notification.send@1.0.0", serde_json::json!({"channel": "alerts", "message": "test"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("NOTIFICATION")), _ => panic!("expected text") };
    }
}