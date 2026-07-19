use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{
    Capability, CapabilityInvocation, CapabilityResult, Extension, ExtensionLoadHandle,
    ExtensionManifestRecord, ExtensionPermission, ExtensionState, ExtensionStateRecord, Provider,
};
use crate::error::ExtensionResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionOperation {
    Install,
    Load,
    Enable,
    Execute,
    Disable,
    Upgrade,
    Uninstall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionStage {
    Validation,
    Loader,
    Host,
    Registry,
    Persistence,
    Invocation,
}

#[derive(Debug, Clone)]
pub struct ExtensionObservation {
    pub operation: ExtensionOperation,
    pub stage: ExtensionStage,
    pub success: bool,
    pub extension_id: Option<Uuid>,
    pub capability_key: Option<String>,
    pub provider_id: Option<Uuid>,
    pub actor: String,
    pub message: Option<String>,
}

pub trait ExtensionObserver: Send + Sync {
    fn on_observation(&self, observation: &ExtensionObservation);
}

pub trait ExtensionInterceptor: Send + Sync {
    fn before_invocation(&self, _invocation: &mut CapabilityInvocation) -> ExtensionResult<()> {
        Ok(())
    }
}

pub trait ExtensionPolicy: Send + Sync {
    fn check(
        &self,
        operation: ExtensionOperation,
        extension: Option<&Extension>,
        permissions: &std::collections::BTreeSet<ExtensionPermission>,
        actor: &str,
    ) -> ExtensionResult<()>;
}

pub trait ExtensionLifecycle: Send + Sync {
    fn transition(&self, from: ExtensionState, to: ExtensionState) -> ExtensionResult<()>;
}

#[async_trait]
pub trait ExtensionLoader: Send + Sync {
    async fn load(
        &self,
        manifest: &ExtensionManifestRecord,
    ) -> ExtensionResult<ExtensionLoadHandle>;
    async fn unload(&self, handle: &ExtensionLoadHandle) -> ExtensionResult<()>;
}

#[async_trait]
pub trait ExtensionHost: Send + Sync {
    async fn start(&self, handle: &ExtensionLoadHandle) -> ExtensionResult<()>;
    async fn stop(&self, handle: &ExtensionLoadHandle) -> ExtensionResult<()>;
    async fn execute(
        &self,
        handle: &ExtensionLoadHandle,
        provider: &Provider,
        invocation: &CapabilityInvocation,
    ) -> ExtensionResult<CapabilityResult>;
}

#[derive(Debug, Clone)]
pub struct ExtensionRegistrationCommit {
    pub extension: Extension,
    pub expected_extension_version: Option<u64>,
    pub manifest: ExtensionManifestRecord,
    pub capabilities: Vec<Capability>,
    pub providers: Vec<Provider>,
    pub state_record: ExtensionStateRecord,
}

impl ExtensionRegistrationCommit {
    pub fn validate(&self) -> ExtensionResult<()> {
        self.extension.validate()?;
        self.manifest.validate()?;
        self.state_record.validate()?;
        for capability in &self.capabilities {
            capability.validate()?;
        }
        for provider in &self.providers {
            provider.validate()?;
        }
        if self.extension.id != self.manifest.extension_id
            || self.extension.current_manifest_id != self.manifest.id
            || self.extension.current_version != self.manifest.manifest.version
            || self.extension.key != self.manifest.manifest.key
            || self.state_record.extension_id != self.extension.id
            || self.state_record.sequence != self.extension.version
            || self.state_record.to_state != self.extension.state
            || self.capabilities.iter().any(|value| {
                value.extension_id != self.extension.id || value.manifest_id != self.manifest.id
            })
            || self.providers.iter().any(|value| {
                value.extension_id != self.extension.id || value.manifest_id != self.manifest.id
            })
        {
            return Err(crate::error::ExtensionError::Validation(
                "Extension registration aggregate is inconsistent".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ExtensionStateCommit {
    pub extension: Extension,
    pub expected_version: u64,
    pub capabilities: Vec<Capability>,
    pub providers: Vec<Provider>,
    pub state_record: ExtensionStateRecord,
}

impl ExtensionStateCommit {
    pub fn validate(&self) -> ExtensionResult<()> {
        self.extension.validate()?;
        self.state_record.validate()?;
        if self.extension.version != self.expected_version.saturating_add(1)
            || self.state_record.extension_id != self.extension.id
            || self.state_record.sequence != self.extension.version
            || self.state_record.to_state != self.extension.state
        {
            return Err(crate::error::ExtensionError::Validation(
                "Extension state aggregate is inconsistent".into(),
            ));
        }
        for capability in &self.capabilities {
            capability.validate()?;
        }
        for provider in &self.providers {
            provider.validate()?;
        }
        Ok(())
    }
}

#[async_trait]
pub trait ExtensionStore: Send + Sync {
    async fn save_registration(
        &self,
        commit: &ExtensionRegistrationCommit,
        actor: &str,
    ) -> ExtensionResult<()>;
    async fn commit_state(&self, commit: &ExtensionStateCommit, actor: &str)
        -> ExtensionResult<()>;

    async fn find_extension(&self, id: Uuid) -> ExtensionResult<Option<Extension>>;
    async fn find_extension_by_key(&self, key: &str) -> ExtensionResult<Option<Extension>>;
    async fn list_extensions(&self) -> ExtensionResult<Vec<Extension>>;
    async fn find_manifest(&self, id: Uuid) -> ExtensionResult<Option<ExtensionManifestRecord>>;
    async fn list_manifests(
        &self,
        extension_id: Uuid,
    ) -> ExtensionResult<Vec<ExtensionManifestRecord>>;
    async fn find_capability(&self, key: &str) -> ExtensionResult<Option<Capability>>;
    async fn list_capabilities(&self, extension_id: Uuid) -> ExtensionResult<Vec<Capability>>;
    async fn find_provider(&self, id: Uuid) -> ExtensionResult<Option<Provider>>;
    async fn list_providers(&self, extension_id: Uuid) -> ExtensionResult<Vec<Provider>>;
    async fn list_states(&self, extension_id: Uuid) -> ExtensionResult<Vec<ExtensionStateRecord>>;
}

pub trait ExtensionRegistry: ExtensionStore {}
impl<T: ExtensionStore + ?Sized> ExtensionRegistry for T {}

pub trait CapabilityRegistry: ExtensionStore {}
impl<T: ExtensionStore + ?Sized> CapabilityRegistry for T {}

pub trait ProviderManager: ExtensionStore {}
impl<T: ExtensionStore + ?Sized> ProviderManager for T {}
