use std::sync::Arc;

use async_trait::async_trait;

use crate::domain::{RawToolOutput, ToolRequest};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{Tool, ToolContext};

/// Execute a git command and return its output.
fn git(args: &[&str], working_dir: Option<&str>) -> ToolRuntimeResult<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(working_dir.unwrap_or("."))
        .output()
        .map_err(|e| ToolError::execution("git", format!("failed to run git: {e}"), true))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout.trim_end().to_string())
    } else {
        Err(ToolError::execution("git", format!("git error: {stderr}"), false))
    }
}

/// `git.diff` — Show working tree changes.
pub struct GitDiffTool;

#[async_trait]
impl Tool for GitDiffTool {
    fn key(&self) -> &str { "builtin/git.diff@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let wd = request.parameters["working_dir"].as_str()
            .or_else(|| request.parameters["path"].as_str())
            .filter(|p| !p.is_empty());
        let staged = request.parameters["staged"].as_bool().unwrap_or(false);

        let mut args = vec!["diff"];
        if staged { args.push("--cached"); }
        args.push("--no-color");

        let result = git(&args, wd)?;
        Ok(RawToolOutput::text(if result.is_empty() { "No changes." } else { &result }))
    }
}

pub fn git_diff_tool() -> Arc<dyn Tool> { Arc::new(GitDiffTool) }

/// `git.status` — Show repository status.
pub struct GitStatusTool;

#[async_trait]
impl Tool for GitStatusTool {
    fn key(&self) -> &str { "builtin/git.status@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let wd = request.parameters["path"].as_str().or_else(|| request.parameters["working_dir"].as_str());
        let result = git(&["status", "--short", "--branch"], wd)?;
        Ok(RawToolOutput::text(result))
    }
}

pub fn git_status_tool() -> Arc<dyn Tool> { Arc::new(GitStatusTool) }

/// `git.log` — Show commit history.
pub struct GitLogTool;

#[async_trait]
impl Tool for GitLogTool {
    fn key(&self) -> &str { "builtin/git.log@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let max_count = request.parameters["max_count"].as_u64().unwrap_or(10);
        let n_flag = format!("-n{max_count}");
        let wd = request.parameters["path"].as_str().or_else(|| request.parameters["working_dir"].as_str());
        let result = git(
            &["log", "--oneline", "--no-decorate", &n_flag],
            wd,
        )?;
        Ok(RawToolOutput::text(if result.is_empty() { "No commits." } else { &result }))
    }
}

pub fn git_log_tool() -> Arc<dyn Tool> { Arc::new(GitLogTool) }

/// `git.commit` — Create a commit.
pub struct GitCommitTool;

#[async_trait]
impl Tool for GitCommitTool {
    fn key(&self) -> &str { "builtin/git.commit@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let message = request.parameters["message"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("message is required".into()))?;
        if message.is_empty() {
            return Err(ToolError::InvalidArgument("message must not be empty".into()));
        }

        let all = request.parameters["all"].as_bool().unwrap_or(true);
        let wd = request.parameters["path"].as_str().or_else(|| request.parameters["working_dir"].as_str());
        if all {
            git(&["add", "-A"], wd)?;
        }
        let result = git(&["commit", "-m", message], wd)?;
        Ok(RawToolOutput::text(result))
    }
}

pub fn git_commit_tool() -> Arc<dyn Tool> { Arc::new(GitCommitTool) }

/// `git.branch` — List or create branches.
pub struct GitBranchTool;

#[async_trait]
impl Tool for GitBranchTool {
    fn key(&self) -> &str { "builtin/git.branch@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let wd = request.parameters["path"].as_str().or_else(|| request.parameters["working_dir"].as_str());
        let mut args = vec!["branch"];
        if let Some(name) = request.parameters["name"].as_str().filter(|n| !n.is_empty()) {
            args.push(name);
        }
        let result = git(&args, wd)?;
        Ok(RawToolOutput::text(result))
    }
}

pub fn git_branch_tool() -> Arc<dyn Tool> { Arc::new(GitBranchTool) }

/// `git.checkout` — Switch branches or restore files.
pub struct GitCheckoutTool;

#[async_trait]
impl Tool for GitCheckoutTool {
    fn key(&self) -> &str { "builtin/git.checkout@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let branch = request.parameters["branch"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArgument("branch is required".into()))?;
        if branch.is_empty() {
            return Err(ToolError::InvalidArgument("branch must not be empty".into()));
        }
        let wd = request.parameters["path"].as_str().or_else(|| request.parameters["working_dir"].as_str());
        let result = git(&["checkout", branch], wd)?;
        Ok(RawToolOutput::text(result))
    }
}

pub fn git_checkout_tool() -> Arc<dyn Tool> { Arc::new(GitCheckoutTool) }

/// `git.push` — Push local commits to a remote.
///
/// Codex CLI 自动化 Git 工作流的最后一步，让 Agent 能完成
/// `commit → push` 全流程，而非停留在本地 commit。
pub struct GitPushTool;

#[async_trait]
impl Tool for GitPushTool {
    fn key(&self) -> &str { "builtin/git.push@1.0.0" }

    async fn execute(&self, request: &ToolRequest, _ctx: &ToolContext) -> ToolRuntimeResult<RawToolOutput> {
        let wd = request.parameters["path"].as_str().or_else(|| request.parameters["working_dir"].as_str());
        let force = request.parameters["force"].as_bool().unwrap_or(false);
        let set_upstream = request.parameters["set_upstream"].as_bool().unwrap_or(true);

        // Optional explicit remote + branch, e.g. "origin main".
        let remote = request.parameters["remote"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or("origin");
        let branch = request.parameters["branch"]
            .as_str()
            .filter(|s| !s.is_empty());

        let mut args = vec!["push"];
        if set_upstream { args.push("-u"); }
        if force { args.push("--force-with-lease"); }
        args.push(remote);
        if let Some(branch) = branch {
            // Normalize with explicit src:dst form to avoid ambiguity.
            args.push(branch);
        }
        let result = git(&args, wd)?;
        Ok(RawToolOutput::text(result))
    }
}

pub fn git_push_tool() -> Arc<dyn Tool> { Arc::new(GitPushTool) }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ToolRequest;
    use tempfile::tempdir;

    fn init_git_repo(dir: &std::path::Path) {
        std::process::Command::new("git")
            .args(["init", "--initial-branch=main"])
            .current_dir(dir)
            .output().unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir)
            .output().unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir)
            .output().unwrap();
    }

    #[tokio::test]
    async fn git_status_shows_branch() {
        let dir = tempdir().unwrap();
        init_git_repo(dir.path());
        let tool = GitStatusTool;
        let request = ToolRequest::new(
            "builtin/git.status@1.0.0",
            serde_json::json!({"path": dir.path().to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("main"));
    }

    #[tokio::test]
    async fn git_log_shows_commits() {
        let dir = tempdir().unwrap();
        init_git_repo(dir.path());
        tokio::fs::write(dir.path().join("f.txt"), "data").await.unwrap();
        git(&["add", "-A"], Some(dir.path().to_str().unwrap())).unwrap();
        git(&["commit", "-m", "initial"], Some(dir.path().to_str().unwrap())).unwrap();

        let tool = GitLogTool;
        let request = ToolRequest::new(
            "builtin/git.log@1.0.0",
            serde_json::json!({"path": dir.path().to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("initial"));
    }

    #[tokio::test]
    async fn git_diff_shows_changes() {
        let dir = tempdir().unwrap();
        init_git_repo(dir.path());
        tokio::fs::write(dir.path().join("f.txt"), "data").await.unwrap();
        git(&["add", "-A"], Some(dir.path().to_str().unwrap())).unwrap();
        git(&["commit", "-m", "init"], Some(dir.path().to_str().unwrap())).unwrap();
        tokio::fs::write(dir.path().join("f.txt"), "modified").await.unwrap();

        let tool = GitDiffTool;
        let request = ToolRequest::new(
            "builtin/git.diff@1.0.0",
            serde_json::json!({"path": dir.path().to_string_lossy()}),
        );
        let result = tool.execute(&request, &ToolContext::default()).await.unwrap();
        let text = match &result.content[0] {
            crate::domain::ToolContent::Text(t) => t.clone(),
            _ => panic!("expected text"),
        };
        assert!(text.contains("modified") || text.contains("diff"));
    }
}