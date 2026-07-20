//! `/security-review` — Security Review Agent
//!
//! 使用 SecurityReview SubAgent 审计代码安全漏洞。
//!
//! 用法：
//!   /security-review <task>         — 运行安全审查 Agent
//!
//! 路由：Agent（触发 SubAgent Runtime）

use std::sync::Arc;

use async_trait::async_trait;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};
use crate::subagent_runtime::{SubAgentProfile, SubAgentRuntime};
use tokio_util::sync::CancellationToken;

/// Security Review 命令
#[derive(Clone)]
pub struct SecurityReviewCommand {
    runtime: Arc<SubAgentRuntime>,
}

impl SecurityReviewCommand {
    pub fn new(runtime: Arc<SubAgentRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl SlashCommand for SecurityReviewCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "security-review".into(),
            display_name: "Security Review Agent".into(),
            description: "Audit code for security vulnerabilities using the Security Review Agent".into(),
            usage: "/security-review <task>".into(),
            category: SlashCategory::Agent,
            min_args: 1,
            max_args: 8,
            read_only: false,
            async_exec: true,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Agent
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let task = ctx.args.join(" ");

        let outcome = self.runtime.run(
            &task,
            SubAgentProfile::SecurityReview,
            ctx.session_id.and_then(|s| s.parse().ok()),
            CancellationToken::new(),
        ).await.map_err(|e| SlashError::Execution(e.to_string()))?;

        Ok(CommandOutput::new(format!(
            "╭────────────────────────╮\n\
             │ Security Review Result │\n\
             ╰────────────────────────╯\n\n\
             {}\n\n\
             ---\n\
             Profile: {:?} | Tool calls: {} | Turns: {}",
            outcome.response,
            outcome.profile,
            outcome.tool_calls,
            outcome.turns,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subagent_runtime::SubAgentProfile;

    #[test]
    fn security_review_profile_is_security_review() {
        assert_eq!(SubAgentProfile::SecurityReview.max_turns(), 6);
    }
}