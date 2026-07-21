use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, RwLock};

use serde_json::Value;
use uuid::Uuid;

use crate::defaults::{
    DefaultSubAgentFactory, DefaultSubAgentLifecycle, InMemorySubAgentStore,
};
use crate::domain::{AgentInstance, AgentRole, InstanceType, SubAgentStatus};
use crate::error::{SubAgentError, SubAgentResult};
use crate::infrastructure::{
    SubAgentFactory, SubAgentInterceptor, SubAgentLifecycle, SubAgentObservation,
    SubAgentObserver, SubAgentOperation, SubAgentStage, SubAgentStore,
};

pub struct SubAgentManagerBuilder {
    store: Arc<dyn SubAgentStore>,
    lifecycle: Arc<dyn SubAgentLifecycle>,
    factory: Arc<dyn SubAgentFactory>,
    interceptors: Vec<Arc<dyn SubAgentInterceptor>>,
    observers: Vec<Arc<dyn SubAgentObserver>>,
}

impl Default for SubAgentManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemorySubAgentStore::default()),
            lifecycle: Arc::new(DefaultSubAgentLifecycle),
            factory: Arc::new(DefaultSubAgentFactory),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl SubAgentManagerBuilder {
    pub fn store(mut self, value: Arc<dyn SubAgentStore>) -> Self {
        self.store = value;
        self
    }

    pub fn lifecycle(mut self, value: Arc<dyn SubAgentLifecycle>) -> Self {
        self.lifecycle = value;
        self
    }

    pub fn factory(mut self, value: Arc<dyn SubAgentFactory>) -> Self {
        self.factory = value;
        self
    }

    pub fn interceptor(mut self, value: Arc<dyn SubAgentInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }

    pub fn observer(mut self, value: Arc<dyn SubAgentObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> SubAgentManager {
        SubAgentManager {
            store: self.store,
            lifecycle: self.lifecycle,
            factory: self.factory,
            interceptors: self.interceptors,
            observers: self.observers,
            live: RwLock::new(HashMap::new()),
        }
    }
}

pub struct SubAgentManager {
    store: Arc<dyn SubAgentStore>,
    lifecycle: Arc<dyn SubAgentLifecycle>,
    factory: Arc<dyn SubAgentFactory>,
    interceptors: Vec<Arc<dyn SubAgentInterceptor>>,
    observers: Vec<Arc<dyn SubAgentObserver>>,
    live: RwLock<HashMap<Uuid, ()>>,
}

impl SubAgentManager {
    pub fn builder() -> SubAgentManagerBuilder {
        SubAgentManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn SubAgentStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn create(
        &self,
        name: String,
        instance_type: InstanceType,
        role: AgentRole,
        parent_agent_id: Option<Uuid>,
        supervisor_agent_id: Option<Uuid>,
        config: Value,
        actor: &str,
    ) -> SubAgentResult<AgentInstance> {
        crate::domain::validate_actor(actor)?;
        let mut instance = self.factory.create(
            name,
            instance_type,
            role,
            parent_agent_id,
            supervisor_agent_id,
            config,
            actor.into(),
        )?;
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| interceptor.before_create(&mut instance)))
                .map_err(|_| SubAgentError::Internal("subagent interceptor panicked".into()))??;
        }
        self.lifecycle.transition(
            &mut instance,
            SubAgentStatus::Initialized,
            actor,
            "instance created",
        )?;
        self.store.save(&instance, None, actor).await?;
        self.notify(
            SubAgentOperation::Create,
            SubAgentStage::Persistence,
            true,
            &instance,
            None,
        );
        Ok(instance)
    }

    pub async fn start(&self, id: Uuid, actor: &str) -> SubAgentResult<AgentInstance> {
        crate::domain::validate_actor(actor)?;
        let mut instance = self.required(id).await?;
        self.lifecycle.transition(
            &mut instance,
            SubAgentStatus::Running,
            actor,
            "instance started",
        )?;
        let expected = instance.version - 1;
        self.store.save(&instance, Some(expected), actor).await?;
        self.notify(
            SubAgentOperation::Start,
            SubAgentStage::Lifecycle,
            true,
            &instance,
            None,
        );
        Ok(instance)
    }

    pub async fn stop(&self, id: Uuid, actor: &str) -> SubAgentResult<AgentInstance> {
        crate::domain::validate_actor(actor)?;
        let mut instance = self.required(id).await?;
        self.lifecycle.transition(
            &mut instance,
            SubAgentStatus::Waiting,
            actor,
            "instance stopped",
        )?;
        let expected = instance.version - 1;
        self.store.save(&instance, Some(expected), actor).await?;
        self.notify(
            SubAgentOperation::Stop,
            SubAgentStage::Lifecycle,
            true,
            &instance,
            None,
        );
        Ok(instance)
    }

    pub async fn destroy(&self, id: Uuid, actor: &str) -> SubAgentResult<AgentInstance> {
        crate::domain::validate_actor(actor)?;
        let mut instance = self.required(id).await?;
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| interceptor.before_destroy(&instance)))
                .map_err(|_| SubAgentError::Internal("subagent interceptor panicked".into()))??;
        }
        self.lifecycle.transition(
            &mut instance,
            SubAgentStatus::Destroyed,
            actor,
            "instance destroyed",
        )?;
        let expected = instance.version - 1;
        self.store.save(&instance, Some(expected), actor).await?;
        self.notify(
            SubAgentOperation::Destroy,
            SubAgentStage::Lifecycle,
            true,
            &instance,
            None,
        );
        Ok(instance)
    }

    pub async fn find(&self, id: Uuid) -> SubAgentResult<Option<AgentInstance>> {
        self.store.find(id).await
    }

    pub async fn list_by_parent(&self, parent_id: Uuid) -> SubAgentResult<Vec<AgentInstance>> {
        self.store.list_by_parent(parent_id).await
    }

    pub async fn list_by_supervisor(
        &self,
        supervisor_id: Uuid,
    ) -> SubAgentResult<Vec<AgentInstance>> {
        self.store.list_by_supervisor(supervisor_id).await
    }

    pub async fn list_by_status(
        &self,
        status: SubAgentStatus,
    ) -> SubAgentResult<Vec<AgentInstance>> {
        self.store.list_by_status(status).await
    }

    pub async fn list_all(&self) -> SubAgentResult<Vec<AgentInstance>> {
        self.store.list_all().await
    }

    async fn required(&self, id: Uuid) -> SubAgentResult<AgentInstance> {
        self.store
            .find(id)
            .await?
            .ok_or_else(|| SubAgentError::not_found(id))
    }

    fn notify(
        &self,
        operation: SubAgentOperation,
        stage: SubAgentStage,
        success: bool,
        instance: &AgentInstance,
        message: Option<String>,
    ) {
        let observation = SubAgentObservation {
            operation,
            stage,
            success,
            agent_id: instance.id,
            status: instance.status,
            actor: instance.actor.clone(),
            message,
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }
}