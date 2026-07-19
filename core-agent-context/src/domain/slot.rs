//! ContextSlot — 上下文槽位机制
//!
//! 每个 Slot 独立计算 Token、裁剪、排序、启用/禁用、预算控制。
//! 未来新增 Context 类型（RCA、CMDB 等），只需增加 Slot + Provider，零修改。

use serde::{Deserialize, Serialize};

/// ContextSlot — 上下文槽位枚举
///
/// 每种 Context 先进入固定 Slot，再参与组装。
/// Future-proof：新增 Slot 只需加枚举值 + 对应 Provider。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum ContextSlot {
    /// 系统提示槽位（优先级最高）
    System,
    /// 环境槽位（OS、Git、Shell 信息）
    Environment,
    /// 工作空间槽位（文件、目录结构）
    Workspace,
    /// 记忆槽位（长短期记忆）
    Memory,
    /// 对话槽位（消息历史）
    Conversation,
    /// 工具槽位（工具结果）
    Tool,
    /// 插件槽位（MCP、插件注入）
    Plugin,
    /// 用户输入槽位
    User,
}

impl ContextSlot {
    /// Composer 与 Reducer 使用的稳定 Slot 顺序。
    pub const ORDERED: [ContextSlot; 8] = [
        ContextSlot::System,
        ContextSlot::Environment,
        ContextSlot::Workspace,
        ContextSlot::Memory,
        ContextSlot::Conversation,
        ContextSlot::Tool,
        ContextSlot::Plugin,
        ContextSlot::User,
    ];

    /// 获取 Slot 名称（大写）
    pub fn as_str(&self) -> &'static str {
        match self {
            ContextSlot::System => "SYSTEM",
            ContextSlot::Environment => "ENVIRONMENT",
            ContextSlot::Workspace => "WORKSPACE",
            ContextSlot::Memory => "MEMORY",
            ContextSlot::Conversation => "CONVERSATION",
            ContextSlot::Tool => "TOOL",
            ContextSlot::Plugin => "PLUGIN",
            ContextSlot::User => "USER",
        }
    }

    /// 默认排序优先级（越大越靠前）
    ///
    /// System(100) 最靠前，User(30) 最后面。
    pub fn default_priority(&self) -> i32 {
        match self {
            ContextSlot::System => 100,
            ContextSlot::Environment => 90,
            ContextSlot::Workspace => 80,
            ContextSlot::Memory => 70,
            ContextSlot::Conversation => 60,
            ContextSlot::Tool => 50,
            ContextSlot::Plugin => 40,
            ContextSlot::User => 30,
        }
    }
}

/// SlotConfig — 单个 Slot 的运行时配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotConfig {
    /// Slot 名称
    pub slot: ContextSlot,
    /// 是否启用
    pub enabled: bool,
    /// Token 预算上限（0 = 不限制）
    pub token_budget: u64,
    /// 优先级覆盖（None 使用默认值）
    pub priority: Option<i32>,
    /// 最大保留消息数（仅 Conversation Slot 有效）
    pub max_messages: Option<usize>,
}

impl SlotConfig {
    /// 创建默认启用的 Slot 配置
    pub fn new(slot: ContextSlot) -> Self {
        Self {
            slot,
            enabled: true,
            token_budget: 0,
            priority: None,
            max_messages: None,
        }
    }

    /// 标记为禁用
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// 设置 Token 预算
    pub fn with_budget(mut self, tokens: u64) -> Self {
        self.token_budget = tokens;
        self
    }

    /// 设置最大消息数
    pub fn with_max_messages(mut self, max: usize) -> Self {
        self.max_messages = Some(max);
        self
    }

    /// 覆盖 Slot 优先级。
    pub fn with_priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
        self
    }

    /// 获取实际优先级（优先使用覆盖值）
    pub fn effective_priority(&self) -> i32 {
        self.priority
            .unwrap_or_else(|| self.slot.default_priority())
    }
}

impl Default for SlotConfig {
    fn default() -> Self {
        Self::new(ContextSlot::Conversation)
    }
}

/// TokenCounter — Token 估算工具
///
/// MVP 使用字符估算（4 ASCII chars ≈ 1 token）。
/// 后续可替换为 tiktoken-rs 精确计算。
pub struct TokenCounter;

impl TokenCounter {
    /// 估算文本的 Token 数
    ///
    /// 规则：
    /// - ASCII 字符：4 字符 ≈ 1 token
    /// - 非 ASCII 字符：MVP 按 1 字符 ≈ 1 token
    pub fn estimate(text: &str) -> u64 {
        if text.is_empty() {
            return 0;
        }
        let mut ascii_count = 0u64;
        let mut non_ascii_count = 0u64;
        for ch in text.chars() {
            if ch.is_ascii() {
                ascii_count += 1;
            } else {
                non_ascii_count += 1;
            }
        }
        ascii_count.div_ceil(4) + non_ascii_count
    }

    /// 估算 JSON Value 的 Token 数
    pub fn estimate_json(value: &serde_json::Value) -> u64 {
        let text = value.to_string();
        Self::estimate(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_priority_order() {
        assert!(
            ContextSlot::System.default_priority() > ContextSlot::Environment.default_priority()
        );
        assert!(
            ContextSlot::Environment.default_priority() > ContextSlot::Workspace.default_priority()
        );
        assert!(
            ContextSlot::Conversation.default_priority() > ContextSlot::User.default_priority()
        );
        assert_eq!(ContextSlot::System.default_priority(), 100);
        assert_eq!(ContextSlot::User.default_priority(), 30);
    }

    #[test]
    fn test_token_counter_ascii() {
        let text = "Hello, world!"; // 13 chars ASCII
        let tokens = TokenCounter::estimate(text);
        assert_eq!(tokens, 4);
    }

    #[test]
    fn test_token_counter_chinese() {
        let text = "你好世界"; // 4 CJK chars
        let tokens = TokenCounter::estimate(text);
        assert_eq!(tokens, 4);
    }

    #[test]
    fn test_token_counter_min_one() {
        let text = "a";
        let tokens = TokenCounter::estimate(text);
        assert_eq!(tokens, 1);
    }

    #[test]
    fn test_token_counter_empty_is_zero() {
        assert_eq!(TokenCounter::estimate(""), 0);
    }

    #[test]
    fn test_slot_config_default_enabled() {
        let config = SlotConfig::new(ContextSlot::System);
        assert!(config.enabled);
        assert_eq!(config.token_budget, 0);
    }

    #[test]
    fn test_slot_config_custom() {
        let config = SlotConfig::new(ContextSlot::Conversation)
            .with_budget(10000)
            .with_max_messages(20);
        assert_eq!(config.token_budget, 10000);
        assert_eq!(config.max_messages, Some(20));
        assert_eq!(config.with_priority(75).effective_priority(), 75);
    }
}
