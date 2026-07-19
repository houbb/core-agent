use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::domain::{
    validate_actor, Capability, CapabilityInvocation, CapabilityResult, Extension,
    ExtensionLoadHandle, ExtensionManifestRecord, ExtensionPermission, ExtensionState,
    ExtensionStateRecord, Provider,
};
use crate::error::{ExtensionError, ExtensionResult};
use crate::infrastructure::{
    ExtensionHost, ExtensionLifecycle, ExtensionLoader, ExtensionOperation, ExtensionPolicy,
    ExtensionRegistrationCommit, ExtensionStateCommit, ExtensionStore,
};

#[derive(Default)]
pub struct EmbeddedExtensionPolicy;

impl ExtensionPolicy for EmbeddedExtensionPolicy {
    fn check(
        &self,
        operation: ExtensionOperation,
        _extension: Option<&Extension>,
        permissions: &std::collections::BTreeSet<ExtensionPermission>,
        actor: &str,
    ) -> ExtensionResult<()> {
        validate_actor(actor)?;
        if !permissions.is_empty()
            && matches!(
                operation,
                ExtensionOperation::Install
                    | ExtensionOperation::Load
                    | ExtensionOperation::Enable
                    | ExtensionOperation::Execute
                    | ExtensionOperation::Upgrade
            )
        {
            return Err(ExtensionError::Denied(
                "default Extension policy denies requested host permissions".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct DefaultExtensionLifecycle;

impl ExtensionLifecycle for DefaultExtensionLifecycle {
    fn transition(&self, from: ExtensionState, to: ExtensionState) -> ExtensionResult<()> {
        let allowed = matches!(
            (from, to),
            (ExtensionState::Installed, ExtensionState::Loaded)
                | (ExtensionState::Loaded, ExtensionState::Enabled)
                | (ExtensionState::Enabled, ExtensionState::Running)
                | (ExtensionState::Running, ExtensionState::Enabled)
                | (ExtensionState::Enabled, ExtensionState::Disabled)
                | (ExtensionState::Loaded, ExtensionState::Disabled)
                | (ExtensionState::Installed, ExtensionState::Disabled)
                | (ExtensionState::Disabled, ExtensionState::Installed)
                | (ExtensionState::Disabled, ExtensionState::Uninstalled)
        );
        if !allowed {
            return Err(ExtensionError::InvalidState(format!(
                "cannot transition Extension from {} to {}",
                from.as_str(),
                to.as_str()
            )));
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct LocalManifestLoader;

#[async_trait]
impl ExtensionLoader for LocalManifestLoader {
    async fn load(
        &self,
        manifest: &ExtensionManifestRecord,
    ) -> ExtensionResult<ExtensionLoadHandle> {
        manifest.validate()?;
        let path = manifest.source_uri.strip_prefix("file:").ok_or_else(|| {
            ExtensionError::Loader("local Extension source is not a file URI".into())
        })?;
        let bytes = std::fs::read(path)
            .map_err(|error| ExtensionError::Loader(format!("cannot read Extension: {error}")))?;
        let actual = format!("{:x}", Sha256::digest(bytes));
        if actual != manifest.checksum.to_ascii_lowercase() {
            return Err(ExtensionError::Loader(
                "Extension artifact checksum does not match Manifest record".into(),
            ));
        }
        Ok(ExtensionLoadHandle {
            extension_id: manifest.extension_id,
            manifest_id: manifest.id,
            generation: Uuid::new_v4(),
        })
    }

    async fn unload(&self, _handle: &ExtensionLoadHandle) -> ExtensionResult<()> {
        Ok(())
    }
}

#[derive(Default)]
pub struct UnavailableExtensionHost;

#[async_trait]
impl ExtensionHost for UnavailableExtensionHost {
    async fn start(&self, _handle: &ExtensionLoadHandle) -> ExtensionResult<()> {
        Err(ExtensionError::Host(
            "ExtensionHost is not configured".into(),
        ))
    }

    async fn stop(&self, _handle: &ExtensionLoadHandle) -> ExtensionResult<()> {
        Ok(())
    }

    async fn execute(
        &self,
        _handle: &ExtensionLoadHandle,
        _provider: &Provider,
        _invocation: &CapabilityInvocation,
    ) -> ExtensionResult<CapabilityResult> {
        Err(ExtensionError::Host(
            "ExtensionHost is not configured".into(),
        ))
    }
}

#[derive(Clone, Default)]
struct MemoryState {
    extensions: HashMap<Uuid, Extension>,
    manifests: HashMap<Uuid, ExtensionManifestRecord>,
    capabilities: HashMap<Uuid, Capability>,
    providers: HashMap<Uuid, Provider>,
    states: HashMap<Uuid, ExtensionStateRecord>,
}

#[derive(Default)]
pub struct InMemoryExtensionStore {
    state: RwLock<MemoryState>,
}

impl InMemoryExtensionStore {
    fn read(&self) -> ExtensionResult<std::sync::RwLockReadGuard<'_, MemoryState>> {
        self.state
            .read()
            .map_err(|_| ExtensionError::Internal("Extension store lock poisoned".into()))
    }

    fn write(&self) -> ExtensionResult<std::sync::RwLockWriteGuard<'_, MemoryState>> {
        self.state
            .write()
            .map_err(|_| ExtensionError::Internal("Extension store lock poisoned".into()))
    }
}

#[async_trait]
impl ExtensionStore for InMemoryExtensionStore {
    async fn save_registration(
        &self,
        commit: &ExtensionRegistrationCommit,
        actor: &str,
    ) -> ExtensionResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut state = self.write()?;
        let mut next = state.clone();
        validate_registration(&next, commit)?;
        if commit.expected_extension_version.is_some() {
            for capability in next
                .capabilities
                .values_mut()
                .filter(|value| value.extension_id == commit.extension.id)
            {
                capability.enabled = false;
            }
            for provider in next
                .providers
                .values_mut()
                .filter(|value| value.extension_id == commit.extension.id)
            {
                provider.enabled = false;
            }
        }
        next.extensions
            .insert(commit.extension.id, commit.extension.clone());
        next.manifests
            .insert(commit.manifest.id, commit.manifest.clone());
        for capability in &commit.capabilities {
            next.capabilities.insert(capability.id, capability.clone());
        }
        for provider in &commit.providers {
            next.providers.insert(provider.id, provider.clone());
        }
        next.states
            .insert(commit.state_record.id, commit.state_record.clone());
        *state = next;
        Ok(())
    }

    async fn commit_state(
        &self,
        commit: &ExtensionStateCommit,
        actor: &str,
    ) -> ExtensionResult<()> {
        validate_actor(actor)?;
        commit.validate()?;
        let mut state = self.write()?;
        let mut next = state.clone();
        let current = next
            .extensions
            .get(&commit.extension.id)
            .ok_or_else(|| ExtensionError::not_found(commit.extension.id))?;
        validate_extension_update(current, &commit.extension, commit.expected_version)?;
        if commit.state_record.from_state != Some(current.state)
            || next.states.values().any(|value| {
                value.extension_id == commit.extension.id
                    && value.sequence == commit.extension.version
            })
        {
            return Err(ExtensionError::Validation(
                "Extension state timeline is inconsistent".into(),
            ));
        }
        for capability in &commit.capabilities {
            validate_capability_update(&next, capability)?;
        }
        for provider in &commit.providers {
            validate_provider_update(&next, provider)?;
        }
        next.extensions
            .insert(commit.extension.id, commit.extension.clone());
        for capability in &commit.capabilities {
            next.capabilities.insert(capability.id, capability.clone());
        }
        for provider in &commit.providers {
            next.providers.insert(provider.id, provider.clone());
        }
        next.states
            .insert(commit.state_record.id, commit.state_record.clone());
        *state = next;
        Ok(())
    }

    async fn find_extension(&self, id: Uuid) -> ExtensionResult<Option<Extension>> {
        Ok(self.read()?.extensions.get(&id).cloned())
    }

    async fn find_extension_by_key(&self, key: &str) -> ExtensionResult<Option<Extension>> {
        Ok(self
            .read()?
            .extensions
            .values()
            .find(|value| value.key == key)
            .cloned())
    }

    async fn list_extensions(&self) -> ExtensionResult<Vec<Extension>> {
        let mut values = self
            .read()?
            .extensions
            .values()
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.key.clone(), value.id));
        Ok(values)
    }

    async fn find_manifest(&self, id: Uuid) -> ExtensionResult<Option<ExtensionManifestRecord>> {
        Ok(self.read()?.manifests.get(&id).cloned())
    }

    async fn list_manifests(
        &self,
        extension_id: Uuid,
    ) -> ExtensionResult<Vec<ExtensionManifestRecord>> {
        let mut values = self
            .read()?
            .manifests
            .values()
            .filter(|value| value.extension_id == extension_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| value.revision);
        Ok(values)
    }

    async fn find_capability(&self, key: &str) -> ExtensionResult<Option<Capability>> {
        let state = self.read()?;
        Ok(state
            .capabilities
            .values()
            .find(|value| {
                value.key == key
                    && value.enabled
                    && state
                        .extensions
                        .get(&value.extension_id)
                        .is_some_and(|extension| {
                            extension.current_manifest_id == value.manifest_id
                                && matches!(
                                    extension.state,
                                    ExtensionState::Enabled | ExtensionState::Running
                                )
                        })
            })
            .cloned())
    }

    async fn list_capabilities(&self, extension_id: Uuid) -> ExtensionResult<Vec<Capability>> {
        let mut values = self
            .read()?
            .capabilities
            .values()
            .filter(|value| value.extension_id == extension_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.key.clone(), value.id));
        Ok(values)
    }

    async fn find_provider(&self, id: Uuid) -> ExtensionResult<Option<Provider>> {
        Ok(self.read()?.providers.get(&id).cloned())
    }

    async fn list_providers(&self, extension_id: Uuid) -> ExtensionResult<Vec<Provider>> {
        let mut values = self
            .read()?
            .providers
            .values()
            .filter(|value| value.extension_id == extension_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.priority, value.key.clone(), value.id));
        Ok(values)
    }

    async fn list_states(&self, extension_id: Uuid) -> ExtensionResult<Vec<ExtensionStateRecord>> {
        let state = self.read()?;
        let extension = state
            .extensions
            .get(&extension_id)
            .ok_or_else(|| ExtensionError::not_found(extension_id))?;
        let mut values = state
            .states
            .values()
            .filter(|value| value.extension_id == extension_id)
            .cloned()
            .collect::<Vec<_>>();
        values.sort_by_key(|value| (value.sequence, value.id));
        validate_timeline(extension, &values)?;
        Ok(values)
    }
}

fn validate_registration(
    state: &MemoryState,
    commit: &ExtensionRegistrationCommit,
) -> ExtensionResult<()> {
    match commit.expected_extension_version {
        None => {
            if commit.extension.version != 1
                || commit.extension.state != ExtensionState::Installed
                || commit.manifest.revision != 1
                || commit.state_record.from_state.is_some()
                || state.extensions.contains_key(&commit.extension.id)
                || state
                    .extensions
                    .values()
                    .any(|value| value.key == commit.extension.key)
            {
                return Err(ExtensionError::Conflict(
                    "Extension install identity or initial state is invalid".into(),
                ));
            }
        }
        Some(expected) => {
            let current = state
                .extensions
                .get(&commit.extension.id)
                .ok_or_else(|| ExtensionError::not_found(commit.extension.id))?;
            validate_extension_update(current, &commit.extension, expected)?;
            let next_revision = state
                .manifests
                .values()
                .filter(|value| value.extension_id == current.id)
                .map(|value| value.revision)
                .max()
                .unwrap_or(0)
                .saturating_add(1);
            if current.state != ExtensionState::Disabled
                || commit.extension.state != ExtensionState::Installed
                || commit.manifest.revision != next_revision
                || commit.state_record.from_state != Some(ExtensionState::Disabled)
            {
                return Err(ExtensionError::InvalidState(
                    "Extension upgrade must replace a Disabled revision".into(),
                ));
            }
        }
    }
    if commit.capabilities.iter().any(|candidate| {
        state.capabilities.values().any(|current| {
            current.extension_id != commit.extension.id && current.key == candidate.key
        })
    }) {
        return Err(ExtensionError::Conflict(
            "Capability key is already owned by another Extension".into(),
        ));
    }
    Ok(())
}

fn validate_extension_update(
    current: &Extension,
    next: &Extension,
    expected: u64,
) -> ExtensionResult<()> {
    if current.version != expected
        || next.version != expected.saturating_add(1)
        || current.id != next.id
        || current.key != next.key
        || current.created_at != next.created_at
        || next.updated_at < current.updated_at
    {
        return Err(ExtensionError::Conflict(
            "Extension optimistic version or immutable identity conflict".into(),
        ));
    }
    Ok(())
}

fn validate_capability_update(state: &MemoryState, value: &Capability) -> ExtensionResult<()> {
    let current = state
        .capabilities
        .get(&value.id)
        .ok_or_else(|| ExtensionError::not_found(value.id))?;
    if current.version.saturating_add(1) != value.version
        || current.extension_id != value.extension_id
        || current.manifest_id != value.manifest_id
        || current.key != value.key
        || current.created_at != value.created_at
    {
        return Err(ExtensionError::Conflict(
            "Capability optimistic version or immutable identity conflict".into(),
        ));
    }
    Ok(())
}

fn validate_provider_update(state: &MemoryState, value: &Provider) -> ExtensionResult<()> {
    let current = state
        .providers
        .get(&value.id)
        .ok_or_else(|| ExtensionError::not_found(value.id))?;
    if current.version.saturating_add(1) != value.version
        || current.extension_id != value.extension_id
        || current.manifest_id != value.manifest_id
        || current.key != value.key
        || current.created_at != value.created_at
    {
        return Err(ExtensionError::Conflict(
            "Provider optimistic version or immutable identity conflict".into(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_timeline(
    extension: &Extension,
    values: &[ExtensionStateRecord],
) -> ExtensionResult<()> {
    let Some(first) = values.first() else {
        return Err(ExtensionError::Validation(
            "Extension has no state timeline".into(),
        ));
    };
    if first.sequence != 1
        || first.from_state.is_some()
        || first.to_state != ExtensionState::Installed
        || values.windows(2).any(|pair| {
            pair[0].sequence >= pair[1].sequence || pair[1].from_state != Some(pair[0].to_state)
        })
        || values.last().is_none_or(|value| {
            value.to_state != extension.state || value.sequence > extension.version
        })
    {
        return Err(ExtensionError::Validation(
            "Extension state timeline is inconsistent".into(),
        ));
    }
    Ok(())
}
