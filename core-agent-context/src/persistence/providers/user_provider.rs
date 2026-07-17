//! UserProvider — 用户输入上下文提供者
//!
//! 将当前用户输入转换为 ContextSegment。

use async_trait::async_trait;

use crate::domain::context::{ContextSegment, ContextSource};
use crate::domain::slot::{ContextSlot, TokenCounter};
use crate::error::ContextResult;
use crate::infrastructure::{ContextProvider, ProviderContext};

/// UserProvider
///
/// 从 ProviderContext 的 extensions 中读取当前用户输入。
pub struct UserProvider;

impl UserProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UserProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextProvider for UserProvider {
    fn name(&self) -> &str {
        "user-provider"
    }

    fn source(&self) -> ContextSource {
        ContextSource::User
    }

    fn slot(&self) -> ContextSlot {
        ContextSlot::User
    }

    async fn collect(&self, ctx: &ProviderContext) -> ContextResult<Vec<ContextSegment>> {
        // 从 extensions 中读取 user_input
        let user_input = match ctx.extensions.get("user_input") {
            Some(val) => val.as_str().unwrap_or("").to_string(),
            None => return Ok(Vec::new()),
        };

        if user_input.is_empty() {
            return Ok(Vec::new());
        }

        let token_count = TokenCounter::estimate(&user_input);
        let segment = ContextSegment::new(
            ContextSource::User,
            ContextSlot::User,
            serde_json::Value::String(user_input),
            token_count,
            ContextSlot::User.default_priority(),
        )
        .required(); // 用户输入不可裁剪

        Ok(vec![segment])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use core_agent_session::SqliteSessionStore;

    #[tokio::test]
    async fn test_user_provider_collect() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();

        let mut extensions = HashMap::new();
        extensions.insert(
            "user_input".to_string(),
            serde_json::Value::String("Hello, agent!".to_string()),
        );

        let ctx = ProviderContext {
            session_id,
            conversation_id: None,
            session_store: store,
            system_prompt: None,
            working_directory: None,
            max_messages: None,
            extensions,
        };

        let provider = UserProvider::new();
        let segments = provider.collect(&ctx).await.unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].source, ContextSource::User);
        assert_eq!(segments[0].slot, ContextSlot::User);
        assert!(segments[0].required);
    }

    #[tokio::test]
    async fn test_user_provider_empty_when_no_input() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();
        let ctx = ProviderContext::new(session_id, store);

        let provider = UserProvider::new();
        let segments = provider.collect(&ctx).await.unwrap();

        assert!(segments.is_empty());
    }
}