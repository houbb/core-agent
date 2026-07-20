//! DefaultComposer — 默认 Context 组装器
//!
//! 将 Reducer 裁剪后的 segments 按 Slot 分配到 Context 对象。

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::domain::context::{Context, ContextSegment, TokenDistribution};
use crate::domain::context_reference::ContextReference;
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
        mut segments: Vec<ContextSegment>,
    ) -> ContextResult<Context> {
        let now = Utc::now();
        let id = Uuid::new_v4();
        let mut dist = TokenDistribution::default();
        let mut total_tokens = 0u64;

        segments.sort_by(|left, right| {
            if left.slot == ContextSlot::Conversation
                && right.slot == ContextSlot::Conversation
                && left.priority == right.priority
            {
                conversation_position(left).cmp(&conversation_position(right))
            } else {
                right.priority.cmp(&left.priority).then_with(|| {
                    right
                        .slot
                        .default_priority()
                        .cmp(&left.slot.default_priority())
                })
            }
        });

        let mut system = SystemContext::default();
        let mut conversation = ConversationContext::new();
        let mut workspace = WorkspaceContext::new();
        let mut memory = MemoryContext::new();
        let mut environment = EnvironmentContext::default();
        let mut plugin = PluginContext::new();
        let mut tool = ToolContext::new();
        let mut user = UserContext::new();
        let mut references: Vec<ContextReference> = Vec::new();

        for seg in &segments {
            add_tokens(&mut total_tokens, seg.token_count)?;

            match seg.slot {
                ContextSlot::System => {
                    add_tokens(&mut dist.system, seg.token_count)?;
                    if let Some(s) = seg.content.as_str() {
                        append_text(&mut system.prompt, s);
                    } else {
                        append_json(&mut system.config, &seg.content);
                    }
                }
                ContextSlot::Conversation => {
                    add_tokens(&mut dist.conversation, seg.token_count)?;
                    if seg.metadata.get("conversation_meta").map(String::as_str) == Some("true") {
                        if let Some(total_count) = conversation_total(seg)? {
                            conversation.total_count = total_count;
                        }
                        continue;
                    }
                    // 检查是否是摘要消息
                    if seg.metadata.get("reduced").map(|s| s.as_str()) == Some("true") {
                        conversation.has_summary = true;
                        let summary = seg.content.as_str().ok_or_else(|| {
                            crate::error::ContextError::InvalidArgument(
                                "reduced conversation segment must contain text".into(),
                            )
                        })?;
                        append_text(&mut conversation.summary, summary);
                    } else {
                        // 正常消息
                        let role = seg
                            .content
                            .get("role")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                crate::error::ContextError::InvalidArgument(
                                    "conversation segment is missing role".into(),
                                )
                            })?
                            .to_string();
                        let content = seg
                            .content
                            .get("content")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                crate::error::ContextError::InvalidArgument(
                                    "conversation segment is missing content".into(),
                                )
                            })?
                            .to_string();
                        let created_at = seg
                            .content
                            .get("created_at")
                            .and_then(|v| v.as_str())
                            .ok_or_else(|| {
                                crate::error::ContextError::InvalidArgument(
                                    "conversation segment is missing created_at".into(),
                                )
                            })?
                            .to_string();
                        chrono::DateTime::parse_from_rfc3339(&created_at).map_err(|_| {
                            crate::error::ContextError::InvalidArgument(
                                "conversation segment has invalid created_at".into(),
                            )
                        })?;
                        let msg_id = seg
                            .metadata
                            .get("message_id")
                            .cloned()
                            .or_else(|| {
                                seg.content
                                    .get("id")
                                    .and_then(|value| value.as_str())
                                    .map(str::to_owned)
                            })
                            .ok_or_else(|| {
                                crate::error::ContextError::InvalidArgument(
                                    "conversation segment is missing message_id".into(),
                                )
                            })?;

                        let ctx_msg = ContextMessage {
                            id: msg_id,
                            role,
                            content,
                            token_count: seg.token_count,
                            created_at,
                        };
                        conversation.messages.push(ctx_msg);
                        conversation.total_count = conversation.messages.len();
                    }
                    if let Some(total_count) = conversation_total(seg)? {
                        conversation.total_count = conversation.total_count.max(total_count);
                    }
                }
                ContextSlot::Workspace => {
                    add_tokens(&mut dist.workspace, seg.token_count)?;
                    workspace.enabled = true;
                    if let Some(root_path) = seg.content.get("root_path").and_then(|v| v.as_str()) {
                        workspace.root_path = Some(root_path.to_owned());
                    }
                    append_json(&mut workspace.content, &seg.content);
                }
                ContextSlot::Memory => {
                    add_tokens(&mut dist.memory, seg.token_count)?;
                    memory.enabled = true;
                    append_json(&mut memory.content, &seg.content);
                }
                ContextSlot::Environment => {
                    add_tokens(&mut dist.environment, seg.token_count)?;
                    if let Some(os) = seg.content.get("os").and_then(|v| v.as_str()) {
                        environment.os = Some(os.to_string());
                    }
                    if let Some(version) = seg.content.get("os_version").and_then(|v| v.as_str()) {
                        environment.os_version = Some(version.to_owned());
                    }
                    if let Some(shell) = seg.content.get("shell").and_then(|v| v.as_str()) {
                        environment.shell = Some(shell.to_owned());
                    }
                    if let Some(wd) = seg
                        .content
                        .get("working_directory")
                        .and_then(|v| v.as_str())
                    {
                        environment.working_directory = Some(wd.to_string());
                    }
                    if let Some(branch) = seg.content.get("git_branch").and_then(|v| v.as_str()) {
                        environment.git_branch = Some(branch.to_string());
                    }
                    if let Some(root) = seg.content.get("git_root").and_then(|v| v.as_str()) {
                        environment.git_root = Some(root.to_string());
                    }
                    append_json(&mut environment.extra, &seg.content);
                }
                ContextSlot::Tool => {
                    add_tokens(&mut dist.tool, seg.token_count)?;
                    tool.enabled = true;
                    append_json(&mut tool.content, &seg.content);
                }
                ContextSlot::Plugin => {
                    add_tokens(&mut dist.plugin, seg.token_count)?;
                    plugin.enabled = true;
                    append_json(&mut plugin.content, &seg.content);
                }
                ContextSlot::User => {
                    add_tokens(&mut dist.user, seg.token_count)?;
                    if let Some(input) = seg.content.as_str() {
                        append_text(&mut user.current_input, input);
                    } else {
                        if let Some(input) =
                            seg.content.get("current_input").and_then(|v| v.as_str())
                        {
                            append_text(&mut user.current_input, input);
                        }
                        if let Some(attachments) =
                            seg.content.get("attachments").and_then(|v| v.as_array())
                        {
                            for attachment in attachments {
                                let attachment = attachment.as_str().ok_or_else(|| {
                                    crate::error::ContextError::InvalidArgument(
                                        "user attachment must be a string".into(),
                                    )
                                })?;
                                user.attachments.push(attachment.to_owned());
                            }
                        }
                        append_json(&mut user.extra, &seg.content);
                    }
                }
                ContextSlot::Reference => {
                    add_tokens(&mut dist.reference, seg.token_count)?;
                    // 尝试从 content 中反序列化 ContextReference
                    if let Ok(r) = serde_json::from_value::<ContextReference>(seg.content.clone()) {
                        references.push(r);
                    }
                }
            }
        }

        let mut context = Context {
            id,
            session_id,
            conversation_id,
            segments,
            system,
            conversation,
            workspace,
            memory,
            environment,
            plugin,
            tool,
            user,
            references,
            total_tokens,
            token_distribution: dist,
            built_at: now,
            hash: String::new(),
            build_duration_ms: 0,
        };
        context
            .refresh_hash()
            .map_err(|error| crate::error::ContextError::Serialization(error.to_string()))?;
        Ok(context)
    }
}

fn append_text(target: &mut Option<String>, value: &str) {
    match target {
        Some(existing) if !existing.is_empty() => {
            existing.push_str("\n\n");
            existing.push_str(value);
        }
        _ => *target = Some(value.to_owned()),
    }
}

fn conversation_position(segment: &ContextSegment) -> i64 {
    segment
        .metadata
        .get("message_index")
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(i64::from(segment.priority))
}

fn conversation_total(segment: &ContextSegment) -> ContextResult<Option<usize>> {
    segment
        .metadata
        .get("conversation_total")
        .map(|value| {
            value.parse::<usize>().map_err(|_| {
                crate::error::ContextError::InvalidArgument(
                    "conversation_total metadata must be a non-negative integer".into(),
                )
            })
        })
        .transpose()
}

fn add_tokens(target: &mut u64, value: u64) -> ContextResult<()> {
    *target = target
        .checked_add(value)
        .ok_or_else(|| crate::error::ContextError::Internal("token count overflow".into()))?;
    Ok(())
}

fn append_json(target: &mut serde_json::Value, value: &serde_json::Value) {
    if !target.is_array() {
        *target = serde_json::Value::Array(Vec::new());
    }
    if let Some(items) = target.as_array_mut() {
        items.push(value.clone());
    }
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

    #[tokio::test]
    async fn test_default_composer_preserves_extension_slots() {
        let composer = DefaultComposer::new();
        let session_id = Uuid::new_v4();
        let segments = [
            (ContextSource::Workspace, ContextSlot::Workspace),
            (ContextSource::Memory, ContextSlot::Memory),
            (ContextSource::Tool, ContextSlot::Tool),
            (ContextSource::Plugin, ContextSlot::Plugin),
        ]
        .into_iter()
        .map(|(source, slot)| {
            ContextSegment::new(
                source,
                slot,
                serde_json::json!({"slot": slot.as_str()}),
                3,
                slot.default_priority(),
            )
        })
        .collect();

        let context = composer.compose(session_id, None, segments).await.unwrap();

        assert!(context.workspace.enabled);
        assert!(context.memory.enabled);
        assert!(context.tool.enabled);
        assert!(context.plugin.enabled);
        assert_eq!(context.token_distribution.total(), 12);
        assert_eq!(context.segments.len(), 4);
        assert!(context
            .segments
            .iter()
            .any(|segment| segment.source == ContextSource::Tool));
    }

    #[tokio::test]
    async fn test_semantic_hash_ignores_build_identity() {
        let composer = DefaultComposer::new();
        let session_id = Uuid::new_v4();
        let segment = ContextSegment::new(
            ContextSource::User,
            ContextSlot::User,
            serde_json::Value::String("same input".into()),
            3,
            ContextSlot::User.default_priority(),
        );

        let first = composer
            .compose(session_id, None, vec![segment.clone()])
            .await
            .unwrap();
        let second = composer
            .compose(session_id, None, vec![segment])
            .await
            .unwrap();

        assert_ne!(first.id, second.id);
        assert_eq!(first.hash, second.hash);
    }

    #[tokio::test]
    async fn test_invalid_conversation_segment_is_rejected() {
        let segment = ContextSegment::new(
            ContextSource::Conversation,
            ContextSlot::Conversation,
            serde_json::json!({"role": "USER"}),
            1,
            ContextSlot::Conversation.default_priority(),
        );

        assert!(DefaultComposer::new()
            .compose(Uuid::new_v4(), None, vec![segment])
            .await
            .is_err());
    }

    #[tokio::test]
    async fn test_semantic_hash_canonicalizes_segment_metadata() {
        let session_id = Uuid::new_v4();
        let first_segment = ContextSegment::new(
            ContextSource::Workspace,
            ContextSlot::Workspace,
            serde_json::json!({"path": "README.md"}),
            2,
            80,
        )
        .with_meta("a", "1")
        .with_meta("b", "2");
        let second_segment = ContextSegment::new(
            ContextSource::Workspace,
            ContextSlot::Workspace,
            serde_json::json!({"path": "README.md"}),
            2,
            80,
        )
        .with_meta("b", "2")
        .with_meta("a", "1");

        let first = DefaultComposer::new()
            .compose(session_id, None, vec![first_segment])
            .await
            .unwrap();
        let second = DefaultComposer::new()
            .compose(session_id, None, vec![second_segment])
            .await
            .unwrap();

        assert_eq!(first.hash, second.hash);
    }
}
