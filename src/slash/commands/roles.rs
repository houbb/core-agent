//! `/roles` — 查看角色列表
//!
//! 查看所有可用角色及其能力要求。
//!
//! 用法：
//!   /roles                           — 查看所有角色
//!
//! 路由：Runtime（零模型调用）

use std::sync::Arc;

use async_trait::async_trait;
use core_agent_multi::MultiAgentManager;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, CommandAction, SlashCategory, SlashCommand,
    SlashResult, SlashError,
};

/// Roles 命令
#[derive(Clone)]
pub struct RolesCommand {
    multi_agent: Arc<MultiAgentManager>,
}

impl RolesCommand {
    pub fn new(multi_agent: Arc<MultiAgentManager>) -> Self {
        Self { multi_agent }
    }
}

#[async_trait]
impl SlashCommand for RolesCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "roles".into(),
            display_name: "Agent Roles".into(),
            description: "View all available roles and their capabilities".into(),
            usage: "/roles".into(),
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
                 │ Available Roles        │\n\
                 ╰────────────────────────╯\n\n\
                 No organizations found.\n\
                 Use /team start to create an organization and team."
            ));
        }

        let mut output = String::from(
            "╭────────────────────────╮\n\
             │ Available Roles        │\n\
             ╰────────────────────────╯\n\n"
        );

        for org in &orgs {
            output.push_str(&format!("Organization: {} ({})\n\n", org.name, org.key));

            let roles = self.multi_agent.list_roles(org.id).await
                .map_err(|e| SlashError::Execution(e.to_string()))?;

            if roles.is_empty() {
                output.push_str("  No roles defined yet.\n");
            } else {
                for role in &roles {
                    let icon = match role.key.as_str() {
                        "planner" => "🧠",
                        "coder" | "developer" => "🛠",
                        "reviewer" => "🔍",
                        "tester" => "🧪",
                        "security" => "🔒",
                        "researcher" => "📚",
                        "operator" => "⚙️",
                        _ => "📋",
                    };
                    output.push_str(&format!(
                        "  {} {} — {}\n     Key: {} | ID: {}\n",
                        icon, role.name, role.description, role.key, role.id
                    ));
                    if !role.required_capabilities.is_empty() {
                        output.push_str(&format!(
                            "     Required Capabilities: {}\n",
                            role.required_capabilities.iter().cloned().collect::<Vec<_>>().join(", ")
                        ));
                    }
                    output.push('\n');
                }
            }
        }

        output.push_str("Use /delegate <task> --role <role-key> to assign tasks to a specific role.\n");

        Ok(CommandOutput::new(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_agent_multi::{Organization, Role, InMemoryMultiAgentStore};

    fn setup() -> Arc<MultiAgentManager> {
        let store = Arc::new(InMemoryMultiAgentStore::default());
        MultiAgentManager::builder().store(store).build().into()
    }

    #[tokio::test]
    async fn test_roles_empty() {
        let agent = setup();
        let cmd = RolesCommand::new(agent);
        let ctx = CommandContext {
            line: "/roles".into(),
            args: vec![],
            workspace: ".".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        assert!(result.response.contains("No organizations"));
    }

    #[tokio::test]
    async fn test_roles_with_roles() {
        let agent = setup();
        let org = Organization::new("engineering", "Engineering", "admin");
        let org = agent.create_organization(org).await.unwrap();

        let role = Role::new(org.id, "planner", "Planner", "admin");
        agent.create_role(role).await.unwrap();

        let role = Role::new(org.id, "coder", "Coder", "admin");
        agent.create_role(role).await.unwrap();

        let cmd = RolesCommand::new(agent);
        let ctx = CommandContext {
            line: "/roles".into(),
            args: vec![],
            workspace: ".".into(),
            session_id: None,
            data: Default::default(),
        };
        let result = cmd.execute(ctx).await.unwrap();
        assert!(result.response.contains("Planner"));
        assert!(result.response.contains("Coder"));
    }
}