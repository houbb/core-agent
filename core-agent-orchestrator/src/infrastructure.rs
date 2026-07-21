use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use core_agent_message::MessageManager;
use core_agent_subagent::SubAgentManager;

use crate::domain::{AggregatedResult, AgentInstanceRef, Orchestration, OrchestrationStatus, WorkerResult};
use crate::error::OrchestratorResult;

// ── OrchestrationStore ──

#[async_trait]
pub trait OrchestrationStore: Send + Sync {
    async fn save(
        &self,
        orchestration: &Orchestration,
        expected_version: Option<u64>,
        actor: &str,
    ) -> OrchestratorResult<()>;

    async fn find(&self, id: Uuid) -> OrchestratorResult<Option<Orchestration>>;

    async fn list_by_supervisor(
        &self,
        supervisor_id: Uuid,
    ) -> OrchestratorResult<Vec<Orchestration>>;

    async fn list_by_status(
        &self,
        status: OrchestrationStatus,
    ) -> OrchestratorResult<Vec<Orchestration>>;

    async fn list_all(&self) -> OrchestratorResult<Vec<Orchestration>>;
}

// ── ExecutionStrategy ──

#[async_trait]
pub trait ExecutionStrategy: Send + Sync {
    fn name(&self) -> &'static str;

    async fn execute(
        &self,
        orchestration: &Orchestration,
        subagent_manager: Arc<SubAgentManager>,
        message_manager: Arc<MessageManager>,
    ) -> OrchestratorResult<AggregatedResult>;
}

// ── ResultAggregator ──

pub trait ResultAggregator: Send + Sync {
    fn aggregate(&self, results: Vec<WorkerResult>) -> OrchestratorResult<AggregatedResult>;
}

// ── OrchestrationObserver ──

pub trait OrchestrationObserver: Send + Sync {
    fn on_started(&self, orchestration: &Orchestration);
    fn on_worker_completed(&self, orchestration_id: Uuid, worker: &WorkerResult);
    fn on_completed(&self, orchestration: &Orchestration, result: &AggregatedResult);
    fn on_failed(&self, orchestration_id: Uuid, error: &str);
}