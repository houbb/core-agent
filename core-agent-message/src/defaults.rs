use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use serde_json::Value;
use uuid::Uuid;

use crate::domain::{AgentMessage, MessageStatus, MessageType, MessagePriority};
use crate::error::{MessageError, MessageResult};
use crate::infrastructure::{MessageBus, MessageStore};

// ── InMemoryMessageStore ──

#[derive(Default)]
struct MemoryState {
    messages: BTreeMap<Uuid, AgentMessage>,
}

#[derive(Default)]
pub struct InMemoryMessageStore {
    state: RwLock<MemoryState>,
}

impl InMemoryMessageStore {
    fn read(&self) -> MessageResult<std::sync::RwLockReadGuard<'_, MemoryState>> {
        self.state
            .read()
            .map_err(|_| MessageError::Internal("message store lock poisoned".into()))
    }

    fn write(&self) -> MessageResult<std::sync::RwLockWriteGuard<'_, MemoryState>> {
        self.state
            .write()
            .map_err(|_| MessageError::Internal("message store lock poisoned".into()))
    }
}

#[async_trait]
impl MessageStore for InMemoryMessageStore {
    async fn save(
        &self,
        message: &AgentMessage,
        expected_version: Option<u64>,
        _actor: &str,
    ) -> MessageResult<()> {
        let mut state = self.write()?;
        if let Some(current) = state.messages.get(&message.id) {
            if let Some(expected) = expected_version {
                if current.version != expected {
                    return Err(MessageError::Conflict("message version conflict".into()));
                }
            }
        }
        state.messages.insert(message.id, message.clone());
        Ok(())
    }

    async fn find(&self, id: Uuid) -> MessageResult<Option<AgentMessage>> {
        Ok(self.read()?.messages.get(&id).cloned())
    }

    async fn list_by_to_agent(
        &self,
        agent_id: Uuid,
        limit: usize,
    ) -> MessageResult<Vec<AgentMessage>> {
        let mut values: Vec<_> = self
            .read()?
            .messages
            .values()
            .filter(|msg| msg.to_agent_id == agent_id)
            .cloned()
            .collect();
        values.sort_by_key(|msg| (std::cmp::Reverse(msg.created_at), msg.id));
        values.truncate(limit);
        Ok(values)
    }

    async fn list_by_from_agent(
        &self,
        agent_id: Uuid,
        limit: usize,
    ) -> MessageResult<Vec<AgentMessage>> {
        let mut values: Vec<_> = self
            .read()?
            .messages
            .values()
            .filter(|msg| msg.from_agent_id == agent_id)
            .cloned()
            .collect();
        values.sort_by_key(|msg| (std::cmp::Reverse(msg.created_at), msg.id));
        values.truncate(limit);
        Ok(values)
    }

    async fn list_by_correlation(
        &self,
        correlation_id: Uuid,
    ) -> MessageResult<Vec<AgentMessage>> {
        let mut values: Vec<_> = self
            .read()?
            .messages
            .values()
            .filter(|msg| msg.correlation_id == Some(correlation_id))
            .cloned()
            .collect();
        values.sort_by_key(|msg| (msg.created_at, msg.id));
        Ok(values)
    }

    async fn mark_read(&self, message_id: Uuid, _actor: &str) -> MessageResult<bool> {
        let mut state = self.write()?;
        if let Some(msg) = state.messages.get_mut(&message_id) {
            if msg.status == MessageStatus::Read {
                return Ok(true);
            }
            msg.status = MessageStatus::Read;
            msg.version = msg.version.saturating_add(1);
            msg.updated_at = chrono::Utc::now();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn list_inbox(
        &self,
        agent_id: Uuid,
        limit: usize,
    ) -> MessageResult<Vec<AgentMessage>> {
        let mut values: Vec<_> = self
            .read()?
            .messages
            .values()
            .filter(|msg| {
                msg.to_agent_id == agent_id
                    && (msg.status == MessageStatus::Pending
                        || msg.status == MessageStatus::Delivered)
            })
            .cloned()
            .collect();
        values.sort_by_key(|msg| (std::cmp::Reverse(msg.priority as i32), std::cmp::Reverse(msg.created_at), msg.id));
        values.truncate(limit);
        Ok(values)
    }
}

// ── DefaultMessageBus ──

pub struct DefaultMessageBus {
    store: std::sync::Arc<dyn MessageStore>,
}

impl DefaultMessageBus {
    pub fn new(store: std::sync::Arc<dyn MessageStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl MessageBus for DefaultMessageBus {
    async fn send(&self, message: &AgentMessage, actor: &str) -> MessageResult<AgentMessage> {
        message.validate()?;
        self.store.save(message, None, actor).await?;
        Ok(message.clone())
    }

    async fn receive(
        &self,
        agent_id: Uuid,
        limit: usize,
    ) -> MessageResult<Vec<AgentMessage>> {
        let messages = self.store.list_inbox(agent_id, limit).await?;
        // Mark as Delivered
        for msg in &messages {
            let mut updated = msg.clone();
            updated.status = MessageStatus::Delivered;
            updated.version = msg.version.saturating_add(1);
            updated.updated_at = chrono::Utc::now();
            self.store
                .save(&updated, Some(msg.version), &msg.actor)
                .await?;
        }
        Ok(messages)
    }

    async fn broadcast(
        &self,
        from: Uuid,
        to_agents: &[Uuid],
        message_type: MessageType,
        intent: &str,
        payload: Value,
        actor: &str,
    ) -> MessageResult<Vec<AgentMessage>> {
        let mut results = Vec::new();
        for &to in to_agents {
            let msg = AgentMessage::new(
                from,
                to,
                message_type,
                intent.into(),
                payload.clone(),
                MessagePriority::Normal,
                actor.into(),
            )?;
            self.store.save(&msg, None, actor).await?;
            results.push(msg);
        }
        Ok(results)
    }
}