//! PluginContext — 插件上下文
//!
//! MVP 阶段仅定义结构，后续 Phase 9 Plugin Runtime 补充完整实现。
//! 包含 MCP 插件、扩展注入的上下文。

use serde::{Deserialize, Serialize};

/// PluginContext
///
/// 在 Phase 9 之前为占位结构。
/// Phase 9 将引入 MCP、Marketplace、Hook 机制。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginContext {
    /// 是否启用
    pub enabled: bool,
    /// 扩展内容（JSON，后续由 Plugin Runtime 填充）
    pub content: serde_json::Value,
}

impl PluginContext {
    pub fn new() -> Self {
        Self {
            enabled: false,
            content: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}
