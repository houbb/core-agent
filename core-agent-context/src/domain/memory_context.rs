//! MemoryContext — 记忆上下文
//!
//! MVP 阶段仅定义结构，后续 Phase 7 Memory Runtime 补充完整实现。
//! 包含短期记忆、长期记忆、知识库检索结果等。

use serde::{Deserialize, Serialize};

/// MemoryContext
///
/// 在 Phase 7 之前为占位结构。
/// Phase 7 将引入短期/长期记忆、语义搜索、知识图谱等。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryContext {
    /// 是否启用
    pub enabled: bool,
    /// 扩展内容（JSON，后续由 Memory Runtime 填充）
    pub content: serde_json::Value,
}

impl MemoryContext {
    pub fn new() -> Self {
        Self {
            enabled: false,
            content: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}