//! ConversationContext — 对话上下文
//!
//! 包含消息历史列表，每条消息带有 role / content / token_count 等。

use serde::{Deserialize, Serialize};

/// ContextMessage — 消息在 Context 中的视图
///
/// 比 Session Runtime 的 Message 更精简，只保留 Context 构建所需的字段。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextMessage {
    /// 消息 ID
    pub id: String,
    /// 消息角色（SYSTEM / USER / ASSISTANT / TOOL）
    pub role: String,
    /// 消息内容
    pub content: String,
    /// 估算 Token 数
    pub token_count: u64,
    /// 创建时间（ISO 8601）
    pub created_at: String,
}

/// ConversationContext
///
/// 包含从 SessionStore 读取的消息历史。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConversationContext {
    /// 消息列表
    pub messages: Vec<ContextMessage>,
    /// 消息总数（可能大于 messages 列表，因为被 Reducer 裁剪过）
    pub total_count: usize,
    /// 是否有摘要（Reducer 压缩产生）
    pub has_summary: bool,
    /// 摘要内容（如果有）
    pub summary: Option<String>,
}

impl ConversationContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加消息
    pub fn add_message(&mut self, msg: ContextMessage) {
        self.messages.push(msg);
        self.total_count += 1;
    }
}