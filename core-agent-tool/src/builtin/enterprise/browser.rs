use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `browser.navigate` — Navigate to a URL using browser automation.
/// Stub — requires Playwright or browser driver configuration.
pub struct BrowserNavigateTool;

#[async_trait]
impl Tool for BrowserNavigateTool {
    fn key(&self) -> &str { "builtin/browser.navigate@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let url = request.parameters["url"].as_str().unwrap_or("");
        Ok(RawToolOutput::text(format!(
            "[BROWSER] Navigate to: {url}\n\nStatus: Browser tool requires Playwright or browser driver configuration.\n\nTo configure:\n  1. Install Playwright: npm install playwright\n  2. Set BROWSER_DRIVER environment variable"
        )))
    }
}

/// `browser.screenshot` — Take a screenshot of a page.
/// Stub — requires Playwright or browser driver configuration.
pub struct BrowserScreenshotTool;

#[async_trait]
impl Tool for BrowserScreenshotTool {
    fn key(&self) -> &str { "builtin/browser.screenshot@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let url = request.parameters["url"].as_str().unwrap_or("");
        Ok(RawToolOutput::text(format!(
            "[BROWSER] Screenshot of: {url}\n\nStatus: Browser screenshot tool requires Playwright or browser driver configuration.\n\nTo configure:\n  1. Install Playwright: npm install playwright\n  2. Set BROWSER_DRIVER environment variable"
        )))
    }
}

pub fn browser_navigate_tool() -> Arc<dyn Tool> { Arc::new(BrowserNavigateTool) }
pub fn browser_screenshot_tool() -> Arc<dyn Tool> { Arc::new(BrowserScreenshotTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = BrowserNavigateTool.execute(&ToolRequest::new("builtin/browser.navigate@1.0.0", serde_json::json!({"url": "https://example.com"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("BROWSER")), _ => panic!("expected text") };
    }
}