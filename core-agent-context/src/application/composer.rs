//! DefaultComposer — 默认 Context 组装器
//!
//! 将 Reducer 裁剪后的 segments 按 Slot 分配到 Context 对象。

use async_trait::async_trait;
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::context::{Context, ContextSegment, TokenDistribution};
use crate::domain::slot::ContextSlot;
use crate::domain::*;
use crate::error::ContextResult;
use crate::infrastructure::ContextComposer;

/// DefaultComposer
pub struct DefaultComposer;

impl DefaultComposer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultComposer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextComposer for DefaultComposer {
    fn name(&self) -> &str {
        "default-composer"
    }

    async fn compose(
        &self,
        session_id: Uuid,
        conversation_id: Option<Uuid>,
        segments: Vec<ContextSegment>,
    ) -> ContextResult<Context> {
        let now = Utc::now();
        let id = Uuid::new_v4();
        let mut dist = TokenDistribution::default();
        let mut total_tokens = 0u64;

        let mut system = SystemContext::new("");
        let mut conversation = ConversationContext::new();
        let workspace = WorkspaceContext::new();
        let memory = MemoryContext::new();
        let mut environment = EnvironmentContext::new();
        let plugin = PluginContext::new();
        let mut user = UserContext::new();

        for seg in &segments {
            total_tokens += seg.token_count;

            match seg.slot {
                ContextSlot::System => {
                    dist.system += seg.token_count;
                    if let Some(s) = seg.content.as_str() {
                        system.prompt = Some(s.to_string());
                    }
                }
                ContextSlot::Conversation => {
                    dist.conversation += seg.token_count;
                    // 检查是否是摘要消息
                    if seg.metadata.get("reduced").map(|s| s.as_str()) == Some("true") {
                        conversation.has_summary = true;
                        if let Some(summary) = seg.content.as_str() {
                            conversation.summary = Some(summary.to_string());
                        }
                    } else {
                        // 正常消息
                        let role = seg
                            .content
                            .get("role")
                            .and_then(|v| v.as_str())
                            .unwrap_or("USER")
                            .to_string();
                        let content = seg
                            .content
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let created_at = seg
                            .content
                            .get("created_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let msg_id = seg
                            .metadata
                            .get("message_id")
                            .cloned()
                            .unwrap_or_else(|| Uuid::new_v4().to_string());

                        let ctx_msg = ContextMessage {
                            id: msg_id,
                            role,
                            content,
                            token_count: seg.token_count,
                            created_at,
                        };
                        conversation.add_message(ctx_msg);
                    }
                }
                ContextSlot::Workspace => {
                    dist.workspace += seg.token_count;
                }
                ContextSlot::Memory => {
                    dist.memory += seg.token_count;
                }
                ContextSlot::Environment => {
                    dist.environment += seg.token_count;
                    if let Some(os) = seg.content.get("os").and_then(|v| v.as_str()) {
                        environment.os = Some(os.to_string());
                    }
                    if let Some(wd) = seg
                        .content
                        .get("working_directory")
                        .and_then(|v| v.as_str())
                    {
                        environment.working_directory = Some(wd.to_string());
                    }
                    if let Some(branch) = seg
                        .content
                        .get("git_branch")
                        .and_then(|v| v.as_str())
                    {
                        environment.git_branch = Some(branch.to_string());
                    }
                    if let Some(root) = seg.content.get("git_root").and_then(|v| v.as_str()) {
                        environment.git_root = Some(root.to_string());
                    }
                }
                ContextSlot::Tool => {
                    dist.tool += seg.token_count;
                }
                ContextSlot::Plugin => {
                    dist.plugin += seg.token_count;
                }
                ContextSlot::User => {
                    dist.user += seg.token_count;
                    if let Some(input) = seg.content.as_str() {
                        user.current_input = Some(input.to_string());
                    }
                }
            }
        }

        // 计算哈希
        let hash = compute_context_hash(session_id, conversation_id, total_tokens, &dist, now);

        Ok(Context {
            id,
            session_id,
            conversation_id,
            system,
            conversation,
            workspace,
            memory,
            environment,
            plugin,
            user,
            total_tokens,
            token_distribution: dist,
            built_at: now,
            hash,
            build_duration_ms: 0,
        })
    }
}

/// 计算 Context 哈希（SHA-256）
fn compute_context_hash(
    session_id: Uuid,
    conversation_id: Option<Uuid>,
    total_tokens: u64,
    dist: &TokenDistribution,
    now: chrono::DateTime<Utc>,
) -> String {
    let payload = serde_json::json!({
        "session_id": session_id.to_string(),
        "conversation_id": conversation_id.map(|id| id.to_string()),
        "total_tokens": total_tokens,
        "distribution": {
            "system": dist.system,
            "conversation": dist.conversation,
            "workspace": dist.workspace,
            "memory": dist.memory,
            "environment": dist.environment,
            "plugin": dist.plugin,
            "user": dist.user,
        },
        "built_at": now.to_rfc3339(),
    });

    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    let digest = Sha256::digest(payload_str.as_bytes());
    digest
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::context::ContextSource;
    use crate::domain::slot::TokenCounter;

    #[tokio::test]
    async fn test_default_composer_empty() {
        let composer = DefaultComposer::new();
        let session_id = Uuid::new_v4();
        let ctx = composer.compose(session_id, None, vec![]).await.unwrap();

        assert_eq!(ctx.session_id, session_id);
        assert_eq!(ctx.total_tokens, 0);
        assert!(!ctx.hash.is_empty());
    }

    #[tokio::test]
    async fn test_default_composer_with_segments() {
        let composer = DefaultComposer::new();
        let session_id = Uuid::new_v4();

        let system_seg = ContextSegment::new(
            ContextSource::System,
            ContextSlot::System,
            serde_json::Value::String("You are helpful".into()),
            TokenCounter::estimate("You are helpful"),
            100,
        );

        let user_seg = ContextSegment::new(
            ContextSource::User,
            ContextSlot::User,
            serde_json::Value::String("Hello".into()),
            TokenCounter::estimate("Hello"),
            30,
        );

        let ctx = composer
            .compose(session_id, None, vec![system_seg, user_seg])
            .await
            .unwrap();

        assert!(ctx.total_tokens > 0);
        assert!(ctx.system.prompt.is_some());
        assert!(ctx.user.current_input.is_some());
    }
}