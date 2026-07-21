use std::sync::Arc;

use async_trait::async_trait;
use core_agent_message::{MessageManager, MessagePriority, MessageType};
use uuid::Uuid;

use crate::slash::{
    CommandContext, CommandMetadata, CommandOutput, SlashCategory, SlashCommand, SlashResult,
};

#[derive(Clone)]
pub struct MessageSendCommand {
    manager: Arc<MessageManager>,
}

impl MessageSendCommand {
    pub fn new(manager: Arc<MessageManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl SlashCommand for MessageSendCommand {
    fn metadata(&self) -> CommandMetadata {
        CommandMetadata {
            name: "message".into(),
            display_name: "Message Send".into(),
            description: "发送消息给 Agent".into(),
            usage: "/message send <to_agent_id> <text>".into(),
            category: SlashCategory::Orchestration,
            min_args: 3,
            max_args: 100,
            read_only: false,
            async_exec: false,
        }
    }

    fn category(&self) -> SlashCategory {
        SlashCategory::Orchestration
    }

    async fn validate(&self, args: &[String]) -> SlashResult<()> {
        if args.len() < 3 || args[0] != "send" {
            return Err(crate::slash::SlashError::InvalidArgument(
                "usage: /message send <to_agent_id> <text>".into(),
            ));
        }
        Uuid::parse_str(&args[1])
            .map_err(|_| crate::slash::SlashError::InvalidArgument("invalid UUID".into()))?;
        Ok(())
    }

    async fn execute(&self, ctx: CommandContext) -> SlashResult<CommandOutput> {
        let to = Uuid::parse_str(&ctx.args[1]).unwrap();
        let text = ctx.args[2..].join(" ");
        let from = Uuid::new_v4(); // system message

        let msg = self
            .manager
            .send(
                from,
                to,
                MessageType::Request,
                "MANUAL_MESSAGE",
                serde_json::json!({"text": text}),
                MessagePriority::Normal,
                "system",
            )
            .await
            .map_err(|e| crate::slash::SlashError::Execution(e.to_string()))?;

        Ok(CommandOutput::new(format!(
            "Message sent.\nID: {}\nTo: {}\nIntent: {}",
            msg.id, msg.to_agent_id, msg.intent
        )))
    }
}