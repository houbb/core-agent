//! ContextReducer — Token 裁剪器 trait
//!
//! 负责裁剪 ContextSegment 列表，确保 Token 总量不超预算。
//! MVP 提供 SummaryReducer（兼容名称，默认执行 Last-N + Token 预算），
//! 未来可扩展为滑动窗口、重要性评分、语义压缩等策略。

use async_trait::async_trait;
use std::collections::HashMap;

use crate::domain::context::ContextSegment;
use crate::domain::slot::ContextSlot;
use crate::error::ContextResult;

/// ReducerConfig — Reducer 运行配置
#[derive(Debug, Clone)]
pub struct ReducerConfig {
    /// 全局 Token 预算上限（0 表示不限制）
    pub max_total_tokens: u64,
    /// 保留最近 N 条消息（仅 Conversation Slot）
    pub keep_recent_messages: usize,
    /// 是否生成摘要（超出部分压缩为摘要）
    pub enable_summary: bool,
    /// 达到全局预算的百分比后启用消息窗口/摘要策略。
    pub trigger_percent: u8,
    /// 各 Slot 的预算（0 表示使用全局预算按比例分配）
    pub slot_budgets: HashMap<ContextSlot, u64>,
}

impl Default for ReducerConfig {
    fn default() -> Self {
        Self {
            max_total_tokens: 128_000,
            keep_recent_messages: 20,
            enable_summary: false,
            trigger_percent: 80,
            slot_budgets: HashMap::new(),
        }
    }
}

/// ContextReducer — Token 裁剪策略
///
/// MVP 实现：保留最近 N 条；显式启用时可生成兼容性提取摘要
/// 未来扩展：滑动窗口、重要性评分、语义压缩
#[async_trait]
pub trait ContextReducer: Send + Sync {
    /// Reducer 名称
    fn name(&self) -> &str;

    /// 裁剪 ContextSegment 列表
    ///
    /// 输入原始 segments + 配置，返回裁剪后的 segments。
    async fn reduce(
        &self,
        segments: Vec<ContextSegment>,
        config: &ReducerConfig,
    ) -> ContextResult<Vec<ContextSegment>>;
}
