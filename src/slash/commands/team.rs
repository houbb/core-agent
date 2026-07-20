//! `/team` — 团队管理
//!
//! 创建、查看、管理 Agent 团队。
//!
//! 用法：
//!   /team start <name> <goal>         — 创建新团队
//!   /team status [team-id]            — 查看团队状态
//!   /team list                        — 列出所有团队
//!   /team activate <team-id>          — 激活团队
//!   /team complete <team-id>          — 完成团队
//!   /team archive <team-id>           — 归档团队
//!
//! 路由：Runtime（零模型调用）

use std::sync::Arc;

use async_trait::async_trait;
use core_agent_multi::{
    CreateTeamRequest, MultiAgentManager, Organization, TeamPolicyDefinition,
};
use uuid::Uuid;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Team 命令
#[derive(Clone)]
pub struct TeamCommand {
    multi_agent: Arc<MultiAgentManager>,
}

impl TeamCommand {
    pub fn new(multi_agent: Arc<MultiAgentManager>) -> Self {
        Self { multi_agent }
    }

    /// 获取或创建默认组织
    async fn ensure_org(&self, actor: &str) -> SlashResult<Organization> {
        let orgs = self.multi_agent.list_organizations().await
            .map_err(|e| SlashError::Execution(e.to_string()))?;
        if let Some(org) = orgs.into_iter().next() {
            return Ok(org);
        }
        let org = Organization::new("default", "Default Organization", actor);
        self.multi_agent.create_organization(org).await
            .map_err(|e| SlashError::Execution(e.to_string()))
    }
}

#[async_trait]
impl SlashCommand for TeamCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "team".into(),
            display_name: "Team Manager".into(),
            description: "Create, view, and manage Agent teams".into(),
            usage: "/team <start|status|list|activate|complete|archive> [args]".into(),
            category: SlashCategory::Society,
            min_args: 1,
            max_args: 4,
            read_only: false,
            async_exec: true,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Society
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.is_empty() {
            return Err(SlashError::InvalidArgument(
                "usage: /team <start|status|list|activate|complete|archive> [args]".into(),
            ));
        }
        let subcmd = args[0].as_str();
        match subcmd {
            "start" => {
                if args.len() < 3 {
                    return Err(SlashError::InvalidArgument(
                        "usage: /team start <team-name> <goal>".into(),
                    ));
                }
            }
            "status" | "activate" | "complete" | "archive" => {
                if args.len() < 2 {
                    return Err(SlashError::InvalidArgument(
                        format!("usage: /team {} <team-id>", subcmd),
                    ));
                }
            }
            "list" => { /* no args needed */ }
            _ => {
                return Err(SlashError::InvalidArgument(format!(
                    "unknown subcommand: {}. Supported: start, status, list, activate, complete, archive",
                    subcmd
                )));
            }
        }
        Ok(())
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let args = &ctx.args;
        let subcmd = args[0].as_str();
        let actor = &ctx.workspace;

        match subcmd {
            "start" => {
                let name = &args[1];
                let goal = args[2..].join(" ");
                let org = self.ensure_org(actor).await?;

                let key = name.to_lowercase().replace(' ', "-");
                let request = CreateTeamRequest::new(org.id, &key, name, &goal, actor);
                let team = self.multi_agent.create_team(request).await
                    .map_err(|e| SlashError::Execution(e.to_string()))?;

                Ok(CommandOutput::new(format!(
                    "╭────────────────────────╮\n\
                     │ Team Created           │\n\
                     ╰────────────────────────╯\n\n\
                     Name: {}\n\
                     Key: {}\n\
                     ID: {}\n\
                     Goal: {}\n\
                     State: {}\n\n\
                     Use /team activate <team-id> to make it ready for delegation.\n",
                    team.name, team.key, team.id, team.goal, team.state.as_str()
                )))
            }

            "status" => {
                let team_id = args[1].parse::<Uuid>()
                    .map_err(|_| SlashError::InvalidArgument("invalid team-id (must be a UUID)".into()))?;
                let team = self.multi_agent.find_team(team_id).await
                    .map_err(|e| SlashError::Execution(e.to_string()))?
                    .ok_or_else(|| SlashError::NotFound("team not found".into()))?;

                let mut output = format!(
                    "╭────────────────────────╮\n\
                     │ Team Status            │\n\
                     ╰────────────────────────╯\n\n\
                     Name: {}\n\
                     Key: {}\n\
                     ID: {}\n\
                     State: {}\n\
                     Goal: {}\n\n",
                    team.name, team.key, team.id, team.state.as_str(), team.goal
                );

                let members = self.multi_agent.list_members(team_id).await
                    .map_err(|e| SlashError::Execution(e.to_string()))?;
                if !members.is_empty() {
                    output.push_str("Members:\n");
                    for member in &members {
                        output.push_str(&format!(
                            "  Agent {} — State: {}\n",
                            member.agent_id, member.state.as_str()
                        ));
                    }
                } else {
                    output.push_str("No members yet.\n");
                }

                let collaborations = self.multi_agent.list_collaborations(team_id).await
                    .map_err(|e| SlashError::Execution(e.to_string()))?;
                if !collaborations.is_empty() {
                    output.push_str("\nCollaborations:\n");
                    for collab in &collaborations {
                        output.push_str(&format!(
                            "  ID: {} — State: {} — Goal: {}\n",
                            collab.id, collab.state.as_str(), collab.goal
                        ));
                    }
                }

                Ok(CommandOutput::new(output))
            }

            "list" => {
                let orgs = self.multi_agent.list_organizations().await
                    .map_err(|e| SlashError::Execution(e.to_string()))?;

                let mut output = String::from(
                    "╭────────────────────────╮\n\
                     │ Teams                  │\n\
                     ╰────────────────────────╯\n\n"
                );

                if orgs.is_empty() {
                    output.push_str("No organizations found.\n");
                } else {
                    for org in &orgs {
                        let teams = self.multi_agent.list_teams(org.id).await
                            .map_err(|e| SlashError::Execution(e.to_string()))?;
                        if teams.is_empty() {
                            continue;
                        }
                        output.push_str(&format!("Organization: {}\n", org.name));
                        for team in &teams {
                            let icon = match team.state.as_str() {
                                "ACTIVE" => "▶️",
                                "READY" => "✅",
                                "COMPLETED" => "🏁",
                                "ARCHIVED" => "📦",
                                _ => "🆕",
                            };
                            output.push_str(&format!(
                                "  {} {} — {} ({})\n",
                                icon, team.name, team.goal, team.state.as_str()
                            ));
                        }
                        output.push('\n');
                    }
                }

                Ok(CommandOutput::new(output))
            }

            "activate" => {
                let team_id = args[1].parse::<Uuid>()
                    .map_err(|_| SlashError::InvalidArgument("invalid team-id (must be a UUID)".into()))?;
                self.multi_agent.activate_team(team_id, actor).await
                    .map_err(|e| SlashError::Execution(e.to_string()))?;
                Ok(CommandOutput::new(format!("Team {} activated successfully.", team_id)))
            }

            "complete" => {
                let team_id = args[1].parse::<Uuid>()
                    .map_err(|_| SlashError::InvalidArgument("invalid team-id (must be a UUID)".into()))?;
                self.multi_agent.complete_team(team_id, actor).await
                    .map_err(|e| SlashError::Execution(e.to_string()))?;
                Ok(CommandOutput::new(format!("Team {} completed successfully.", team_id)))
            }

            "archive" => {
                let team_id = args[1].parse::<Uuid>()
                    .map_err(|_| SlashError::InvalidArgument("invalid team-id (must be a UUID)".into()))?;
                self.multi_agent.archive_team(team_id, actor).await
                    .map_err(|e| SlashError::Execution(e.to_string()))?;
                Ok(CommandOutput::new(format!("Team {} archived successfully.", team_id)))
            }

            _ => Err(SlashError::InvalidArgument(format!(
                "unknown subcommand: {}", subcmd
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_agent_multi::{InMemoryMultiAgentStore, Organization};

    fn setup() -> Arc<MultiAgentManager> {
        let store = Arc::new(InMemoryMultiAgentStore::default());
        MultiAgentManager::builder().store(store).build().into()
    }

    #[tokio::test]
    async fn test_team_validate_empty() {
        let agent = setup();
        let cmd = TeamCommand::new(agent);
        assert!(cmd.validate(&[]).await.is_err());
    }

    #[tokio::test]
    async fn test_team_validate_start() {
        let agent = setup();
        let cmd = TeamCommand::new(agent);
        assert!(cmd.validate(&["start".into(), "my-team".into(), "do something".into()]).await.is_ok());
    }

    #[tokio::test]
    async fn test_team_validate_start_no_goal() {
        let agent = setup();
        let cmd = TeamCommand::new(agent);
        assert!(cmd.validate(&["start".into(), "my-team".into()]).await.is_err());
    }

    #[tokio::test]
    async fn test_team_validate_bad_subcmd() {
        let agent = setup();
        let cmd = TeamCommand::new(agent);
        assert!(cmd.validate(&["badcmd".into()]).await.is_err());
    }

    #[tokio::test]
    async fn test_team_list_empty() {
        let agent = setup();
        let cmd = TeamCommand::new(agent);
        let ctx = CommandContext {
            line: "/team list".into(),
            args: vec!["list".into()],
            workspace: ".".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        assert!(result.response.contains("No organizations"));
    }

    #[tokio::test]
    async fn test_team_start_and_list() {
        let agent = setup();
        let cmd = TeamCommand::new(agent);

        // Start a team
        let ctx = CommandContext {
            line: "/team start my-team do-something".into(),
            args: vec!["start".into(), "my-team".into(), "do-something".into()],
            workspace: "admin".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        assert!(result.response.contains("my-team"));

        // List teams
        let ctx = CommandContext {
            line: "/team list".into(),
            args: vec!["list".into()],
            workspace: "admin".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        assert!(result.response.contains("my-team"));
    }

    #[tokio::test]
    async fn test_team_activate_and_status() {
        let agent = setup();
        let cmd = TeamCommand::new(agent);

        // Start a team
        let ctx = CommandContext {
            line: "/team start test-team test-goal".into(),
            args: vec!["start".into(), "test-team".into(), "test-goal".into()],
            workspace: "admin".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        // Extract team ID from output
        let output = result.response;
        let id_line = output.lines().find(|l| l.trim().starts_with("ID:")).unwrap();
        let team_id = id_line.trim().strip_prefix("ID:").unwrap().trim().to_string();

        // Activate - should fail because team has no members (requires at least 1 active member)
        let ctx = CommandContext {
            line: format!("/team activate {}", team_id),
            args: vec!["activate".into(), team_id.clone()],
            workspace: "admin".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await;
        // Without members, activation fails - that's expected behavior
        assert!(result.is_err() || !result.unwrap().response.contains("activated"));

        // Status still works
        let ctx = CommandContext {
            line: format!("/team status {}", team_id),
            args: vec!["status".into(), team_id],
            workspace: "admin".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        assert!(result.response.contains("test-team"));
    }
}