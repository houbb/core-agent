use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `security.scan` — Scan code for security vulnerabilities.
/// Stub — requires Semgrep/SonarQube/Snyk configuration.
pub struct SecurityScanTool;

#[async_trait]
impl Tool for SecurityScanTool {
    fn key(&self) -> &str { "builtin/security.scan@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let path = request.parameters["path"].as_str().unwrap_or(".");
        let severity = request.parameters["severity"].as_str().unwrap_or("all");
        Ok(RawToolOutput::text(format!(
            "[SECURITY_SCAN] Path: {path}, Severity: {severity}\n\nStatus: Security scan requires Semgrep/SonarQube/Snyk configuration.\n\nTo configure:\n  1. Set SECURITY_SCAN_ENGINE (semgrep | sonarqube | snyk)\n  2. Provide API endpoint and credentials"
        )))
    }
}

pub fn security_scan_tool() -> Arc<dyn Tool> { Arc::new(SecurityScanTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = SecurityScanTool.execute(&ToolRequest::new("builtin/security.scan@1.0.0", serde_json::json!({"path": "."})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("SECURITY_SCAN")), _ => panic!("expected text") };
    }
}