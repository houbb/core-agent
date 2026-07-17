//! Context 实体体系
//!
//! Context 不等于 Prompt。Context 是结构化的上下文数据，
//! 由多个 ContextSegment 组成，最终被 Composer 组装为完整 Context。
//!
//! Context = User Input + Conversation + Workspace + Memory
//!         + System Prompt + Environment + Plugin Context + Tool Result

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::slot::ContextSlot;

/// Context 来源枚举
///
/// 每一个 ContextSegment 都有明确的来源，用于 Debug 和 Trace。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ContextSource {
    /// 系统来源
    System,
    /// 用户来源
    User,
    /// 插件来源
    Plugin,
    /// 工作空间来源
    Workspace,
    /// 记忆来源
    Memory,
    /// 工具来源
    Tool,
    /// 环境来源
    Environment,
}

impl ContextSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            ContextSource::System => "SYSTEM",
            ContextSource::User => "USER",
            ContextSource::Plugin => "PLUGIN",
            ContextSource::Workspace => "WORKSPACE",
            ContextSource::Memory => "MEMORY",
            ContextSource::Tool => "TOOL",
            ContextSource::Environment => "ENVIRONMENT",
        }
    }
}

/// ContextSegment — 上下文片段
///
/// 每个 Provider 的 collect() 返回一个或多个 ContextSegment。
/// Composer 将它们组装成最终的 Context 对象。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSegment {
    /// 数据来源
    pub source: ContextSource,
    /// 所属 Slot
    pub slot: ContextSlot,
    /// 结构化内容（JSON Value）
    pub content: serde_json::Value,
    /// 估算 Token 数
    pub token_count: u64,
    /// 排序优先级（越大越靠前）
    pub priority: i32,
    /// 是否必须保留（不可被 Reducer 裁剪）
    pub required: bool,
    /// 附加元数据
    pub metadata: std::collections::HashMap<String, String>,
}

impl ContextSegment {
    /// 创建新的 ContextSegment
    pub fn new(
        source: ContextSource,
        slot: ContextSlot,
        content: serde_json::Value,
        token_count: u64,
        priority: i32,
    ) -> Self {
        Self {
            source,
            slot,
            content,
            token_count,
            priority,
            required: false,
            metadata: std::collections::HashMap::new(),
        }
    }

    /// 标记为必须保留
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// 添加元数据键值对
    pub fn with_meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }
}

/// Context — 最终产出的完整上下文
///
/// 由 Composer 组装多个 ContextSegment 后生成。
/// 这是 Context Runtime 的最终产出物，后续 Phase 2 Model Runtime 据此构建 Prompt。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    /// 上下文 ID（每次构建唯一）
    pub id: Uuid,
    /// 所属 Session ID
    pub session_id: Uuid,
    /// 关联 Conversation ID（可选）
    pub conversation_id: Option<Uuid>,
    /// 系统上下文
    pub system: super::system_context::SystemContext,
    /// 对话上下文
    pub conversation: super::conversation_context::ConversationContext,
    /// 工作空间上下文
    pub workspace: super::workspace_context::WorkspaceContext,
    /// 记忆上下文
    pub memory: super::memory_context::MemoryContext,
    /// 环境上下文
    pub environment: super::environment_context::EnvironmentContext,
    /// 插件上下文
    pub plugin: super::plugin_context::PluginContext,
    /// 用户上下文
    pub user: super::user_context::UserContext,
    /// 总 Token 数
    pub total_tokens: u64,
    /// 各 Slot 的 Token 分布
    pub token_distribution: TokenDistribution,
    /// 构建时间
    pub built_at: DateTime<Utc>,
    /// Context 哈希（SHA-256），用于完整性校验和去重
    pub hash: String,
    /// 构建耗时（毫秒）
    pub build_duration_ms: u64,
}

/// Token 分布统计
///
/// 用于 UX 中的 Token 可视化。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenDistribution {
    pub system: u64,
    pub conversation: u64,
    pub workspace: u64,
    pub memory: u64,
    pub environment: u64,
    pub plugin: u64,
    pub tool: u64,
    pub user: u64,
}

impl TokenDistribution {
    /// 计算各 Slot 占比
    pub fn percentage(&self, slot_tokens: u64) -> f64 {
        if self.total() == 0 {
            return 0.0;
        }
        (slot_tokens as f64 / self.total() as f64) * 100.0
    }

    /// 总 Token 数
    pub fn total(&self) -> u64 {
        self.system
            + self.conversation
            + self.workspace
            + self.memory
            + self.environment
            + self.plugin
            + self.tool
            + self.user
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_segment_new() {
        let seg = ContextSegment::new(
            ContextSource::User,
            ContextSlot::User,
            serde_json::Value::String("hello".into()),
            2,
            30,
        );
        assert_eq!(seg.source, ContextSource::User);
        assert_eq!(seg.slot, ContextSlot::User);
        assert_eq!(seg.token_count, 2);
        assert!(!seg.required);
    }

    #[test]
    fn test_context_segment_required() {
        let seg = ContextSegment::new(
            ContextSource::System,
            ContextSlot::System,
            serde_json::Value::String("system".into()),
            5,
            100,
        )
        .required();
        assert!(seg.required);
    }

    #[test]
    fn test_context_segment_with_meta() {
        let seg = ContextSegment::new(
            ContextSource::User,
            ContextSlot::User,
            serde_json::Value::Null,
            0,
            0,
        )
        .with_meta("key", "value");
        assert_eq!(seg.metadata.get("key").unwrap(), "value");
    }

    #[test]
    fn test_token_distribution_total() {
        let dist = TokenDistribution {
            system: 100,
            conversation: 200,
            workspace: 50,
            ..Default::default()
        };
        assert_eq!(dist.total(), 350);
    }

    #[test]
    fn test_token_distribution_percentage() {
        let dist = TokenDistribution {
            system: 50,
            conversation: 50,
            ..Default::default()
        };
        assert!((dist.percentage(50) - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_context_source_as_str() {
        assert_eq!(ContextSource::System.as_str(), "SYSTEM");
        assert_eq!(ContextSource::User.as_str(), "USER");
        assert_eq!(ContextSource::Workspace.as_str(), "WORKSPACE");
    }
}