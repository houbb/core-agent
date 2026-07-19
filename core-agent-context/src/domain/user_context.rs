//! UserContext — 用户输入上下文
//!
//! 包含当前用户输入和附件信息。

use serde::{Deserialize, Serialize};

/// UserContext
///
/// 每次 build() 时由 UserProvider 填充当前用户输入。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserContext {
    /// 当前用户输入文本
    pub current_input: Option<String>,
    /// 附件列表（文件名或 ID）
    pub attachments: Vec<String>,
    /// 扩展内容（JSON）
    pub extra: serde_json::Value,
}

impl UserContext {
    pub fn new() -> Self {
        Self {
            current_input: None,
            attachments: Vec::new(),
            extra: serde_json::Value::Object(serde_json::Map::new()),
        }
    }

    /// 设置当前输入
    pub fn with_input(mut self, input: impl Into<String>) -> Self {
        self.current_input = Some(input.into());
        self
    }

    /// 添加附件
    pub fn with_attachment(mut self, attachment: impl Into<String>) -> Self {
        self.attachments.push(attachment.into());
        self
    }
}
