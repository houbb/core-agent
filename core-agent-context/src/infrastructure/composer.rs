//! ContextComposer — 上下文组装器 trait
//!
//! 负责将多个 ContextSegment 组装成最终的 Context 对象。
//! 不同模型（Claude / GPT / Gemini）可提供不同的 Composer 实现（排序、格式化差异）。

use async_trait::async_trait;

use crate::domain::context::{Context, ContextSegment};
use crate::error::ContextResult;

/// ContextComposer — 上下文组装器
///
/// 负责：
/// 1. 排序（按 Slot 优先级）
/// 2. 分配到各子 Context 结构
/// 3. 生成 Context 对象（含 Token 分布和哈希）
#[async_trait]
pub trait ContextComposer: Send + Sync {
    /// Composer 名称
    fn name(&self) -> &str;

    /// 组装上下文
    ///
    /// # Arguments
    /// * `session_id` - 所属 Session ID
    /// * `conversation_id` - 关联 Conversation ID
    /// * `segments` - 所有 Provider 产出 + Reducer 裁剪后的 segments
    ///
    /// # Returns
    /// 完整的 Context 对象
    async fn compose(
        &self,
        session_id: uuid::Uuid,
        conversation_id: Option<uuid::Uuid>,
        segments: Vec<ContextSegment>,
    ) -> ContextResult<Context>;
}