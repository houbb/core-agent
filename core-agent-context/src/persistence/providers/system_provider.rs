//! SystemProvider — 系统提示上下文提供者
//!
//! 将外部注入的系统提示转换为 ContextSegment。

use async_trait::async_trait;

use crate::domain::context::{ContextSegment, ContextSource};
use crate::domain::slot::{ContextSlot, TokenCounter};
use crate::error::ContextResult;
use crate::infrastructure::{ContextProvider, ProviderContext};

/// SystemProvider
///
/// 从 ProviderContext 中读取 system_prompt，生成 System Slot 的 segment。
pub struct SystemProvider;

impl SystemProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextProvider for SystemProvider {
    fn name(&self) -> &str {
        "system-provider"
    }

    fn source(&self) -> ContextSource {
        ContextSource::System
    }

    fn slot(&self) -> ContextSlot {
        ContextSlot::System
    }

    async fn collect(&self, ctx: &ProviderContext) -> ContextResult<Vec<ContextSegment>> {
        let prompt = match &ctx.system_prompt {
            Some(p) => p.clone(),
            None => return Ok(Vec::new()),
        };

        let token_count = TokenCounter::estimate(&prompt);
        let segment = ContextSegment::new(
            ContextSource::System,
            ContextSlot::System,
            serde_json::Value::String(prompt),
            token_count,
            ContextSlot::System.default_priority(),
        )
        .required(); // 系统提示不可裁剪

        Ok(vec![segment])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use core_agent_session::SqliteSessionStore;

    #[tokio::test]
    async fn test_system_provider_collect() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();

        let ctx = ProviderContext::new(session_id, store)
            .with_system_prompt("You are a helpful assistant.");

        let provider = SystemProvider::new();
        let segments = provider.collect(&ctx).await.unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].source, ContextSource::System);
        assert_eq!(segments[0].slot, ContextSlot::System);
        assert!(segments[0].required);
    }

    #[tokio::test]
    async fn test_system_provider_empty_when_no_prompt() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();
        let ctx = ProviderContext::new(session_id, store);

        let provider = SystemProvider::new();
        let segments = provider.collect(&ctx).await.unwrap();

        assert!(segments.is_empty());
    }
}