use std::sync::Arc;
use async_trait::async_trait;
use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

/// `k8s.query` — Query Kubernetes resources.
/// Stub — requires kubectl or kubeconfig configuration.
pub struct K8sQueryTool;

#[async_trait]
impl Tool for K8sQueryTool {
    fn key(&self) -> &str { "builtin/k8s.query@1.0.0" }
    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let resource = request.parameters["resource"].as_str().unwrap_or("pods");
        let namespace = request.parameters["namespace"].as_str().unwrap_or("default");
        Ok(RawToolOutput::text(format!(
            "[K8S_QUERY] Resource: {resource}, Namespace: {namespace}\n\nStatus: Kubernetes query tool requires kubectl access.\n\nTo configure:\n  1. Ensure kubectl is installed and configured\n  2. Set KUBECONFIG environment variable if needed\n  3. Supported resources: pods, deployments, services, nodes, events, logs"
        )))
    }
}

pub fn k8s_query_tool() -> Arc<dyn Tool> { Arc::new(K8sQueryTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    #[tokio::test]
    async fn stub_returns_config_message() {
        let result = K8sQueryTool.execute(&ToolRequest::new("builtin/k8s.query@1.0.0", serde_json::json!({"resource": "pods"})), &ToolContext::default()).await.unwrap();
        match &result.content[0] { crate::domain::ToolContent::Text(t) => assert!(t.contains("K8S_QUERY")), _ => panic!("expected text") };
    }
}