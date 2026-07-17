//! EnvironmentProvider — 环境上下文提供者
//!
//! 收集 Agent 运行环境信息：OS、Shell、工作目录、Git 状态等。

use async_trait::async_trait;
use std::process::Command;

use crate::domain::context::{ContextSegment, ContextSource};
use crate::domain::slot::{ContextSlot, TokenCounter};
use crate::error::ContextResult;
use crate::infrastructure::{ContextProvider, ProviderContext};

/// EnvironmentProvider
///
/// 在每次 build() 时实时收集环境信息。
pub struct EnvironmentProvider;

impl EnvironmentProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for EnvironmentProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContextProvider for EnvironmentProvider {
    fn name(&self) -> &str {
        "environment-provider"
    }

    fn source(&self) -> ContextSource {
        ContextSource::Environment
    }

    fn slot(&self) -> ContextSlot {
        ContextSlot::Environment
    }

    async fn collect(&self, ctx: &ProviderContext) -> ContextResult<Vec<ContextSegment>> {
        let os = std::env::consts::OS.to_string();
        let os_version = os_version();
        let shell = std::env::var("SHELL").or_else(|_| std::env::var("COMSPEC")).ok();
        let working_directory = ctx
            .working_directory
            .clone()
            .or_else(|| std::env::current_dir().ok().map(|p| p.display().to_string()));

        let git_branch = git_branch(&working_directory);
        let git_root = git_root(&working_directory);

        let content = serde_json::json!({
            "os": os,
            "os_version": os_version,
            "shell": shell,
            "working_directory": working_directory,
            "git_branch": git_branch,
            "git_root": git_root,
        });

        let token_count = TokenCounter::estimate_json(&content);
        let segment = ContextSegment::new(
            ContextSource::Environment,
            ContextSlot::Environment,
            content,
            token_count,
            ContextSlot::Environment.default_priority(),
        )
        .required(); // 环境上下文不可裁剪

        Ok(vec![segment])
    }
}

/// 获取操作系统版本
fn os_version() -> Option<String> {
    if cfg!(target_os = "windows") {
        // Windows: wmic os get Caption
        Command::new("cmd")
            .args(["/c", "ver"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        // Unix: uname -a
        Command::new("uname")
            .args(["-a"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    }
}

/// 获取当前的 Git 分支名
fn git_branch(working_dir: &Option<String>) -> Option<String> {
    let dir = working_dir.as_deref().unwrap_or(".");
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// 获取 Git 仓库根路径
fn git_root(working_dir: &Option<String>) -> Option<String> {
    let dir = working_dir.as_deref().unwrap_or(".");
    Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use core_agent_session::SqliteSessionStore;

    #[tokio::test]
    async fn test_environment_provider_collect() {
        let store = Arc::new(SqliteSessionStore::new(":memory:").unwrap());
        let session_id = uuid::Uuid::new_v4();
        let ctx = ProviderContext::new(session_id, store);

        let provider = EnvironmentProvider::new();
        let segments = provider.collect(&ctx).await.unwrap();

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].source, ContextSource::Environment);
        assert_eq!(segments[0].slot, ContextSlot::Environment);
        assert!(segments[0].required);
    }
}