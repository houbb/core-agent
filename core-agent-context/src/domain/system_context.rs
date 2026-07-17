//! SystemContext — 系统提示上下文
//!
//! 包含系统提示词、启用的能力列表、模型配置等。

use serde::{Deserialize, Serialize};

/// SystemContext
///
/// 系统提示是 Agent 行为的基础约束。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemContext {
    /// 系统提示文本
    pub prompt: Option<String>,
    /// 启用的能力列表（如 ["code", "shell", "file"]）
    pub capabilities: Vec<String>,
    /// 目标模型名称
    pub model: Option<String>,
    /// 附加配置（JSON，可扩展）
    pub config: serde_json::Value,
}

impl SystemContext {
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: Some(prompt.into()),
            capabilities: Vec::new(),
            model: None,
            config: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// 添加能力
    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        self.capabilities.push(cap.into());
        self
    }

    /// 设置模型
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}