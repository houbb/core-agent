use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// `web.fetch` — Fetch a URL and return its content as text.
pub struct WebFetchTool;

#[async_trait]
impl Tool for WebFetchTool {
    fn key(&self) -> &str {
        "builtin/web.fetch@1.0.0"
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        _context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        let url = request.parameters["url"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("url is required".into()))?;
        if url.is_empty() {
            return Err(ToolError::InvalidArgument("url must not be empty".into()));
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .user_agent("core-agent/1.0")
            .build()
            .map_err(|e| ToolError::execution("web.fetch", format!("failed to create client: {e}"), false))?;

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| ToolError::execution("web.fetch", format!("request failed: {e}"), true))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| ToolError::execution("web.fetch", format!("failed to read body: {e}"), false))?;

        Ok(RawToolOutput::text(format!(
            "Status: {status}\n\n{text}",
            text = &text[..text.len().min(100_000)]
        )))
    }
}

pub fn web_fetch_tool() -> Arc<dyn Tool> {
    Arc::new(WebFetchTool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;

    #[tokio::test]
    async fn rejects_empty_url() {
        let tool = WebFetchTool;
        let request = ToolRequest::new(
            "builtin/web.fetch@1.0.0",
            serde_json::json!({"url": ""}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await;
        assert!(result.is_err());
    }
}