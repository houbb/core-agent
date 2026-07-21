use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Arc;

use serde_json::Value;
use uuid::Uuid;

use crate::defaults::{DefaultMessageBus, InMemoryMessageStore};
use crate::domain::{AgentMessage, MessageType, MessagePriority};
use crate::error::{MessageError, MessageResult};
use crate::infrastructure::{MessageBus, MessageInterceptor, MessageObserver, MessageStore};

pub struct MessageManagerBuilder {
    store: Arc<dyn MessageStore>,
    bus: Arc<dyn MessageBus>,
    interceptors: Vec<Arc<dyn MessageInterceptor>>,
    observers: Vec<Arc<dyn MessageObserver>>,
}

impl Default for MessageManagerBuilder {
    fn default() -> Self {
        let store: Arc<dyn MessageStore> = Arc::new(InMemoryMessageStore::default());
        let bus = Arc::new(DefaultMessageBus::new(store.clone()));
        Self {
            store,
            bus,
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl MessageManagerBuilder {
    pub fn store(mut self, value: Arc<dyn MessageStore>) -> Self {
        self.store = value;
        self.bus = Arc::new(DefaultMessageBus::new(self.store.clone()));
        self
    }

    pub fn bus(mut self, value: Arc<dyn MessageBus>) -> Self {
        self.bus = value;
        self
    }

    pub fn interceptor(mut self, value: Arc<dyn MessageInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }

    pub fn observer(mut self, value: Arc<dyn MessageObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> MessageManager {
        MessageManager {
            store: self.store,
            bus: self.bus,
            interceptors: self.interceptors,
            observers: self.observers,
        }
    }
}

pub struct MessageManager {
    store: Arc<dyn MessageStore>,
    bus: Arc<dyn MessageBus>,
    interceptors: Vec<Arc<dyn MessageInterceptor>>,
    observers: Vec<Arc<dyn MessageObserver>>,
}

impl MessageManager {
    pub fn builder() -> MessageManagerBuilder {
        MessageManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn MessageStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn send(
        &self,
        from: Uuid,
        to: Uuid,
        message_type: MessageType,
        intent: &str,
        payload: Value,
        priority: MessagePriority,
        actor: &str,
    ) -> MessageResult<AgentMessage> {
        let mut message = AgentMessage::new(
            from,
            to,
            message_type,
            intent.into(),
            payload,
            priority,
            actor.into(),
        )?;
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| interceptor.before_send(&mut message)))
                .map_err(|_| MessageError::Internal("message interceptor panicked".into()))??;
        }
        let result = self.bus.send(&message, actor).await?;
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_message_sent(&result)));
        }
        Ok(result)
    }

    pub async fn receive(
        &self,
        agent_id: Uuid,
        limit: usize,
    ) -> MessageResult<Vec<AgentMessage>> {
        self.bus.receive(agent_id, limit).await
    }

    pub async fn broadcast(
        &self,
        from: Uuid,
        to_agents: &[Uuid],
        message_type: MessageType,
        intent: &str,
        payload: Value,
        actor: &str,
    ) -> MessageResult<Vec<AgentMessage>> {
        let results = self
            .bus
            .broadcast(from, to_agents, message_type, intent, payload, actor)
            .await?;
        for observer in &self.observers {
            for msg in &results {
                let _ = catch_unwind(AssertUnwindSafe(|| observer.on_message_sent(msg)));
            }
        }
        Ok(results)
    }

    pub async fn reply_to(
        &self,
        original: &AgentMessage,
        payload: Value,
        actor: &str,
    ) -> MessageResult<AgentMessage> {
        let mut reply = AgentMessage::new(
            original.to_agent_id,
            original.from_agent_id,
            MessageType::Response,
            format!("{}_REPLY", original.intent),
            payload,
            original.priority,
            actor.into(),
        )?;
        reply.correlation_id = Some(original.id);
        self.bus.send(&reply, actor).await
    }

    pub async fn mark_read(&self, message_id: Uuid, actor: &str) -> MessageResult<bool> {
        self.store.mark_read(message_id, actor).await
    }

    pub async fn list_by_agent(
        &self,
        agent_id: Uuid,
        limit: usize,
    ) -> MessageResult<Vec<AgentMessage>> {
        let mut received = self.store.list_by_to_agent(agent_id, limit).await?;
        let sent = self.store.list_by_from_agent(agent_id, limit).await?;
        received.extend(sent);
        received.sort_by_key(|msg| (std::cmp::Reverse(msg.created_at), msg.id));
        received.truncate(limit);
        Ok(received)
    }

    pub async fn list_by_correlation(
        &self,
        correlation_id: Uuid,
    ) -> MessageResult<Vec<AgentMessage>> {
        self.store.list_by_correlation(correlation_id).await
    }

    pub async fn list_inbox(
        &self,
        agent_id: Uuid,
        limit: usize,
    ) -> MessageResult<Vec<AgentMessage>> {
        self.store.list_inbox(agent_id, limit).await
    }
}