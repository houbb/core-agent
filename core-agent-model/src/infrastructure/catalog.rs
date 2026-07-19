use std::collections::BTreeMap;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::domain::{ModelProfile, ProviderDefinition};
use crate::error::ModelResult;

use super::ModelCatalog;

/// Process-local Catalog for embedded use and tests.
#[derive(Default)]
pub struct InMemoryModelCatalog {
    providers: RwLock<BTreeMap<String, ProviderDefinition>>,
    profiles: RwLock<BTreeMap<String, ModelProfile>>,
}

#[async_trait]
impl ModelCatalog for InMemoryModelCatalog {
    async fn upsert_provider(&self, provider: &ProviderDefinition) -> ModelResult<()> {
        provider.validate()?;
        let mut providers = self.providers.write().await;
        let mut stored = provider.clone();
        if let Some(existing) = providers.get(&provider.key) {
            stored.id = existing.id;
            stored.created_at = existing.created_at;
        }
        providers.insert(provider.key.clone(), stored);
        Ok(())
    }

    async fn find_provider(&self, key: &str) -> ModelResult<Option<ProviderDefinition>> {
        Ok(self.providers.read().await.get(key).cloned())
    }

    async fn list_providers(&self) -> ModelResult<Vec<ProviderDefinition>> {
        Ok(self.providers.read().await.values().cloned().collect())
    }

    async fn upsert_profile(&self, profile: &ModelProfile) -> ModelResult<()> {
        profile.validate()?;
        let mut profiles = self.profiles.write().await;
        let mut stored = profile.clone();
        if let Some(existing) = profiles.get(&profile.key) {
            stored.id = existing.id;
            stored.created_at = existing.created_at;
        }
        profiles.insert(profile.key.clone(), stored);
        Ok(())
    }

    async fn find_profile(&self, key: &str) -> ModelResult<Option<ModelProfile>> {
        Ok(self.profiles.read().await.get(key).cloned())
    }

    async fn list_profiles(&self) -> ModelResult<Vec<ModelProfile>> {
        Ok(self.profiles.read().await.values().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn catalog_upserts_by_stable_key() {
        let catalog = InMemoryModelCatalog::default();
        let mut first = ModelProfile::new("coding", "p", "m1");
        catalog.upsert_profile(&first).await.unwrap();
        first.model = "m2".into();
        catalog.upsert_profile(&first).await.unwrap();

        assert_eq!(catalog.list_profiles().await.unwrap().len(), 1);
        assert_eq!(
            catalog.find_profile("coding").await.unwrap().unwrap().model,
            "m2"
        );
    }
}
