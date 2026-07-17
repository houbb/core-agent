//! EnvironmentContext — 环境上下文
//!
//! 包含 Agent 运行时的环境信息：操作系统、Shell、Git 状态等。

use serde::{Deserialize, Serialize};

/// EnvironmentContext
///
/// 由 EnvironmentProvider 在每次 build() 时实时收集。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvironmentContext {
    /// 操作系统名称（如 "windows", "linux", "macos"）
    pub os: Option<String>,
    /// 操作系统版本
    pub os_version: Option<String>,
    /// 当前 Shell（如 "bash", "powershell"）
    pub shell: Option<String>,
    /// 当前工作目录
    pub working_directory: Option<String>,
    /// 当前 Git 分支（如果有）
    pub git_branch: Option<String>,
    /// Git 仓库根路径（如果有）
    pub git_root: Option<String>,
    /// 扩展环境变量
    pub extra: serde_json::Value,
}

impl EnvironmentContext {
    pub fn new() -> Self {
        Self {
            os: Some(std::env::consts::OS.to_string()),
            extra: serde_json::Value::Object(serde_json::Map::new()),
            ..Default::default()
        }
    }
}