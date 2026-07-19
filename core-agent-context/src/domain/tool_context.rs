//! ToolContext — 工具结果上下文
//!
//! P1 仅保留结构化工具结果；真正的 Tool Runtime 在 Phase 3 实现。

use serde::{Deserialize, Serialize};

/// 工具结果占位上下文。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContext {
    /// 是否包含工具结果。
    pub enabled: bool,
    /// Provider 注入的结构化工具结果。
    pub content: serde_json::Value,
}

impl ToolContext {
    /// 创建未启用的空 ToolContext。
    pub fn new() -> Self {
        Self {
            enabled: false,
            content: serde_json::Value::Array(Vec::new()),
        }
    }
}

impl Default for ToolContext {
    fn default() -> Self {
        Self::new()
    }
}
