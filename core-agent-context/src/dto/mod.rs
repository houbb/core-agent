//! DTO 层 — 输入输出数据传输对象
//!
//! 隔离领域模型与外部 API，保证领域模型可以独立演进。

use serde::{Deserialize, Serialize};

// ── BuildContext DTOs ──

/// 构建 Context 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildContextRequest {
    /// Session ID
    pub session_id: String,
    /// Conversation ID（可选，不指定则取 MAIN Conversation）
    pub conversation_id: Option<String>,
    /// 系统提示（可选）
    pub system_prompt: Option<String>,
    /// 当前用户输入（可选）
    pub user_input: Option<String>,
    /// 最大消息数（可选，默认 20）
    pub max_messages: Option<usize>,
    /// 最大 Token 预算（可选，默认 128000）
    pub max_tokens: Option<u64>,
    /// 压缩策略（recent-window / extractive-summary）
    #[serde(default)]
    pub compression_strategy: Option<String>,
    /// 压缩触发阈值百分比（默认 80）
    #[serde(default)]
    pub compression_trigger_percent: Option<u8>,
    /// 工作目录（可选）
    pub working_directory: Option<String>,
}

/// Context 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextResponse {
    /// 完整结构化 Context，供后续 Runtime 与 Inspector 使用
    pub context: crate::domain::Context,
    /// Context ID
    pub id: String,
    /// Session ID
    pub session_id: String,
    /// Conversation ID
    pub conversation_id: Option<String>,
    /// 总 Token 数
    pub total_tokens: u64,
    /// Token 分布
    pub token_distribution: TokenDistributionResponse,
    /// 构建时间（ISO 8601）
    pub built_at: String,
    /// SHA-256 哈希
    pub hash: String,
    /// 构建耗时（毫秒）
    pub build_duration_ms: u64,
    /// 子 Context 摘要
    pub system: SystemContextSummary,
    pub conversation: ConversationContextSummary,
    pub environment: EnvironmentContextSummary,
    pub user: UserContextSummary,
}

/// Token 分布响应
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenDistributionResponse {
    pub system: u64,
    pub conversation: u64,
    pub workspace: u64,
    pub memory: u64,
    pub environment: u64,
    pub plugin: u64,
    pub tool: u64,
    pub user: u64,
}

/// 系统上下文摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemContextSummary {
    pub prompt_len: usize,
    pub capabilities_count: usize,
}

/// 对话上下文摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationContextSummary {
    pub message_count: usize,
    pub total_count: usize,
    pub has_summary: bool,
}

/// 环境上下文摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentContextSummary {
    pub os: Option<String>,
    pub working_directory: Option<String>,
    pub git_branch: Option<String>,
}

/// 用户上下文摘要
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContextSummary {
    pub has_input: bool,
    pub attachments_count: usize,
}

// ── Snapshot DTOs ──

/// Snapshot 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshotResponse {
    /// 快照 ID
    pub id: String,
    /// Session ID
    pub session_id: String,
    /// Conversation ID
    pub conversation_id: Option<String>,
    /// 创建时间（ISO 8601）
    pub created_at: String,
    /// Token 总数
    pub token_count: u64,
    /// SHA-256 哈希
    pub hash: String,
}

// ── 列表响应 ──

/// 分页列表响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListResponse<T> {
    /// 数据列表
    pub items: Vec<T>,
    /// 总数
    pub total: u64,
    /// 偏移量
    pub offset: u64,
    /// 限制数
    pub limit: u64,
}

// ── 转换实现 ──

impl From<&crate::domain::context::Context> for ContextResponse {
    fn from(ctx: &crate::domain::context::Context) -> Self {
        Self {
            context: ctx.clone(),
            id: ctx.id.to_string(),
            session_id: ctx.session_id.to_string(),
            conversation_id: ctx.conversation_id.map(|id| id.to_string()),
            total_tokens: ctx.total_tokens,
            token_distribution: TokenDistributionResponse::from(&ctx.token_distribution),
            built_at: ctx.built_at.to_rfc3339(),
            hash: ctx.hash.clone(),
            build_duration_ms: ctx.build_duration_ms,
            system: SystemContextSummary {
                prompt_len: ctx.system.prompt.as_ref().map(|s| s.len()).unwrap_or(0),
                capabilities_count: ctx.system.capabilities.len(),
            },
            conversation: ConversationContextSummary {
                message_count: ctx.conversation.messages.len(),
                total_count: ctx.conversation.total_count,
                has_summary: ctx.conversation.has_summary,
            },
            environment: EnvironmentContextSummary {
                os: ctx.environment.os.clone(),
                working_directory: ctx.environment.working_directory.clone(),
                git_branch: ctx.environment.git_branch.clone(),
            },
            user: UserContextSummary {
                has_input: ctx.user.current_input.is_some(),
                attachments_count: ctx.user.attachments.len(),
            },
        }
    }
}

impl From<&crate::infrastructure::ContextSnapshotMeta> for ContextSnapshotResponse {
    fn from(meta: &crate::infrastructure::ContextSnapshotMeta) -> Self {
        Self {
            id: meta.id.to_string(),
            session_id: meta.session_id.to_string(),
            conversation_id: meta.conversation_id.map(|id| id.to_string()),
            created_at: meta.created_at.to_rfc3339(),
            token_count: meta.token_count,
            hash: meta.hash.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::context::Context;
    use crate::domain::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn test_context_to_response() {
        let ctx = Context {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            conversation_id: None,
            segments: Vec::new(),
            system: SystemContext::new("You are helpful"),
            conversation: ConversationContext::new(),
            workspace: WorkspaceContext::new(),
            memory: MemoryContext::new(),
            environment: EnvironmentContext::new(),
            plugin: PluginContext::new(),
            tool: ToolContext::new(),
            user: UserContext::new().with_input("Hello"),
            total_tokens: 100,
            token_distribution: TokenDistribution::default(),
            built_at: Utc::now(),
            hash: "abc123".into(),
            build_duration_ms: 42,
        };

        let resp = ContextResponse::from(&ctx);
        assert_eq!(resp.id, ctx.id.to_string());
        assert_eq!(resp.total_tokens, 100);
        assert_eq!(resp.hash, "abc123");
        assert_eq!(resp.system.prompt_len, 15);
        assert!(resp.user.has_input);
    }

    #[test]
    fn test_build_context_request_serialization() {
        let req = BuildContextRequest {
            session_id: "550e8400-e29b-41d4-a716-446655440000".into(),
            conversation_id: None,
            system_prompt: Some("You are an agent.".into()),
            user_input: Some("Hello".into()),
            max_messages: Some(10),
            max_tokens: Some(64000),
            compression_strategy: None,
            compression_trigger_percent: None,
            working_directory: Some("/home/user".into()),
        };

        let json = serde_json::to_string(&req).unwrap();
        let restored: BuildContextRequest = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.session_id, "550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(restored.max_messages, Some(10));
    }
}

/// Read-only, content-free Context occupancy exposed to UI and reducer extensions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextAccessSnapshot {
    pub context_id: String,
    pub total_tokens: u64,
    pub max_tokens: u64,
    pub estimated: bool,
    pub build_duration_ms: u64,
    pub distribution: TokenDistributionResponse,
}

impl ContextAccessSnapshot {
    /// Builds a content-free occupancy view suitable for UI and reducer extensions.
    pub fn from_context(context: &crate::domain::Context, max_tokens: u64) -> Self {
        Self {
            context_id: context.id.to_string(),
            total_tokens: context.total_tokens,
            max_tokens,
            estimated: true,
            build_duration_ms: context.build_duration_ms,
            distribution: TokenDistributionResponse::from(&context.token_distribution),
        }
    }
}

impl From<&crate::domain::TokenDistribution> for TokenDistributionResponse {
    fn from(distribution: &crate::domain::TokenDistribution) -> Self {
        Self {
            system: distribution.system,
            conversation: distribution.conversation,
            workspace: distribution.workspace,
            memory: distribution.memory,
            environment: distribution.environment,
            plugin: distribution.plugin,
            tool: distribution.tool,
            user: distribution.user,
        }
    }
}
