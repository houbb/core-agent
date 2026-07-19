use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::ToolRuntimeResult;
use crate::infrastructure::{Tool, ToolContext};

type FunctionFuture = Pin<Box<dyn Future<Output = ToolRuntimeResult<RawToolOutput>> + Send>>;
type FunctionHandler = dyn Fn(ToolRequest, ToolContext) -> FunctionFuture + Send + Sync;

/// Safe adapter for implementing Builtin Tools without coupling the Runtime to
/// filesystem, terminal, browser or other later-phase capabilities.
pub struct FunctionTool {
    key: String,
    handler: Arc<FunctionHandler>,
}

impl FunctionTool {
    pub fn new<F, Fut>(key: impl Into<String>, handler: F) -> Self
    where
        F: Fn(ToolRequest, ToolContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ToolRuntimeResult<RawToolOutput>> + Send + 'static,
    {
        Self {
            key: key.into(),
            handler: Arc::new(move |request, context| Box::pin(handler(request, context))),
        }
    }
}

#[async_trait]
impl Tool for FunctionTool {
    fn key(&self) -> &str {
        &self.key
    }

    async fn execute(
        &self,
        request: &ToolRequest,
        context: &ToolContext,
    ) -> ToolRuntimeResult<RawToolOutput> {
        (self.handler)(request.clone(), context.clone()).await
    }
}
