use std::sync::Arc;

use async_trait::async_trait;
use core_agent_message::MessageManager;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, SlashCategory, SlashCommand, SlashResult,
};

#[derive(Clone)]
pub struct MessageInboxCommand {
    manager: Arc<MessageManager>,
}

impl MessageInboxCommand {
    pub fn new(manager: Arc<MessageManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl SlashCommand for MessageInboxCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "message".into(),
            display_name: "Message Inbox".into(),
            description: "查看 Agent 消息收件箱".into(),
            usage: "/message inbox [agent_id]".into(),
            category: SlashCategory::Orchestration,
            min_args: 1,
            max_args: 2,
            read_only: true,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Orchestration
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.is_empty() || args[0] != "inbox" {
            return Err(crate::slash::SlashError::InvalidArgument(
                "usage: /message inbox [agent_id]".into(),
            ));
        }
        Ok(())
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        // If an agent_id is provided, use it; otherwise the inbox query
        // needs a concrete agent_id — use a default for system messages
        let agent_id = if ctx.args.len() >= 2 {
            match uuid::Uuid::parse_str(&ctx.args[1]) {
                Ok(id) => id,
                Err(_) => {
                    return Err(crate::slash::SlashError::InvalidArgument(
                        "invalid UUID".into(),
                    ))
                }
            }
        } else {
            // Without a specific agent_id, list all pending messages by receiver
            // For MVP, pick up messages directed to any recipient
            uuid::Uuid::nil()
        };

        let messages = self
            .manager
            .list_inbox(agent_id, 20)
            .await
            .map_err(|e| crate::slash::SlashError::Execution(e.to_string()))?;

        if messages.is_empty() {
            return Ok(CommandOutput::new(
                "Inbox is empty.\nNo pending messages.",
            ));
        }

        let mut lines = vec![
            "╭──────────────────────────────────────────────╮".into(),
            "│ Message Inbox                                 │".into(),
            "╰──────────────────────────────────────────────╯".into(),
            String::new(),
        ];
        for msg in &messages {
            let priority_icon = match msg.priority {
                core_agent_message::MessagePriority::Critical => "🔴",
                core_agent_message::MessagePriority::High => "🟠",
                core_agent_message::MessagePriority::Normal => "🟢",
                core_agent_message::MessagePriority::Low => "⚪",
            };
            lines.push(format!(
                "{priority_icon} [{}] ID: {} — {}",
                msg.message_type.as_str(),
                msg.id,
                msg.intent
            ));
            lines.push(format!("   From: {}", msg.from_agent_id));
            lines.push(format!("   Status: {}", msg.status.as_str()));
            lines.push(String::new());
        }
        lines.push(format!("Summary: {} message(s)", messages.len()));

        Ok(CommandOutput::new(lines.join("\n")))
    }
}