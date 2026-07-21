use std::collections::{BTreeSet, HashMap};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::domain::{
    PermissionDecision, ToolCapability, ToolDefinition, ToolExecutionRecord, ToolLifecycleStatus,
    ToolProviderDefinition, ToolRequest, ToolResult,
};
use crate::error::{ToolError, ToolRuntimeResult};
use crate::infrastructure::{
    AllowAllToolPolicy, DefaultToolExecutor, DefaultToolPermission, DefaultToolResultMapper,
    InMemoryToolCatalog, InMemoryToolLifecycle, InMemoryToolRegistry, JsonSchemaToolValidator,
    Tool, ToolCatalog, ToolContext, ToolExecutor, ToolInterceptor, ToolLifecycle, ToolObservation,
    ToolObserver, ToolPermission, ToolPolicy, ToolProvider, ToolRegistration, ToolRegistry,
    ToolResultMapper, ToolStage, ToolValidator,
};

pub struct ToolManagerBuilder {
    catalog: Arc<dyn ToolCatalog>,
    registry: Arc<dyn ToolRegistry>,
    executor: Arc<dyn ToolExecutor>,
    permission: Arc<dyn ToolPermission>,
    validator: Arc<dyn ToolValidator>,
    mapper: Arc<dyn ToolResultMapper>,
    lifecycle: Arc<dyn ToolLifecycle>,
    policy: Arc<dyn ToolPolicy>,
    interceptors: Vec<Arc<dyn ToolInterceptor>>,
    observers: Vec<Arc<dyn ToolObserver>>,
}

impl Default for ToolManagerBuilder {
    fn default() -> Self {
        Self {
            catalog: Arc::new(InMemoryToolCatalog::default()),
            registry: Arc::new(InMemoryToolRegistry::default()),
            executor: Arc::new(DefaultToolExecutor),
            permission: Arc::new(DefaultToolPermission),
            validator: Arc::new(JsonSchemaToolValidator),
            mapper: Arc::new(DefaultToolResultMapper),
            lifecycle: Arc::new(InMemoryToolLifecycle::default()),
            policy: Arc::new(AllowAllToolPolicy),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl ToolManagerBuilder {
    pub fn catalog(mut self, value: Arc<dyn ToolCatalog>) -> Self {
        self.catalog = value;
        self
    }

    pub fn registry(mut self, value: Arc<dyn ToolRegistry>) -> Self {
        self.registry = value;
        self
    }

    pub fn executor(mut self, value: Arc<dyn ToolExecutor>) -> Self {
        self.executor = value;
        self
    }

    pub fn permission(mut self, value: Arc<dyn ToolPermission>) -> Self {
        self.permission = value;
        self
    }

    pub fn validator(mut self, value: Arc<dyn ToolValidator>) -> Self {
        self.validator = value;
        self
    }

    pub fn mapper(mut self, value: Arc<dyn ToolResultMapper>) -> Self {
        self.mapper = value;
        self
    }

    pub fn lifecycle(mut self, value: Arc<dyn ToolLifecycle>) -> Self {
        self.lifecycle = value;
        self
    }

    pub fn policy(mut self, value: Arc<dyn ToolPolicy>) -> Self {
        self.policy = value;
        self
    }

    pub fn interceptor(mut self, value: Arc<dyn ToolInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }

    pub fn observer(mut self, value: Arc<dyn ToolObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> ToolManager {
        ToolManager {
            catalog: self.catalog,
            registry: self.registry,
            executor: self.executor,
            permission: self.permission,
            validator: self.validator,
            mapper: self.mapper,
            lifecycle: self.lifecycle,
            policy: self.policy,
            interceptors: self.interceptors,
            observers: self.observers,
            in_flight: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

pub struct ToolManager {
    catalog: Arc<dyn ToolCatalog>,
    registry: Arc<dyn ToolRegistry>,
    executor: Arc<dyn ToolExecutor>,
    permission: Arc<dyn ToolPermission>,
    validator: Arc<dyn ToolValidator>,
    mapper: Arc<dyn ToolResultMapper>,
    lifecycle: Arc<dyn ToolLifecycle>,
    policy: Arc<dyn ToolPolicy>,
    interceptors: Vec<Arc<dyn ToolInterceptor>>,
    observers: Vec<Arc<dyn ToolObserver>>,
    in_flight: Arc<RwLock<HashMap<Uuid, CancellationToken>>>,
}

impl ToolManager {
    pub fn builder() -> ToolManagerBuilder {
        ToolManagerBuilder::default()
    }

    pub async fn load_provider(&self, provider: &dyn ToolProvider) -> ToolRuntimeResult<usize> {
        let provider_definition = provider.definition();
        provider_definition.validate()?;
        let registrations = provider.discover().await?;
        let mut keys = BTreeSet::new();
        for registration in &registrations {
            self.validate_registration(&provider_definition, registration)?;
            if !keys.insert(registration.definition.key.clone()) {
                return Err(ToolError::Registry(format!(
                    "provider returned duplicate tool {}",
                    registration.definition.key
                )));
            }
        }
        self.catalog.upsert_provider(&provider_definition).await?;
        for registration in registrations.iter().cloned() {
            self.catalog.upsert_tool(&registration.definition).await?;
            self.registry.register(registration)?;
        }
        Ok(registrations.len())
    }

    pub async fn register_tool(
        &self,
        definition: ToolDefinition,
        tool: Arc<dyn Tool>,
    ) -> ToolRuntimeResult<()> {
        definition.validate()?;
        self.validator.validate_schema(&definition.input_schema)?;
        let provider = self
            .catalog
            .find_provider(&definition.provider_key)
            .await?
            .ok_or_else(|| ToolError::ProviderNotFound(definition.provider_key.clone()))?;
        if !provider.enabled {
            return Err(ToolError::ProviderDisabled(provider.key));
        }
        let registration = ToolRegistration::new(definition.clone(), tool);
        self.validate_registration(&provider, &registration)?;
        self.catalog.upsert_tool(&definition).await?;
        self.registry.register(registration)
    }

    pub async fn unregister_tool(&self, key: &str) -> ToolRuntimeResult<bool> {
        let removed_runtime = self.registry.remove(key)?.is_some();
        let removed_catalog = self.catalog.remove_tool(key).await?;
        Ok(removed_runtime || removed_catalog)
    }

    pub async fn list(&self) -> ToolRuntimeResult<Vec<ToolDefinition>> {
        self.catalog.list_tools().await
    }

    pub async fn categories(&self) -> ToolRuntimeResult<Vec<String>> {
        self.catalog.categories().await
    }

    pub async fn find(&self, key: &str) -> ToolRuntimeResult<Option<ToolDefinition>> {
        self.catalog.find_tool(key).await
    }

    pub async fn find_by_capability(
        &self,
        capability: &ToolCapability,
        include_descendants: bool,
    ) -> ToolRuntimeResult<Vec<ToolDefinition>> {
        self.catalog
            .find_by_capability(capability, include_descendants)
            .await
    }

    pub fn cancel(&self, request_id: Uuid) -> ToolRuntimeResult<bool> {
        let token = self
            .in_flight
            .read()
            .map_err(|_| ToolError::Internal("in-flight registry lock poisoned".into()))?
            .get(&request_id)
            .cloned();
        if let Some(token) = token {
            token.cancel();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn execute(&self, mut request: ToolRequest) -> ToolRuntimeResult<ToolResult> {
        request.validate()?;
        let original_id = request.id;
        let original_tool = request.tool.clone();
        for interceptor in &self.interceptors {
            interceptor
                .intercept_request(&mut request)
                .await
                .map_err(|error| ToolError::Interceptor(error.to_string()))?;
        }
        if request.id != original_id || request.tool != original_tool {
            return Err(ToolError::Interceptor(
                "request interceptor must not change request id or tool key".into(),
            ));
        }
        request.validate()?;

        let definition = self
            .catalog
            .find_tool(&request.tool)
            .await?
            .ok_or_else(|| ToolError::ToolNotFound(request.tool.clone()))?;
        definition.validate()?;
        self.validator.validate_schema(&definition.input_schema)?;
        if !definition.enabled {
            return Err(ToolError::ToolDisabled(definition.key));
        }
        let provider = self
            .catalog
            .find_provider(&definition.provider_key)
            .await?
            .ok_or_else(|| ToolError::ProviderNotFound(definition.provider_key.clone()))?;
        if !provider.enabled {
            return Err(ToolError::ProviderDisabled(provider.key));
        }
        let tool = self
            .registry
            .find(&definition.key)?
            .ok_or_else(|| ToolError::ToolNotFound(definition.key.clone()))?;
        if tool.key() != definition.key {
            return Err(ToolError::Registry(
                "live tool key does not match Catalog definition".into(),
            ));
        }

        let cancellation = CancellationToken::new();
        {
            let mut in_flight = self
                .in_flight
                .write()
                .map_err(|_| ToolError::Internal("in-flight registry lock poisoned".into()))?;
            match in_flight.entry(request.id) {
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(cancellation.clone());
                }
                std::collections::hash_map::Entry::Occupied(_) => {
                    return Err(ToolError::InvalidArgument(format!(
                        "request {} is already running",
                        request.id
                    )));
                }
            }
        }
        let _guard = InFlightGuard {
            request_id: request.id,
            in_flight: Arc::clone(&self.in_flight),
        };

        let mut record = ToolExecutionRecord::new(
            request.id,
            &definition.key,
            &definition.provider_key,
            request.session_id,
            request.subject.clone(),
            &request.metadata,
        );
        if let Err(error) = self.lifecycle.transition(&record).await {
            self.observe(&record, ToolStage::AuditFailed, Some(error.kind()));
            return Err(error);
        }
        self.observe(&record, ToolStage::Created, None);

        if let Err(error) = self
            .validator
            .validate(&definition.input_schema, &request.parameters)
        {
            return self.preflight_failure(record, error).await;
        }
        self.observe(&record, ToolStage::Validated, None);

        if let Err(error) = self.policy.evaluate(&request, &definition).await {
            return self.preflight_failure(record, error).await;
        }
        let decision = match self.permission.check(&request, &definition).await {
            Ok(decision) => decision,
            Err(error) => return self.preflight_failure(record, error).await,
        };
        self.observe(&record, ToolStage::PermissionChecked, None);
        match decision {
            PermissionDecision::Allow => {}
            PermissionDecision::Ask => {
                let error = ToolError::ApprovalRequired(definition.key.clone());
                return self.preflight_failure(record, error).await;
            }
            PermissionDecision::Deny => {
                let error = ToolError::PermissionDenied(definition.key.clone());
                return self.preflight_failure(record, error).await;
            }
        }

        self.required_transition(&mut record, ToolLifecycleStatus::Ready, ToolStage::Ready)
            .await?;
        self.required_transition(
            &mut record,
            ToolLifecycleStatus::Running,
            ToolStage::Running,
        )
        .await?;

        let started_at = record.started_at.unwrap_or_else(Utc::now);
        let context = ToolContext {
            request_id: request.id,
            cancellation: cancellation.clone(),
        };
        let timeout_ms = request
            .timeout_ms
            .unwrap_or(definition.timeout_ms)
            .min(definition.timeout_ms);
        let output = tokio::select! {
            _ = cancellation.cancelled() => Err(ToolError::Cancelled(request.id.to_string())),
            result = tokio::time::timeout(
                Duration::from_millis(timeout_ms),
                self.executor.invoke(tool, &request, &context),
            ) => match result {
                Ok(result) => result,
                Err(_) => Err(ToolError::Timeout { tool: definition.key.clone(), timeout_ms }),
            },
        };
        let completed_at = Utc::now();
        let was_cancelled = matches!(&output, Err(ToolError::Cancelled(_)));
        self.observe(
            &record,
            ToolStage::Mapping,
            output.as_ref().err().map(ToolError::kind),
        );
        let mut result = if was_cancelled {
            ToolResult::cancelled(request.id, &definition.key, started_at, completed_at)
        } else {
            match self
                .mapper
                .map(&request, &definition, started_at, completed_at, output)
            {
                Ok(result) => result,
                Err(error) => ToolResult::failed(
                    request.id,
                    &definition.key,
                    &ToolError::Mapping(error.to_string()),
                    started_at,
                    completed_at,
                ),
            }
        };

        if !result.status.is_terminal() {
            let error = ToolError::Mapping("result mapper returned a non-terminal status".into());
            result = ToolResult::failed(
                request.id,
                &definition.key,
                &error,
                started_at,
                completed_at,
            );
        }
        for interceptor in &self.interceptors {
            if let Err(error) = interceptor.intercept_result(&mut result).await {
                result = ToolResult::failed(
                    request.id,
                    &definition.key,
                    &ToolError::Interceptor(error.to_string()),
                    started_at,
                    completed_at,
                );
                break;
            }
        }
        result.request_id = request.id;
        result.tool_key = definition.key.clone();
        if let Err(error) = result.validate() {
            result = ToolResult::failed(
                request.id,
                &definition.key,
                &ToolError::Mapping(error.to_string()),
                started_at,
                completed_at,
            );
        }

        record.latency_ms = result.usage.duration_ms;
        record.error_kind = result.error.as_ref().map(|error| error.kind.clone());
        let final_status = result.status;
        let mut final_record = record.clone();
        final_record.transition(final_status)?;
        if let Err(error) = self.lifecycle.transition(&final_record).await {
            result
                .metadata
                .insert("core_agent.execution_audit".into(), "FAILED".into());
            self.observe(&final_record, ToolStage::AuditFailed, Some(error.kind()));
        }
        self.observe(
            &final_record,
            stage_for_status(final_status),
            result.error.as_ref().map(|error| error.kind.as_str()),
        );
        Ok(result)
    }

    fn validate_registration(
        &self,
        provider: &ToolProviderDefinition,
        registration: &ToolRegistration,
    ) -> ToolRuntimeResult<()> {
        registration.definition.validate()?;
        self.validator
            .validate_schema(&registration.definition.input_schema)?;
        if registration.definition.provider_key != provider.key {
            return Err(ToolError::Registry(format!(
                "tool {} belongs to provider {}, expected {}",
                registration.definition.key, registration.definition.provider_key, provider.key
            )));
        }
        if registration.tool.key() != registration.definition.key {
            return Err(ToolError::Registry(format!(
                "runtime key {} does not match definition {}",
                registration.tool.key(),
                registration.definition.key
            )));
        }
        Ok(())
    }

    async fn preflight_failure(
        &self,
        mut record: ToolExecutionRecord,
        error: ToolError,
    ) -> ToolRuntimeResult<ToolResult> {
        record.error_kind = Some(error.kind().into());
        if record.transition(ToolLifecycleStatus::Failed).is_ok() {
            if let Err(audit_error) = self.lifecycle.transition(&record).await {
                self.observe(&record, ToolStage::AuditFailed, Some(audit_error.kind()));
            }
            self.observe(&record, ToolStage::Failed, Some(error.kind()));
        }
        Err(error)
    }

    async fn required_transition(
        &self,
        record: &mut ToolExecutionRecord,
        status: ToolLifecycleStatus,
        stage: ToolStage,
    ) -> ToolRuntimeResult<()> {
        let mut next = record.clone();
        next.transition(status)?;
        self.lifecycle.transition(&next).await?;
        *record = next;
        self.observe(record, stage, None);
        Ok(())
    }

    fn observe(&self, record: &ToolExecutionRecord, stage: ToolStage, error_kind: Option<&str>) {
        let observation = ToolObservation {
            request_id: record.request_id,
            tool_key: record.tool_key.clone(),
            provider_key: record.provider_key.clone(),
            stage,
            duration_ms: record.latency_ms,
            error_kind: error_kind.map(str::to_owned),
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }
}

fn stage_for_status(status: ToolLifecycleStatus) -> ToolStage {
    match status {
        ToolLifecycleStatus::Success => ToolStage::Success,
        ToolLifecycleStatus::Failed => ToolStage::Failed,
        ToolLifecycleStatus::Cancelled => ToolStage::Cancelled,
        ToolLifecycleStatus::Created => ToolStage::Created,
        ToolLifecycleStatus::Ready => ToolStage::Ready,
        ToolLifecycleStatus::Running => ToolStage::Running,
    }
}

struct InFlightGuard {
    request_id: Uuid,
    in_flight: Arc<RwLock<HashMap<Uuid, CancellationToken>>>,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut in_flight) = self.in_flight.write() {
            in_flight.remove(&self.request_id);
        }
    }
}
