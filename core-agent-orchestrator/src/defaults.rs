use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use core_agent_message::{MessageManager, MessagePriority, MessageType};
use core_agent_subagent::{AgentRole, SubAgentManager, SubAgentStatus};

use crate::domain::{
    AggregatedResult, AgentInstanceRef, Orchestration, OrchestrationStatus, WorkerResult,
};
use crate::error::{OrchestratorError, OrchestratorResult};
use crate::infrastructure::{ExecutionStrategy, OrchestrationStore, ResultAggregator};

// ── SequentialStrategy ──

pub struct SequentialStrategy;

#[async_trait]
impl ExecutionStrategy for SequentialStrategy {
    fn name(&self) -> &'static str {
        "sequential"
    }

    async fn execute(
        &self,
        orchestration: &Orchestration,
        _subagent_manager: Arc<SubAgentManager>,
        _message_manager: Arc<MessageManager>,
    ) -> OrchestratorResult<AggregatedResult> {
        // Workers complete sequentially — in integration, they would be executed
        // via SubAgentRuntime; for the MVP, the strategy just asserts the workers exist
        if orchestration.worker_agents.is_empty() {
            return Err(OrchestratorError::StrategyExecution(
                "sequential strategy requires at least one worker".into(),
            ));
        }
        // Use the mock resolution
        let results = orchestration
            .worker_agents
            .iter()
            .map(|worker| WorkerResult {
                agent_id: worker.agent_id,
                agent_name: worker.agent_name.clone(),
                role: worker.role,
                status: SubAgentStatus::Completed,
                finding: format!("{} completed sequential task for: {}", worker.agent_name, orchestration.goal),
                confidence: 0.8,
            })
            .collect();
        DefaultResultAggregator.aggregate(results)
    }
}

// ── ParallelStrategy ──

pub struct ParallelStrategy;

#[async_trait]
impl ExecutionStrategy for ParallelStrategy {
    fn name(&self) -> &'static str {
        "parallel"
    }

    async fn execute(
        &self,
        orchestration: &Orchestration,
        _subagent_manager: Arc<SubAgentManager>,
        _message_manager: Arc<MessageManager>,
    ) -> OrchestratorResult<AggregatedResult> {
        if orchestration.worker_agents.is_empty() {
            return Err(OrchestratorError::StrategyExecution(
                "parallel strategy requires at least one worker".into(),
            ));
        }
        // All workers "run in parallel" — in integration via tokio::spawn
        let results = orchestration
            .worker_agents
            .iter()
            .map(|worker| WorkerResult {
                agent_id: worker.agent_id,
                agent_name: worker.agent_name.clone(),
                role: worker.role,
                status: SubAgentStatus::Completed,
                finding: format!("{} completed parallel analysis for: {}", worker.agent_name, orchestration.goal),
                confidence: 0.85,
            })
            .collect();
        DefaultResultAggregator.aggregate(results)
    }
}

// ── SupervisorStrategy ──

pub struct SupervisorStrategy;

#[async_trait]
impl ExecutionStrategy for SupervisorStrategy {
    fn name(&self) -> &'static str {
        "supervisor"
    }

    async fn execute(
        &self,
        orchestration: &Orchestration,
        subagent_manager: Arc<SubAgentManager>,
        message_manager: Arc<MessageManager>,
    ) -> OrchestratorResult<AggregatedResult> {
        if orchestration.worker_agents.is_empty() {
            return Err(OrchestratorError::StrategyExecution(
                "supervisor strategy requires workers to be created before execution".into(),
            ));
        }

        // Supervisor sends task messages to each worker
        for worker in &orchestration.worker_agents {
            message_manager
                .send(
                    orchestration.supervisor_agent_id,
                    worker.agent_id,
                    MessageType::Request,
                    "TASK_ASSIGNMENT",
                    serde_json::json!({
                        "goal": orchestration.goal,
                        "role": worker.role.as_str()
                    }),
                    MessagePriority::High,
                    &orchestration.actor,
                )
                .await?;

            // Mark worker as started
            subagent_manager
                .start(worker.agent_id, &orchestration.actor)
                .await?;
        }

        // Collect results (in production this would wait for workers to respond)
        let mut results = Vec::new();
        for worker in &orchestration.worker_agents {
            // Receive the task ack from each worker
            let _messages = message_manager
                .receive(worker.agent_id, 1)
                .await?;

            let finding = format!(
                "{} (role={}) analyzed: {}",
                worker.agent_name,
                worker.role.as_str(),
                orchestration.goal
            );

            let wr = WorkerResult {
                agent_id: worker.agent_id,
                agent_name: worker.agent_name.clone(),
                role: worker.role,
                status: SubAgentStatus::Completed,
                finding,
                confidence: 0.9,
            };

            // Mark worker as completed
            let mut instance = subagent_manager
                .find(worker.agent_id)
                .await?
                .ok_or_else(|| OrchestratorError::not_found(worker.agent_id))?;

            // Stop -> then we'd transition to completed in a full lifecycle
            subagent_manager
                .stop(worker.agent_id, &orchestration.actor)
                .await?;

            results.push(wr);
        }

        DefaultResultAggregator.aggregate(results)
    }
}

// ── DefaultResultAggregator ──

pub struct DefaultResultAggregator;

impl ResultAggregator for DefaultResultAggregator {
    fn aggregate(&self, results: Vec<WorkerResult>) -> OrchestratorResult<AggregatedResult> {
        if results.is_empty() {
            return Err(OrchestratorError::Aggregation(
                "no worker results to aggregate".into(),
            ));
        }

        let confidence = results.iter().map(|r| r.confidence).sum::<f64>() / results.len() as f64;
        let findings: Vec<String> = results
            .iter()
            .map(|r| format!("[{}] {}", r.agent_name, r.finding))
            .collect();
        let summary = format!(
            "Root Cause Analysis — {} findings. Conclusion: {}",
            results.len(),
            findings.join(" | ")
        );

        Ok(AggregatedResult {
            summary,
            confidence: (confidence * 100.0).round() / 100.0,
            details: results,
            metadata: BTreeMap::new(),
        })
    }
}

// ── InMemoryOrchestrationStore ──

#[derive(Default)]
struct MemoryState {
    orchestrations: BTreeMap<Uuid, Orchestration>,
}

#[derive(Default)]
pub struct InMemoryOrchestrationStore {
    state: std::sync::RwLock<MemoryState>,
}

impl InMemoryOrchestrationStore {
    fn read(&self) -> OrchestratorResult<std::sync::RwLockReadGuard<'_, MemoryState>> {
        self.state
            .read()
            .map_err(|_| OrchestratorError::Internal("orchestration store lock poisoned".into()))
    }

    fn write(&self) -> OrchestratorResult<std::sync::RwLockWriteGuard<'_, MemoryState>> {
        self.state
            .write()
            .map_err(|_| OrchestratorError::Internal("orchestration store lock poisoned".into()))
    }
}

#[async_trait]
impl OrchestrationStore for InMemoryOrchestrationStore {
    async fn save(
        &self,
        orchestration: &Orchestration,
        expected_version: Option<u64>,
        _actor: &str,
    ) -> OrchestratorResult<()> {
        let mut state = self.write()?;
        if let Some(current) = state.orchestrations.get(&orchestration.id) {
            if let Some(expected) = expected_version {
                if current.version != expected {
                    return Err(OrchestratorError::Conflict(
                        "orchestration version conflict".into(),
                    ));
                }
            }
        }
        state
            .orchestrations
            .insert(orchestration.id, orchestration.clone());
        Ok(())
    }

    async fn find(&self, id: Uuid) -> OrchestratorResult<Option<Orchestration>> {
        Ok(self.read()?.orchestrations.get(&id).cloned())
    }

    async fn list_by_supervisor(
        &self,
        supervisor_id: Uuid,
    ) -> OrchestratorResult<Vec<Orchestration>> {
        let mut values: Vec<_> = self
            .read()?
            .orchestrations
            .values()
            .filter(|o| o.supervisor_agent_id == supervisor_id)
            .cloned()
            .collect();
        values.sort_by_key(|o| (std::cmp::Reverse(o.created_at), o.id));
        Ok(values)
    }

    async fn list_by_status(
        &self,
        status: OrchestrationStatus,
    ) -> OrchestratorResult<Vec<Orchestration>> {
        let mut values: Vec<_> = self
            .read()?
            .orchestrations
            .values()
            .filter(|o| o.status == status)
            .cloned()
            .collect();
        values.sort_by_key(|o| (std::cmp::Reverse(o.updated_at), o.id));
        Ok(values)
    }

    async fn list_all(&self) -> OrchestratorResult<Vec<Orchestration>> {
        let mut values: Vec<_> = self
            .read()?
            .orchestrations
            .values()
            .cloned()
            .collect();
        values.sort_by_key(|o| (std::cmp::Reverse(o.created_at), o.id));
        Ok(values)
    }
}