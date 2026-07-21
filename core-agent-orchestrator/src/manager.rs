use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use core_agent_message::MessageManager;
use core_agent_subagent::{InstanceType, SubAgentManager, SubAgentStatus};

use crate::defaults::{
    DefaultResultAggregator, InMemoryOrchestrationStore, ParallelStrategy, SequentialStrategy,
    SupervisorStrategy,
};
use crate::domain::{
    AggregatedResult, AgentInstanceRef, Orchestration, OrchestrationStrategy, OrchestrationStatus,
};
use crate::error::{OrchestratorError, OrchestratorResult};
use crate::infrastructure::{
    ExecutionStrategy, OrchestrationObserver, OrchestrationStore, ResultAggregator,
};

pub struct OrchestratorManagerBuilder {
    store: Arc<dyn OrchestrationStore>,
    subagent_manager: Arc<SubAgentManager>,
    message_manager: Arc<MessageManager>,
    strategies: HashMap<String, Arc<dyn ExecutionStrategy>>,
    aggregator: Arc<dyn ResultAggregator>,
    observers: Vec<Arc<dyn OrchestrationObserver>>,
}

impl Default for OrchestratorManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryOrchestrationStore::default()),
            subagent_manager: Arc::new(SubAgentManager::builder().build()),
            message_manager: Arc::new(MessageManager::builder().build()),
            strategies: {
                let mut m = HashMap::new();
                m.insert("sequential".into(), Arc::new(SequentialStrategy) as Arc<dyn ExecutionStrategy>);
                m.insert("parallel".into(), Arc::new(ParallelStrategy) as Arc<dyn ExecutionStrategy>);
                m.insert("supervisor".into(), Arc::new(SupervisorStrategy) as Arc<dyn ExecutionStrategy>);
                m
            },
            aggregator: Arc::new(DefaultResultAggregator),
            observers: Vec::new(),
        }
    }
}

impl OrchestratorManagerBuilder {
    pub fn store(mut self, value: Arc<dyn OrchestrationStore>) -> Self {
        self.store = value;
        self
    }

    pub fn subagent_manager(mut self, value: Arc<SubAgentManager>) -> Self {
        self.subagent_manager = value;
        self
    }

    pub fn message_manager(mut self, value: Arc<MessageManager>) -> Self {
        self.message_manager = value;
        self
    }

    pub fn strategy(mut self, value: Arc<dyn ExecutionStrategy>) -> Self {
        self.strategies.insert(value.name().into(), value);
        self
    }

    pub fn aggregator(mut self, value: Arc<dyn ResultAggregator>) -> Self {
        self.aggregator = value;
        self
    }

    pub fn observer(mut self, value: Arc<dyn OrchestrationObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> OrchestratorManager {
        OrchestratorManager {
            store: self.store,
            subagent_manager: self.subagent_manager,
            message_manager: self.message_manager,
            strategies: self.strategies,
            aggregator: self.aggregator,
            observers: self.observers,
            live: Mutex::new(HashMap::new()),
        }
    }
}

pub struct OrchestratorManager {
    store: Arc<dyn OrchestrationStore>,
    subagent_manager: Arc<SubAgentManager>,
    message_manager: Arc<MessageManager>,
    strategies: HashMap<String, Arc<dyn ExecutionStrategy>>,
    aggregator: Arc<dyn ResultAggregator>,
    observers: Vec<Arc<dyn OrchestrationObserver>>,
    live: Mutex<HashMap<Uuid, ()>>,
}

impl OrchestratorManager {
    pub fn builder() -> OrchestratorManagerBuilder {
        OrchestratorManagerBuilder::default()
    }

    pub async fn create(
        &self,
        goal: String,
        strategy: OrchestrationStrategy,
        supervisor_agent_id: Uuid,
        actor: &str,
    ) -> OrchestratorResult<Orchestration> {
        let orchestration = Orchestration::new(goal, strategy, supervisor_agent_id, actor.into())?;
        self.store.save(&orchestration, None, actor).await?;
        Ok(orchestration)
    }

    pub async fn start(
        &self,
        orchestration_id: Uuid,
        actor: &str,
    ) -> OrchestratorResult<Orchestration> {
        let mut orchestration = self.required(orchestration_id).await?;
        if orchestration.status != OrchestrationStatus::Created {
            return Err(OrchestratorError::InvalidState(format!(
                "cannot start {} orchestration",
                orchestration.status.as_str()
            )));
        }

        // Enter live
        {
            let mut live = self.live.lock().map_err(|_| {
                OrchestratorError::Internal("orchestration lock poisoned".into())
            })?;
            if live.contains_key(&orchestration_id) {
                return Err(OrchestratorError::Conflict(format!(
                    "orchestration {orchestration_id} is already running"
                )));
            }
            live.insert(orchestration_id, ());
        }

        orchestration.status = OrchestrationStatus::Running;
        orchestration.version = orchestration.version.saturating_add(1);
        orchestration.updated_at = chrono::Utc::now();
        self.store
            .save(&orchestration, Some(orchestration.version - 1), actor)
            .await?;

        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_started(&orchestration)));
        }

        // Execute strategy
        let strategy_name = orchestration.strategy.as_str();
        let strategy = self.strategies.get(strategy_name).ok_or_else(|| {
            OrchestratorError::StrategyExecution(format!(
                "unknown strategy: {strategy_name}"
            ))
        })?;

        let result = strategy
            .execute(
                &orchestration,
                self.subagent_manager.clone(),
                self.message_manager.clone(),
            )
            .await?;

        // Aggregate and update
        orchestration.result = Some(result.clone());
        orchestration.status = OrchestrationStatus::Completed;
        orchestration.version = orchestration.version.saturating_add(1);
        orchestration.updated_at = chrono::Utc::now();
        let expected = orchestration.version - 1;
        self.store.save(&orchestration, Some(expected), actor).await?;

        // Leave live
        {
            let mut live = self.live.lock().map_err(|_| {
                OrchestratorError::Internal("orchestration lock poisoned".into())
            })?;
            live.remove(&orchestration_id);
        }

        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| {
                observer.on_completed(&orchestration, &result)
            }));
        }

        Ok(orchestration)
    }

    pub async fn add_worker(
        &self,
        orchestration_id: Uuid,
        agent_ref: AgentInstanceRef,
        actor: &str,
    ) -> OrchestratorResult<Orchestration> {
        let mut orchestration = self.required(orchestration_id).await?;
        if orchestration.status != OrchestrationStatus::Created {
            return Err(OrchestratorError::InvalidState(
                "can only add workers to Created orchestration".into(),
            ));
        }
        orchestration.worker_agents.push(agent_ref);
        orchestration.version = orchestration.version.saturating_add(1);
        orchestration.actor = actor.into();
        orchestration.updated_at = chrono::Utc::now();
        let expected = orchestration.version - 1;
        self.store.save(&orchestration, Some(expected), actor).await?;
        Ok(orchestration)
    }

    pub async fn get_result(
        &self,
        orchestration_id: Uuid,
    ) -> OrchestratorResult<Option<AggregatedResult>> {
        Ok(self
            .store
            .find(orchestration_id)
            .await?
            .and_then(|o| o.result))
    }

    pub async fn list_by_supervisor(
        &self,
        supervisor_id: Uuid,
    ) -> OrchestratorResult<Vec<Orchestration>> {
        self.store.list_by_supervisor(supervisor_id).await
    }

    pub async fn list_all(&self) -> OrchestratorResult<Vec<Orchestration>> {
        self.store.list_all().await
    }

    /// Convenience: create workers, create orchestration, start it — all in one call.
    pub async fn supervise(
        &self,
        goal: String,
        workers: Vec<(String, core_agent_subagent::AgentRole)>,
        supervisor_agent_id: Uuid,
        actor: &str,
    ) -> OrchestratorResult<AggregatedResult> {
        let mut orchestration = self
            .create(goal, OrchestrationStrategy::Supervisor, supervisor_agent_id, actor)
            .await?;

        for (name, role) in &workers {
            let instance = self
                .subagent_manager
                .create(
                    name.clone(),
                    InstanceType::Worker,
                    *role,
                    Some(supervisor_agent_id),
                    Some(supervisor_agent_id),
                    serde_json::json!({}),
                    actor,
                )
                .await?;
            orchestration = self
                .add_worker(
                    orchestration.id,
                    AgentInstanceRef {
                        agent_id: instance.id,
                        agent_name: name.clone(),
                        role: *role,
                    },
                    actor,
                )
                .await?;
        }

        let completed = self.start(orchestration.id, actor).await?;
        completed
            .result
            .ok_or_else(|| OrchestratorError::Internal("orchestration completed without result".into()))
    }

    async fn required(&self, id: Uuid) -> OrchestratorResult<Orchestration> {
        self.store
            .find(id)
            .await?
            .ok_or_else(|| OrchestratorError::not_found(id))
    }
}