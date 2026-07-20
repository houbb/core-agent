//! `/agents` — Agent Society 成员列表
//!
//! 查看所有 Agent、角色、组织、团队的信息。
//!
//! 用法：
//!   /agents                          — 查看所有 Agent 社会成员
//!
//! 路由：Runtime（零模型调用）

use std::sync::Arc;

use async_trait::async_trait;
use core_agent_multi::MultiAgentManager;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Agents 命令
#[derive(Clone)]
pub struct AgentsCommand {
    multi_agent: Arc<MultiAgentManager>,
}

impl AgentsCommand {
    pub fn new(multi_agent: Arc<MultiAgentManager>) -> Self {
        Self { multi_agent }
    }

    async fn collect_summary(&self) -> SlashResult<(usize, usize, usize)> {
        let orgs = self.multi_agent.list_organizations().await
            .map_err(|e| SlashError::Execution(e.to_string()))?;
        let mut total_teams = 0usize;
        let mut total_members = 0usize;
        for org in &orgs {
            if let Ok(teams) = self.multi_agent.list_teams(org.id).await {
                for team in &teams {
                    total_teams += 1;
                    if let Ok(members) = self.multi_agent.list_members(team.id).await {
                        total_members += members.len();
                    }
                }
            }
        }
        Ok((orgs.len(), total_teams, total_members))
    }
}

#[async_trait]
impl SlashCommand for AgentsCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "agents".into(),
            display_name: "Agent Society".into(),
            description: "View all agents, roles, organizations and teams in Agent Society".into(),
            usage: "/agents".into(),
            category: SlashCategory::Society,
            min_args: 0,
            max_args: 0,
            read_only: true,
            async_exec: true,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Society
    }

    async fn execute(&self, _ctx: CommandContext) -> SlashResult<CommandOutput> {
        let orgs = self.multi_agent.list_organizations().await
            .map_err(|e| SlashError::Execution(e.to_string()))?;

        if orgs.is_empty() {
            return Ok(CommandOutput::new(
                "╭────────────────────────╮\n\
                 │ Available Agents       │\n\
                 ╰────────────────────────╯\n\n\
                 No organizations found.\n\
                 Use /team start to create a team."
            ));
        }

        let mut output = String::from(
            "╭────────────────────────╮\n\
             │ Available Agents       │\n\
             ╰────────────────────────╯\n\n"
        );

        let mut total_teams = 0usize;
        let mut total_members = 0usize;

        for org in &orgs {
            output.push_str(&format!("🏢 {} ({})\n\n", org.name, org.key));

            let roles = self.multi_agent.list_roles(org.id).await
                .map_err(|e| SlashError::Execution(e.to_string()))?;

            if !roles.is_empty() {
                output.push_str("  System Agents:\n");
                for role in &roles {
                    let desc = if role.description.is_empty() {
                        String::new()
                    } else {
                        format!(" — {}", role.description)
                    };
                    output.push_str(&format!("    {} {}\n", role.name, desc));
                }
                output.push('\n');
            }

            let teams = self.multi_agent.list_teams(org.id).await
                .map_err(|e| SlashError::Execution(e.to_string()))?;

            for team in &teams {
                total_teams += 1;
                let status_icon = match team.state.as_str() {
                    "ACTIVE" => "▶️",
                    "READY" => "✅",
                    "COMPLETED" => "🏁",
                    "ARCHIVED" => "📦",
                    _ => "🆕",
                };
                output.push_str(&format!(
                    "  {} Team: {} ({}) — State: {}\n     Goal: {}\n",
                    status_icon, team.name, team.key, team.state.as_str(), team.goal
                ));

                let members = self.multi_agent.list_members(team.id).await
                    .map_err(|e| SlashError::Execution(e.to_string()))?;
                for member in &members {
                    total_members += 1;
                    let icon = match member.state.as_str() {
                        "WORKING" => "🔄",
                        "WAITING" => "⏳",
                        "COMPLETED" => "✅",
                        _ => "🟢",
                    };
                    output.push_str(&format!(
                        "     {} Agent {} — {}\n",
                        icon, member.agent_id, member.state.as_str()
                    ));
                }
            }
            output.push('\n');
        }

        output.push_str(&format!(
            "Summary: {} org(s), {} team(s), {} member(s)\n",
            orgs.len(), total_teams, total_members
        ));

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
    async fn test_agents_empty() {
        let agent = setup_agent();
        let cmd = AgentsCommand::new(agent);
        let ctx = CommandContext {
            line: "/agents".into(),
            args: vec![],
            workspace: ".".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        assert!(result.response.contains("No organizations"));
    }

    #[tokio::test]
    async fn test_agents_with_org() {
        let agent = setup_agent();
        let org = Organization::new("engineering", "Engineering", "admin");
        agent.create_organization(org).await.unwrap();

        let cmd = AgentsCommand::new(agent);
        let ctx = CommandContext {
            line: "/agents".into(),
            args: vec![],
            workspace: ".".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        assert!(result.response.contains("Engineering"));
    }

    #[tokio::test]
    async fn test_agents_with_team() {
        let agent = setup_agent();
        let org = Organization::new("engineering", "Engineering", "admin");
        let org = agent.create_organization(org).await.unwrap();

        let req = CreateTeamRequest::new(org.id, "refactor", "Refactor Team", "Refactor auth module", "admin");
        agent.create_team(req).await.unwrap();

        let cmd = AgentsCommand::new(agent);
        let ctx = CommandContext {
            line: "/agents".into(),
            args: vec![],
            workspace: ".".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        assert!(result.response.contains("Refactor Team"));
    }
}