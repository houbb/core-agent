//! `/delegate` — 任务委派
//!
//! 将任务委派给指定 Agent/角色。
//!
//! 用法：
//!   /delegate <task>                  — 委派任务到默认角色
//!   /delegate <task> --role <role>    — 委派任务到指定角色
//!   /delegate <task> --priority <p>   — 指定优先级
//!
//! 路由：Runtime（零模型调用）

use std::sync::Arc;

use async_trait::async_trait;
use core_agent_multi::{
    AssignmentRequest, MessagePriority, MultiAgentManager, Organization,
};
use uuid::Uuid;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Delegate 命令
#[derive(Clone)]
pub struct DelegateCommand {
    multi_agent: Arc<MultiAgentManager>,
}

impl DelegateCommand {
    pub fn new(multi_agent: Arc<MultiAgentManager>) -> Self {
        Self { multi_agent }
    }

    /// 查找第一个可用的 Organization（用于默认委派）
    async fn first_org(&self) -> SlashResult<Organization> {
        self.multi_agent.list_organizations().await
            .map_err(|e| SlashError::Execution(e.to_string()))?
            .into_iter()
            .next()
            .ok_or_else(|| SlashError::Execution("No organization found. Create one first with /team start.".into()))
    }

    /// 查找第一个可用团队（用于默认委派）
    async fn first_ready_team(&self, org_id: Uuid) -> SlashResult<Uuid> {
        let teams = self.multi_agent.list_teams(org_id).await
            .map_err(|e| SlashError::Execution(e.to_string()))?;
        teams.into_iter()
            .find(|t| t.state.as_str() == "READY")
            .map(|t| t.id)
            .ok_or_else(|| SlashError::Execution("No READY team found. Create one first with /team start.".into()))
    }
}

#[async_trait]
impl SlashCommand for DelegateCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "delegate".into(),
            display_name: "Delegate Task".into(),
            description: "Delegate a task to an agent or role in the Agent Society".into(),
            usage: "/delegate <task> [--role <role>] [--priority <p>]".into(),
            category: SlashCategory::Society,
            min_args: 1,
            max_args: 6,
            read_only: false,
            async_exec: true,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Society
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.is_empty() || args[0].starts_with("--") {
            return Err(SlashError::InvalidArgument(
                "usage: /delegate <task> [--role <role>] [--priority <p>]".into(),
            ));
        }
        // Validate --role <name> and --priority <low|normal|high|critical>
        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--role" | "--priority" => {
                    if i + 1 >= args.len() {
                        return Err(SlashError::InvalidArgument(
                            format!("{} requires a value", args[i])
                        ));
                    }
                    if args[i] == "--priority" {
                        let p = args[i + 1].to_lowercase();
                        if !matches!(p.as_str(), "low" | "normal" | "high" | "critical") {
                            return Err(SlashError::InvalidArgument(
                                "priority must be one of: low, normal, high, critical".into()
                            ));
                        }
                    }
                    i += 2;
                }
                _ => {
                    return Err(SlashError::InvalidArgument(format!(
                        "unknown flag: {}. Supported: --role, --priority",
                        args[i]
                    )));
                }
            }
        }
        Ok(())
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let args = &ctx.args;
        let task = args[0].clone();

        // Parse optional flags
        let mut role_name: Option<String> = None;
        let mut priority = MessagePriority::Normal;

        let mut i = 1;
        while i < args.len() {
            match args[i].as_str() {
                "--role" => {
                    role_name = Some(args[i + 1].clone());
                    i += 2;
                }
                "--priority" => {
                    priority = match args[i + 1].to_lowercase().as_str() {
                        "low" => MessagePriority::Low,
                        "high" => MessagePriority::High,
                        "critical" => MessagePriority::Critical,
                        _ => MessagePriority::Normal,
                    };
                    i += 2;
                }
                _ => i += 1,
            }
        }

        // Find org and team to delegate to
        let org = self.first_org().await?;
        let team_id = self.first_ready_team(org.id).await?;

        // Build the assignment request
        let mut request = AssignmentRequest::new(team_id, &task, &ctx.workspace);
        request.priority = priority;

        // Execute delegation
        let collaboration = self.multi_agent.assign(request).await
            .map_err(|e| SlashError::Execution(format!("Delegation failed: {e}")))?;

        let priority_str = collaboration.priority.as_str();
        let state_str = collaboration.state.as_str();

        let mut output = format!(
            "╭────────────────────────╮\n\
             │ Delegation Result      │\n\
             ╰────────────────────────╯\n\n\
             Task: {}\n\
             Collaboration ID: {}\n\
             Priority: {}\n\
             State: {}\n",
            task, collaboration.id, priority_str, state_str
        );

        if let Some(binding) = &collaboration.binding {
            output.push_str(&format!(
                "Binding: {} (kind: {})\n",
                binding.dispatch_id, binding.external_kind
            ));
        }

        if let Some(result) = &collaboration.result {
            output.push_str(&format!(
                "Result: {}\nExternal State: {}\n",
                result.summary, result.external_state
            ));
        }

        if let Some(error) = &collaboration.error {
            output.push_str(&format!("Error: {}\n", error));
        }

        Ok(CommandOutput::new(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_agent_multi::{CreateTeamRequest, Organization, InMemoryMultiAgentStore};

    fn setup_agent() -> Arc<MultiAgentManager> {
        let store = Arc::new(InMemoryMultiAgentStore::default());
        MultiAgentManager::builder().store(store).build().into()
    }

    #[tokio::test]
    async fn test_delegate_validate_empty() {
        let agent = setup_agent();
        let cmd = DelegateCommand::new(agent);
        let result = cmd.validate(&[]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delegate_validate_ok() {
        let agent = setup_agent();
        let cmd = DelegateCommand::new(agent);
        let result = cmd.validate(&["review the code".into()]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delegate_validate_with_role() {
        let agent = setup_agent();
        let cmd = DelegateCommand::new(agent);
        let result = cmd.validate(&["review".into(), "--role".into(), "reviewer".into()]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_delegate_validate_bad_priority() {
        let agent = setup_agent();
        let cmd = DelegateCommand::new(agent);
        let result = cmd.validate(&["task".into(), "--priority".into(), "urgent".into()]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delegate_no_org() {
        let agent = Arc::new(MultiAgentManager::builder().build());
        let cmd = DelegateCommand::new(agent);
        let ctx = CommandContext {
            line: "/delegate test".into(),
            args: vec!["test".into()],
            workspace: ".".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await;
        assert!(result.is_err());
    }
}