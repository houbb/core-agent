//! WorkspaceContext — 工作空间上下文
//!
//! MVP 阶段仅定义结构，后续 Phase 4 Workspace Runtime 补充完整实现。
//! 包含 Agent 工作空间的文件、目录、Git 等信息。

use serde::{Deserialize, Serialize};

/// WorkspaceContext
///
/// 在 Phase 4 之前为占位结构。
/// Phase 4 将引入文件树、Git 状态、索引等。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceContext {
    /// 是否启用
    pub enabled: bool,
    /// 工作空间根路径
    pub root_path: Option<String>,
    /// 扩展内容（JSON，后续由 Workspace Runtime 填充）
    pub content: serde_json::Value,
}

impl WorkspaceContext {
    pub fn new() -> Self {
        Self {
            enabled: false,
            root_path: None,
            content: serde_json::Value::Object(serde_json::Map::new()),
        }
    }
}
