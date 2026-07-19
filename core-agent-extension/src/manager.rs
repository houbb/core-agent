use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use uuid::Uuid;

use crate::defaults::{
    DefaultExtensionLifecycle, EmbeddedExtensionPolicy, InMemoryExtensionStore,
    LocalManifestLoader, UnavailableExtensionHost,
};
use crate::domain::{
    entities_from_manifest, invocation_hash, validate_actor, Capability, CapabilityInvocation,
    CapabilityResult, Extension, ExtensionLoadHandle, ExtensionManifestRecord, ExtensionState,
    ExtensionStateRecord, InstallExtensionRequest, Provider,
};
use crate::error::{ExtensionError, ExtensionResult};
use crate::infrastructure::{
    ExtensionHost, ExtensionInterceptor, ExtensionLifecycle, ExtensionLoader, ExtensionObservation,
    ExtensionObserver, ExtensionOperation, ExtensionPolicy, ExtensionRegistrationCommit,
    ExtensionStage, ExtensionStateCommit, ExtensionStore,
};

pub struct ExtensionManagerBuilder {
    store: Arc<dyn ExtensionStore>,
    loader: Arc<dyn ExtensionLoader>,
    host: Arc<dyn ExtensionHost>,
    policy: Arc<dyn ExtensionPolicy>,
    lifecycle: Arc<dyn ExtensionLifecycle>,
    interceptors: Vec<Arc<dyn ExtensionInterceptor>>,
    observers: Vec<Arc<dyn ExtensionObserver>>,
}

impl Default for ExtensionManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryExtensionStore::default()),
            loader: Arc::new(LocalManifestLoader),
            host: Arc::new(UnavailableExtensionHost),
            policy: Arc::new(EmbeddedExtensionPolicy),
            lifecycle: Arc::new(DefaultExtensionLifecycle),
            interceptors: Vec::new(),
            observers: Vec::new(),
        }
    }
}

impl ExtensionManagerBuilder {
    pub fn store(mut self, value: Arc<dyn ExtensionStore>) -> Self {
        self.store = value;
        self
    }
    pub fn loader(mut self, value: Arc<dyn ExtensionLoader>) -> Self {
        self.loader = value;
        self
    }
    pub fn host(mut self, value: Arc<dyn ExtensionHost>) -> Self {
        self.host = value;
        self
    }
    pub fn policy(mut self, value: Arc<dyn ExtensionPolicy>) -> Self {
        self.policy = value;
        self
    }
    pub fn lifecycle(mut self, value: Arc<dyn ExtensionLifecycle>) -> Self {
        self.lifecycle = value;
        self
    }
    pub fn interceptor(mut self, value: Arc<dyn ExtensionInterceptor>) -> Self {
        self.interceptors.push(value);
        self
    }
    pub fn observer(mut self, value: Arc<dyn ExtensionObserver>) -> Self {
        self.observers.push(value);
        self
    }
    pub fn build(self) -> ExtensionManager {
        ExtensionManager {
            store: self.store,
            loader: self.loader,
            host: self.host,
            policy: self.policy,
            lifecycle: self.lifecycle,
            interceptors: self.interceptors,
            observers: self.observers,
            handles: Mutex::new(HashMap::new()),
            live: Mutex::new(HashMap::new()),
        }
    }
}

pub struct ExtensionManager {
    store: Arc<dyn ExtensionStore>,
    loader: Arc<dyn ExtensionLoader>,
    host: Arc<dyn ExtensionHost>,
    policy: Arc<dyn ExtensionPolicy>,
    lifecycle: Arc<dyn ExtensionLifecycle>,
    interceptors: Vec<Arc<dyn ExtensionInterceptor>>,
    observers: Vec<Arc<dyn ExtensionObserver>>,
    handles: Mutex<HashMap<Uuid, ExtensionLoadHandle>>,
    live: Mutex<HashMap<Uuid, Uuid>>,
}

struct LiveGuard<'a> {
    live: &'a Mutex<HashMap<Uuid, Uuid>>,
    extension_id: Uuid,
}

impl Drop for LiveGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut live) = self.live.lock() {
            live.remove(&self.extension_id);
        }
    }
}

impl ExtensionManager {
    pub fn builder() -> ExtensionManagerBuilder {
        ExtensionManagerBuilder::default()
    }

    pub async fn install(&self, request: InstallExtensionRequest) -> ExtensionResult<Extension> {
        request.validate()?;
        self.policy.check(
            ExtensionOperation::Install,
            None,
            &request.manifest.requested_permissions,
            &request.actor,
        )?;
        if self
            .store
            .find_extension_by_key(&request.manifest.key)
            .await?
            .is_some()
        {
            return Err(ExtensionError::Conflict(format!(
                "Extension key {} is already installed",
                request.manifest.key
            )));
        }
        let now = Utc::now();
        let extension_id = Uuid::new_v4();
        let manifest = ExtensionManifestRecord {
            id: Uuid::new_v4(),
            extension_id,
            revision: 1,
            manifest: request.manifest,
            source_uri: request.source_uri,
            checksum: request.checksum.to_ascii_lowercase(),
            actor: request.actor.clone(),
            created_at: now,
        };
        let extension = Extension {
            id: extension_id,
            key: manifest.manifest.key.clone(),
            name: manifest.manifest.name.clone(),
            current_manifest_id: manifest.id,
            current_version: manifest.manifest.version.clone(),
            state: ExtensionState::Installed,
            current_request_id: None,
            current_capability_key: None,
            current_provider_id: None,
            current_invocation_hash: None,
            version: 1,
            actor: request.actor.clone(),
            created_at: now,
            updated_at: now,
        };
        let (capabilities, providers) =
            entities_from_manifest(&extension, &manifest, &request.actor);
        let state_record = state_record(&extension, None, "Extension installed", &request.actor);
        self.store
            .save_registration(
                &ExtensionRegistrationCommit {
                    extension: extension.clone(),
                    expected_extension_version: None,
                    manifest,
                    capabilities,
                    providers,
                    state_record,
                },
                &request.actor,
            )
            .await?;
        Ok(extension)
    }

    pub async fn load(&self, id: Uuid, actor: &str) -> ExtensionResult<Extension> {
        validate_actor(actor)?;
        let _guard = self.enter_live(id, Uuid::nil())?;
        let mut extension = self.required_extension(id).await?;
        let manifest = self
            .required_manifest(extension.current_manifest_id)
            .await?;
        self.policy.check(
            ExtensionOperation::Load,
            Some(&extension),
            &manifest.manifest.requested_permissions,
            actor,
        )?;
        if extension.state != ExtensionState::Installed {
            return Err(ExtensionError::InvalidState(format!(
                "cannot load {} Extension",
                extension.state.as_str()
            )));
        }
        let handle = self.loader.load(&manifest).await?;
        validate_handle(&extension, &handle)?;
        let expected = extension.version;
        self.transition(&mut extension, ExtensionState::Loaded, actor)?;
        if let Err(error) = self
            .store
            .commit_state(
                &state_commit(
                    extension.clone(),
                    expected,
                    Vec::new(),
                    Vec::new(),
                    ExtensionState::Installed,
                    "Extension loaded",
                    actor,
                ),
                actor,
            )
            .await
        {
            let _ = self.loader.unload(&handle).await;
            return Err(error);
        }
        self.insert_handle(extension.id, handle)?;
        Ok(extension)
    }

    pub async fn enable(&self, id: Uuid, actor: &str) -> ExtensionResult<Extension> {
        validate_actor(actor)?;
        let _guard = self.enter_live(id, Uuid::nil())?;
        let mut extension = self.required_extension(id).await?;
        if extension.state != ExtensionState::Loaded {
            return Err(ExtensionError::InvalidState(
                "Extension must be Loaded before enable".into(),
            ));
        }
        let manifest = self
            .required_manifest(extension.current_manifest_id)
            .await?;
        self.policy.check(
            ExtensionOperation::Enable,
            Some(&extension),
            &manifest.manifest.requested_permissions,
            actor,
        )?;
        let handle = self.required_handle(id)?;
        self.host.start(&handle).await?;
        let mut capabilities = self.current_capabilities(&extension).await?;
        let mut providers = self.current_providers(&extension).await?;
        set_catalog_enabled(&mut capabilities, &mut providers, true, actor);
        let expected = extension.version;
        self.transition(&mut extension, ExtensionState::Enabled, actor)?;
        if let Err(error) = self
            .store
            .commit_state(
                &state_commit(
                    extension.clone(),
                    expected,
                    capabilities,
                    providers,
                    ExtensionState::Loaded,
                    "Extension enabled",
                    actor,
                ),
                actor,
            )
            .await
        {
            let _ = self.host.stop(&handle).await;
            return Err(error);
        }
        Ok(extension)
    }

    pub async fn execute(
        &self,
        mut invocation: CapabilityInvocation,
    ) -> ExtensionResult<CapabilityResult> {
        invocation.validate()?;
        let original_request = invocation.request_id;
        let original_capability = invocation.capability_key.clone();
        let original_actor = invocation.actor.clone();
        for interceptor in &self.interceptors {
            catch_unwind(AssertUnwindSafe(|| {
                interceptor.before_invocation(&mut invocation)
            }))
            .map_err(|_| ExtensionError::Extension("Extension interceptor panicked".into()))??;
        }
        invocation.validate()?;
        if invocation.request_id != original_request
            || invocation.capability_key != original_capability
            || invocation.actor != original_actor
        {
            return Err(ExtensionError::Validation(
                "Extension interceptor changed invocation identity or actor".into(),
            ));
        }
        let capability = self
            .store
            .find_capability(&invocation.capability_key)
            .await?
            .ok_or_else(|| ExtensionError::NotFound(invocation.capability_key.clone()))?;
        let _guard = self.enter_live(capability.extension_id, invocation.request_id)?;
        let extension = self.required_extension(capability.extension_id).await?;
        if extension.state != ExtensionState::Enabled {
            return Err(ExtensionError::InvalidState(
                "Capability owner Extension is not Enabled".into(),
            ));
        }
        self.policy.check(
            ExtensionOperation::Execute,
            Some(&extension),
            &capability.permissions,
            &invocation.actor,
        )?;
        let provider = self.resolve_provider(&extension, &capability).await?;
        self.drive_invocation(extension, provider, invocation).await
    }

    pub async fn resume(
        &self,
        invocation: CapabilityInvocation,
    ) -> ExtensionResult<CapabilityResult> {
        invocation.validate()?;
        let extension = self
            .store
            .list_extensions()
            .await?
            .into_iter()
            .find(|value| value.current_request_id == Some(invocation.request_id))
            .ok_or_else(|| ExtensionError::not_found(invocation.request_id))?;
        if extension.state != ExtensionState::Running
            || extension.current_capability_key.as_deref()
                != Some(invocation.capability_key.as_str())
            || extension.current_invocation_hash.as_deref()
                != Some(invocation_hash(&invocation)?.as_str())
        {
            return Err(ExtensionError::InvalidState(
                "invocation does not match durable Running Extension".into(),
            ));
        }
        let provider_id = extension.current_provider_id.ok_or_else(|| {
            ExtensionError::Validation("Running Extension has no Provider".into())
        })?;
        let provider = self
            .store
            .find_provider(provider_id)
            .await?
            .ok_or_else(|| ExtensionError::not_found(provider_id))?;
        let _guard = self.enter_live(extension.id, invocation.request_id)?;
        self.invoke_host(extension, provider, invocation).await
    }

    async fn drive_invocation(
        &self,
        mut extension: Extension,
        provider: Provider,
        invocation: CapabilityInvocation,
    ) -> ExtensionResult<CapabilityResult> {
        let expected = extension.version;
        extension.current_request_id = Some(invocation.request_id);
        extension.current_capability_key = Some(invocation.capability_key.clone());
        extension.current_provider_id = Some(provider.id);
        extension.current_invocation_hash = Some(invocation_hash(&invocation)?);
        self.transition(&mut extension, ExtensionState::Running, &invocation.actor)?;
        self.store
            .commit_state(
                &state_commit(
                    extension.clone(),
                    expected,
                    Vec::new(),
                    Vec::new(),
                    ExtensionState::Enabled,
                    "Capability invocation started",
                    &invocation.actor,
                ),
                &invocation.actor,
            )
            .await?;
        self.invoke_host(extension, provider, invocation).await
    }

    async fn invoke_host(
        &self,
        mut extension: Extension,
        provider: Provider,
        invocation: CapabilityInvocation,
    ) -> ExtensionResult<CapabilityResult> {
        let handle = self.ensure_running_handle(&extension).await?;
        let result = match self.host.execute(&handle, &provider, &invocation).await {
            Ok(value) => value,
            Err(error) => {
                self.notify(
                    ExtensionOperation::Execute,
                    ExtensionStage::Invocation,
                    false,
                    Some(&extension),
                    Some(&invocation.capability_key),
                    Some(provider.id),
                    &invocation.actor,
                    Some(error.to_string()),
                );
                return Err(ExtensionError::OutcomeUnknown(error.to_string()));
            }
        };
        result
            .validate()
            .map_err(|error| ExtensionError::OutcomeUnknown(error.to_string()))?;
        if result.request_id != invocation.request_id || result.provider_id != provider.id {
            return Err(ExtensionError::OutcomeUnknown(
                "ExtensionHost returned a result for another invocation".into(),
            ));
        }
        let expected = extension.version;
        extension.current_request_id = None;
        extension.current_capability_key = None;
        extension.current_provider_id = None;
        extension.current_invocation_hash = None;
        self.transition(&mut extension, ExtensionState::Enabled, &invocation.actor)?;
        self.store
            .commit_state(
                &state_commit(
                    extension.clone(),
                    expected,
                    Vec::new(),
                    Vec::new(),
                    ExtensionState::Running,
                    "Capability invocation completed",
                    &invocation.actor,
                ),
                &invocation.actor,
            )
            .await
            .map_err(|error| ExtensionError::OutcomeUnknown(error.to_string()))?;
        self.notify(
            ExtensionOperation::Execute,
            ExtensionStage::Invocation,
            true,
            Some(&extension),
            Some(&invocation.capability_key),
            Some(provider.id),
            &invocation.actor,
            None,
        );
        Ok(result)
    }

    pub async fn disable(&self, id: Uuid, actor: &str) -> ExtensionResult<Extension> {
        validate_actor(actor)?;
        let _guard = self.enter_live(id, Uuid::nil())?;
        let mut extension = self.required_extension(id).await?;
        if !matches!(
            extension.state,
            ExtensionState::Installed | ExtensionState::Loaded | ExtensionState::Enabled
        ) {
            return Err(ExtensionError::InvalidState(
                "Extension cannot be disabled from its current state".into(),
            ));
        }
        let previous = extension.state;
        self.policy.check(
            ExtensionOperation::Disable,
            Some(&extension),
            &Default::default(),
            actor,
        )?;
        let handle = self.handle(id)?;
        if previous == ExtensionState::Enabled {
            if let Some(handle) = &handle {
                self.host.stop(handle).await?;
            }
        }
        let mut capabilities = self.current_capabilities(&extension).await?;
        let mut providers = self.current_providers(&extension).await?;
        set_catalog_enabled(&mut capabilities, &mut providers, false, actor);
        let expected = extension.version;
        self.transition(&mut extension, ExtensionState::Disabled, actor)?;
        if let Err(error) = self
            .store
            .commit_state(
                &state_commit(
                    extension.clone(),
                    expected,
                    capabilities,
                    providers,
                    previous,
                    "Extension disabled",
                    actor,
                ),
                actor,
            )
            .await
        {
            if previous == ExtensionState::Enabled {
                if let Some(handle) = &handle {
                    let _ = self.host.start(handle).await;
                }
            }
            return Err(error);
        }
        if let Some(handle) = handle {
            self.loader.unload(&handle).await?;
            self.remove_handle(id)?;
        }
        Ok(extension)
    }

    pub async fn upgrade(
        &self,
        id: Uuid,
        request: InstallExtensionRequest,
    ) -> ExtensionResult<Extension> {
        request.validate()?;
        let _guard = self.enter_live(id, Uuid::nil())?;
        let mut extension = self.required_extension(id).await?;
        if extension.state != ExtensionState::Disabled
            || extension.key != request.manifest.key
            || extension.current_version == request.manifest.version
        {
            return Err(ExtensionError::InvalidState(
                "offline upgrade requires a Disabled Extension and a new matching version".into(),
            ));
        }
        self.policy.check(
            ExtensionOperation::Upgrade,
            Some(&extension),
            &request.manifest.requested_permissions,
            &request.actor,
        )?;
        let expected = extension.version;
        let previous = extension.state;
        extension.current_manifest_id = Uuid::new_v4();
        extension.current_version = request.manifest.version.clone();
        extension.name = request.manifest.name.clone();
        self.transition(&mut extension, ExtensionState::Installed, &request.actor)?;
        let revision = self.store.list_manifests(id).await?.len() as u64 + 1;
        let manifest = ExtensionManifestRecord {
            id: extension.current_manifest_id,
            extension_id: id,
            revision,
            manifest: request.manifest,
            source_uri: request.source_uri,
            checksum: request.checksum.to_ascii_lowercase(),
            actor: request.actor.clone(),
            created_at: Utc::now(),
        };
        let (capabilities, providers) =
            entities_from_manifest(&extension, &manifest, &request.actor);
        self.store
            .save_registration(
                &ExtensionRegistrationCommit {
                    extension: extension.clone(),
                    expected_extension_version: Some(expected),
                    manifest,
                    capabilities,
                    providers,
                    state_record: state_record(
                        &extension,
                        Some(previous),
                        "Extension upgraded offline",
                        &request.actor,
                    ),
                },
                &request.actor,
            )
            .await?;
        Ok(extension)
    }

    pub async fn uninstall(&self, id: Uuid, actor: &str) -> ExtensionResult<Extension> {
        validate_actor(actor)?;
        let _guard = self.enter_live(id, Uuid::nil())?;
        let mut extension = self.required_extension(id).await?;
        if extension.state != ExtensionState::Disabled {
            return Err(ExtensionError::InvalidState(
                "Extension must be Disabled before uninstall".into(),
            ));
        }
        self.policy.check(
            ExtensionOperation::Uninstall,
            Some(&extension),
            &Default::default(),
            actor,
        )?;
        let expected = extension.version;
        self.transition(&mut extension, ExtensionState::Uninstalled, actor)?;
        self.store
            .commit_state(
                &state_commit(
                    extension.clone(),
                    expected,
                    Vec::new(),
                    Vec::new(),
                    ExtensionState::Disabled,
                    "Extension uninstalled",
                    actor,
                ),
                actor,
            )
            .await?;
        Ok(extension)
    }

    pub async fn find(&self, id: Uuid) -> ExtensionResult<Option<Extension>> {
        self.store.find_extension(id).await
    }
    pub async fn list(&self) -> ExtensionResult<Vec<Extension>> {
        self.store.list_extensions().await
    }
    pub async fn list_manifests(&self, id: Uuid) -> ExtensionResult<Vec<ExtensionManifestRecord>> {
        self.store.list_manifests(id).await
    }
    pub async fn find_capability(&self, key: &str) -> ExtensionResult<Option<Capability>> {
        self.store.find_capability(key).await
    }
    pub async fn list_capabilities(&self, id: Uuid) -> ExtensionResult<Vec<Capability>> {
        self.store.list_capabilities(id).await
    }
    pub async fn list_providers(&self, id: Uuid) -> ExtensionResult<Vec<Provider>> {
        self.store.list_providers(id).await
    }
    pub async fn list_states(&self, id: Uuid) -> ExtensionResult<Vec<ExtensionStateRecord>> {
        self.store.list_states(id).await
    }

    async fn resolve_provider(
        &self,
        extension: &Extension,
        capability: &Capability,
    ) -> ExtensionResult<Provider> {
        let mut providers = self
            .current_providers(extension)
            .await?
            .into_iter()
            .filter(|value| value.enabled && value.capabilities.contains(&capability.key))
            .collect::<Vec<_>>();
        providers.sort_by_key(|value| (value.priority, value.key.clone(), value.id));
        providers.into_iter().next().ok_or_else(|| {
            ExtensionError::NotFound(format!(
                "no enabled Provider for Capability {}",
                capability.key
            ))
        })
    }

    async fn current_capabilities(
        &self,
        extension: &Extension,
    ) -> ExtensionResult<Vec<Capability>> {
        Ok(self
            .store
            .list_capabilities(extension.id)
            .await?
            .into_iter()
            .filter(|value| value.manifest_id == extension.current_manifest_id)
            .collect())
    }

    async fn current_providers(&self, extension: &Extension) -> ExtensionResult<Vec<Provider>> {
        Ok(self
            .store
            .list_providers(extension.id)
            .await?
            .into_iter()
            .filter(|value| value.manifest_id == extension.current_manifest_id)
            .collect())
    }

    async fn ensure_running_handle(
        &self,
        extension: &Extension,
    ) -> ExtensionResult<ExtensionLoadHandle> {
        if let Some(handle) = self.handle(extension.id)? {
            return Ok(handle);
        }
        let manifest = self
            .required_manifest(extension.current_manifest_id)
            .await?;
        let handle = self.loader.load(&manifest).await?;
        validate_handle(extension, &handle)?;
        self.host.start(&handle).await?;
        self.insert_handle(extension.id, handle.clone())?;
        Ok(handle)
    }

    fn transition(
        &self,
        extension: &mut Extension,
        state: ExtensionState,
        actor: &str,
    ) -> ExtensionResult<()> {
        self.lifecycle.transition(extension.state, state)?;
        extension.state = state;
        extension.version = extension.version.saturating_add(1);
        extension.actor = actor.into();
        extension.updated_at = Utc::now().max(extension.updated_at);
        extension.validate()
    }

    async fn required_extension(&self, id: Uuid) -> ExtensionResult<Extension> {
        self.store
            .find_extension(id)
            .await?
            .ok_or_else(|| ExtensionError::not_found(id))
    }

    async fn required_manifest(&self, id: Uuid) -> ExtensionResult<ExtensionManifestRecord> {
        self.store
            .find_manifest(id)
            .await?
            .ok_or_else(|| ExtensionError::not_found(id))
    }

    fn insert_handle(&self, id: Uuid, handle: ExtensionLoadHandle) -> ExtensionResult<()> {
        let mut handles = self
            .handles
            .lock()
            .map_err(|_| ExtensionError::Internal("Extension handle lock poisoned".into()))?;
        match handles.entry(id) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(handle);
                Ok(())
            }
            std::collections::hash_map::Entry::Occupied(_) => Err(ExtensionError::Conflict(
                "Extension already has a live Host handle".into(),
            )),
        }
    }

    fn required_handle(&self, id: Uuid) -> ExtensionResult<ExtensionLoadHandle> {
        self.handle(id)?
            .ok_or_else(|| ExtensionError::NotFound(format!("live handle for Extension {id}")))
    }

    fn handle(&self, id: Uuid) -> ExtensionResult<Option<ExtensionLoadHandle>> {
        Ok(self
            .handles
            .lock()
            .map_err(|_| ExtensionError::Internal("Extension handle lock poisoned".into()))?
            .get(&id)
            .cloned())
    }

    fn remove_handle(&self, id: Uuid) -> ExtensionResult<()> {
        self.handles
            .lock()
            .map_err(|_| ExtensionError::Internal("Extension handle lock poisoned".into()))?
            .remove(&id);
        Ok(())
    }

    fn enter_live(&self, id: Uuid, request_id: Uuid) -> ExtensionResult<LiveGuard<'_>> {
        let mut live = self
            .live
            .lock()
            .map_err(|_| ExtensionError::Internal("Extension live lock poisoned".into()))?;
        match live.entry(id) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(request_id);
                Ok(LiveGuard {
                    live: &self.live,
                    extension_id: id,
                })
            }
            std::collections::hash_map::Entry::Occupied(entry) => Err(ExtensionError::Conflict(
                format!("Extension {id} is executing request {}", entry.get()),
            )),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        operation: ExtensionOperation,
        stage: ExtensionStage,
        success: bool,
        extension: Option<&Extension>,
        capability_key: Option<&str>,
        provider_id: Option<Uuid>,
        actor: &str,
        message: Option<String>,
    ) {
        let observation = ExtensionObservation {
            operation,
            stage,
            success,
            extension_id: extension.map(|value| value.id),
            capability_key: capability_key.map(str::to_string),
            provider_id,
            actor: actor.into(),
            message,
        };
        for observer in &self.observers {
            let _ = catch_unwind(AssertUnwindSafe(|| observer.on_observation(&observation)));
        }
    }
}

fn validate_handle(extension: &Extension, handle: &ExtensionLoadHandle) -> ExtensionResult<()> {
    if handle.extension_id != extension.id || handle.manifest_id != extension.current_manifest_id {
        return Err(ExtensionError::Validation(
            "ExtensionLoader returned a handle for another Extension revision".into(),
        ));
    }
    Ok(())
}

fn state_record(
    extension: &Extension,
    from_state: Option<ExtensionState>,
    reason: &str,
    actor: &str,
) -> ExtensionStateRecord {
    ExtensionStateRecord {
        id: Uuid::new_v4(),
        extension_id: extension.id,
        sequence: extension.version,
        from_state,
        to_state: extension.state,
        reason: reason.into(),
        actor: actor.into(),
        created_at: Utc::now(),
    }
}

fn state_commit(
    extension: Extension,
    expected_version: u64,
    capabilities: Vec<Capability>,
    providers: Vec<Provider>,
    from_state: ExtensionState,
    reason: &str,
    actor: &str,
) -> ExtensionStateCommit {
    let state_record = state_record(&extension, Some(from_state), reason, actor);
    ExtensionStateCommit {
        extension,
        expected_version,
        capabilities,
        providers,
        state_record,
    }
}

fn set_catalog_enabled(
    capabilities: &mut [Capability],
    providers: &mut [Provider],
    enabled: bool,
    actor: &str,
) {
    let now = Utc::now();
    for capability in capabilities {
        capability.enabled = enabled;
        capability.version = capability.version.saturating_add(1);
        capability.actor = actor.into();
        capability.updated_at = now.max(capability.updated_at);
    }
    for provider in providers {
        provider.enabled = enabled;
        provider.version = provider.version.saturating_add(1);
        provider.actor = actor.into();
        provider.updated_at = now.max(provider.updated_at);
    }
}
