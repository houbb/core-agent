//! `/collaborate` — 查看协作过程
//!
//! 查看团队成员之间的协作过程和状态。
//!
//! 用法：
//!   /collaborate                     — 查看所有活跃协作
//!   /collaborate <team-id>           — 查看指定团队的协作详情
//!
//! 路由：Runtime（零模型调用）

use std::sync::Arc;

use async_trait::async_trait;
use core_agent_multi::MultiAgentManager;
use uuid::Uuid;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Collaborate 命令
#[derive(Clone)]
pub struct CollaborateCommand {
    multi_agent: Arc<MultiAgentManager>,
}

impl CollaborateCommand {
    pub fn new(multi_agent: Arc<MultiAgentManager>) -> Self {
        Self { multi_agent }
    }
}

#[async_trait]
impl SlashCommand for CollaborateCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "collaborate".into(),
            display_name: "Collaboration Status".into(),
            description: "View collaboration progress between team members".into(),
            usage: "/collaborate [team-id]".into(),
            category: SlashCategory::Society,
            min_args: 0,
            max_args: 1,
            read_only: true,
            async_exec: true,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Society
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let args = &ctx.args;

        if args.is_empty() {
            // List all collaborations across all teams
            let orgs = self.multi_agent.list_organizations().await
                .map_err(|e| SlashError::Execution(e.to_string()))?;

            let mut output = String::from(
                "╭────────────────────────╮\n\
                 │ Current Collaboration  │\n\
                 ╰────────────────────────╯\n\n"
            );

            let mut has_any = false;
            for org in &orgs {
                let teams = self.multi_agent.list_teams(org.id).await
                    .map_err(|e| SlashError::Execution(e.to_string()))?;
                for team in &teams {
                    let collaborations = self.multi_agent.list_collaborations(team.id).await
                        .map_err(|e| SlashError::Execution(e.to_string()))?;
                    if collaborations.is_empty() {
                        continue;
                    }
                    has_any = true;
                    output.push_str(&format!("Team: {} ({})\n\n", team.name, team.key));
                    for collab in &collaborations {
                        let icon = match collab.state.as_str() {
                            "WORKING" => "🔄",
                            "WAITING" => "⏳",
                            "COMPLETED" => "✅",
                            "FAILED" => "❌",
                            "CANCELLED" => "🚫",
                            _ => "📋",
                        };
                        output.push_str(&format!(
                            "  {} Collaboration: {}\n     Goal: {}\n     State: {}\n",
                            icon, collab.id, collab.goal, collab.state.as_str()
                        ));
                        if let Some(result) = &collab.result {
                            output.push_str(&format!("     Result: {}\n", result.summary));
                        }
                        if let Some(error) = &collab.error {
                            output.push_str(&format!("     Error: {}\n", error));
                        }
                        output.push('\n');
                    }
                }
            }

            if !has_any {
                output.push_str("No active collaborations.\n");
                output.push_str("Use /delegate <task> to start a new collaboration.\n");
            }

            Ok(CommandOutput::new(output))
        } else {
            // Show collaborations for a specific team
            let team_id = args[0].parse::<Uuid>()
                .map_err(|_| SlashError::InvalidArgument("invalid team-id (must be a UUID)".into()))?;

            let team = self.multi_agent.find_team(team_id).await
                .map_err(|e| SlashError::Execution(e.to_string()))?
                .ok_or_else(|| SlashError::NotFound("team not found".into()))?;

            let mut output = format!(
                "╭────────────────────────╮\n\
                 │ Collaboration Details  │\n\
                 ╰────────────────────────╯\n\n\
                 Team: {} ({})\n\
                 State: {}\n\n",
                team.name, team.key, team.state.as_str()
            );

            let members = self.multi_agent.list_members(team_id).await
                .map_err(|e| SlashError::Execution(e.to_string()))?;
            if !members.is_empty() {
                output.push_str("Members:\n");
                for member in &members {
                    output.push_str(&format!("  Agent {} — {}\n", member.agent_id, member.state.as_str()));
                }
                output.push('\n');
            }

            let collaborations = self.multi_agent.list_collaborations(team_id).await
                .map_err(|e| SlashError::Execution(e.to_string()))?;
            if collaborations.is_empty() {
                output.push_str("No collaborations for this team.\n");
            } else {
                output.push_str("Collaborations:\n");
                for collab in &collaborations {
                    let icon = match collab.state.as_str() {
                        "WORKING" => "🔄",
                        "WAITING" => "⏳",
                        "COMPLETED" => "✅",
                        "FAILED" => "❌",
                        _ => "📋",
                    };
                    output.push_str(&format!(
                        "  {} ID: {}\n     Goal: {}\n     State: {}\n     Priority: {}\n",
                        icon, collab.id, collab.goal, collab.state.as_str(), collab.priority.as_str()
                    ));
                    if let Some(result) = &collab.result {
                        output.push_str(&format!(
                            "     Result: {}\n     External State: {}\n",
                            result.summary, result.external_state
                        ));
                    }
                    if let Some(error) = &collab.error {
                        output.push_str(&format!("     Error: {}\n", error));
                    }
                    output.push('\n');
                }
            }

            Ok(CommandOutput::new(output))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_agent_multi::{CreateTeamRequest, Organization, InMemoryMultiAgentStore};

    fn setup() -> Arc<MultiAgentManager> {
        let store = Arc::new(InMemoryMultiAgentStore::default());
        MultiAgentManager::builder().store(store).build().into()
    }

    #[tokio::test]
    async fn test_collaborate_empty() {
        let agent = setup();
        let cmd = CollaborateCommand::new(agent);
        let ctx = CommandContext {
            line: "/collaborate".into(),
            args: vec![],
            workspace: ".".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        assert!(result.response.contains("No active collaborations"));
    }

    #[tokio::test]
    async fn test_collaborate_with_team() {
        let agent = setup();
        let org = Organization::new("engineering", "Engineering", "admin");
        let org = agent.create_organization(org).await.unwrap();

        let req = CreateTeamRequest::new(org.id, "refactor", "Refactor Team", "Refactor auth", "admin");
        let team = agent.create_team(req).await.unwrap();

        let cmd = CollaborateCommand::new(agent);
        let ctx = CommandContext {
            line: format!("/collaborate {}", team.id),
            args: vec![team.id.to_string()],
            workspace: ".".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        assert!(result.response.contains("Refactor Team"));
    }

    #[tokio::test]
    async fn test_collaborate_bad_team_id() {
        let agent = setup();
        let cmd = CollaborateCommand::new(agent);
        let ctx = CommandContext {
            line: "/collaborate bad-id".into(),
            args: vec!["bad-id".into()],
            workspace: ".".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await;
        assert!(result.is_err());
    }
}