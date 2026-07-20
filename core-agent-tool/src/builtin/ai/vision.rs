use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `vision.analyze` — Analyze images/screenshots using vision models.
/// Stub — requires multi-modal model configuration.
pub struct VisionAnalyzeTool;

#[async_trait]
impl Tool for VisionAnalyzeTool {
    fn key(&self) -> &str { "builtin/vision.analyze@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let image_path = request.parameters["image"].as_str().unwrap_or("");
        let prompt = request.parameters["prompt"].as_str().unwrap_or("Describe this image");
        Ok(RawToolOutput::text(format!(
            "[VISION] Image: {image_path}, Prompt: {prompt}\n\nStatus: Vision analysis requires multi-modal model configuration.\n\nTo configure:\n  1. Set VISION_MODEL_ENDPOINT\n  2. Set VISION_MODEL_TYPE (gpt-4-vision | claude-vision | local)"
        )))
    }
}

pub fn vision_analyze_tool() -> Arc<dyn Tool> { Arc::new(VisionAnalyzeTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = VisionAnalyzeTool.execute(&ToolRequest::new("builtin/vision.analyze@1.0.0", serde_json::json!({"image": "screenshot.png"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("VISION")), _ => panic!("expected text") };
    }
}