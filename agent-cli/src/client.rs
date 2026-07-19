use std::pin::Pin;

use async_trait::async_trait;
use futures_core::Stream;
use uuid::Uuid;

use crate::{AgentEvent, AgentRequest, CliResult, SessionStatus, SessionSummary, Submission};

pub type EventStream = Pin<Box<dyn Stream<Item = CliResult<AgentEvent>> + Send>>;

#[async_trait]
pub trait AgentClient: Send + Sync {
    async fn send(&self, request: AgentRequest) -> CliResult<Submission>;
    async fn stream(&self, session_id: Uuid) -> CliResult<EventStream>;
    async fn resume(&self, session_id: Uuid) -> CliResult<EventStream>;
    async fn cancel(&self, session_id: Uuid) -> CliResult<bool>;
    async fn status(&self, session_id: Uuid) -> CliResult<SessionStatus>;
    async fn sessions(&self) -> CliResult<Vec<SessionSummary>>;
}

pub trait TerminalAgentClient: AgentClient + crate::ProfessionalAgentClient {}

impl<T> TerminalAgentClient for T where T: AgentClient + crate::ProfessionalAgentClient + ?Sized {}
