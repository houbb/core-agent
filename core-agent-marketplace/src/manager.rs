use std::sync::Arc;

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use crate::defaults::{InMemoryMarketplaceStore, NoopMarketplaceObserver};
use crate::domain::{
    AssetType, MarketplacePackage, MarketplaceQuery, MarketplaceSnapshot, PackageState,
    validate_actor,
};
use crate::error::{MarketplaceError, MarketplaceResult};
use crate::infrastructure::{MarketplaceObserver, MarketplaceStore};

pub struct MarketplaceManagerBuilder {
    store: Arc<dyn MarketplaceStore>,
    observers: Vec<Arc<dyn MarketplaceObserver>>,
}

impl Default for MarketplaceManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(InMemoryMarketplaceStore::default()),
            observers: Vec::new(),
        }
    }
}

impl MarketplaceManagerBuilder {
    pub fn store(mut self, value: Arc<dyn MarketplaceStore>) -> Self {
        self.store = value;
        self
    }

    pub fn observer(mut self, value: Arc<dyn MarketplaceObserver>) -> Self {
        self.observers.push(value);
        self
    }

    pub fn build(self) -> MarketplaceManager {
        MarketplaceManager {
            store: self.store,
            observers: self.observers,
        }
    }
}

pub struct MarketplaceManager {
    store: Arc<dyn MarketplaceStore>,
    observers: Vec<Arc<dyn MarketplaceObserver>>,
}

impl MarketplaceManager {
    pub fn builder() -> MarketplaceManagerBuilder {
        MarketplaceManagerBuilder::default()
    }

    pub fn new(store: Arc<dyn MarketplaceStore>) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn publish(
        &self,
        asset_type: AssetType,
        key: &str,
        name: &str,
        version: &str,
        author: &str,
        description: &str,
        content: Value,
    ) -> MarketplaceResult<MarketplacePackage> {
        let pkg = MarketplacePackage::new(asset_type, key, name, version, author, description, content)?;
        let mut pkg = pkg;
        pkg.state = PackageState::Published;
        let author_owned = pkg.author.clone();
        self.store.record(&pkg, &author_owned).await?;
        for observer in &self.observers {
            observer.on_publish(&pkg);
        }
        Ok(pkg)
    }

    pub async fn install(&self, id: Uuid, actor: &str) -> MarketplaceResult<MarketplacePackage> {
        validate_actor(actor)?;
        let mut pkg = self
            .store
            .find(id)
            .await?
            .ok_or_else(|| MarketplaceError::NotFound(id.to_string()))?;
        pkg.downloads += 1;
        pkg.updated_at = Utc::now();
        pkg.actor = actor.into();
        pkg.version_count += 1;
        Ok(pkg)
    }

    pub async fn deprecate(&self, id: Uuid, actor: &str) -> MarketplaceResult<MarketplacePackage> {
        validate_actor(actor)?;
        let mut pkg = self
            .store
            .find(id)
            .await?
            .ok_or_else(|| MarketplaceError::NotFound(id.to_string()))?;
        pkg.state = PackageState::Deprecated;
        pkg.updated_at = Utc::now();
        pkg.actor = actor.into();
        pkg.version_count += 1;
        Ok(pkg)
    }

    pub async fn find(&self, id: Uuid) -> MarketplaceResult<Option<MarketplacePackage>> {
        self.store.find(id).await
    }

    pub async fn find_by_key(
        &self,
        key: &str,
        version: &str,
    ) -> MarketplaceResult<Option<MarketplacePackage>> {
        self.store.find_by_key(key, version).await
    }

    pub async fn list(&self, query: &MarketplaceQuery) -> MarketplaceResult<Vec<MarketplacePackage>> {
        self.store.list(query).await
    }

    pub async fn count(&self, query: &MarketplaceQuery) -> MarketplaceResult<u64> {
        self.store.count(query).await
    }

    pub async fn snapshot(&self) -> MarketplaceResult<MarketplaceSnapshot> {
        self.store.snapshot().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_and_list() {
        let manager = MarketplaceManager::builder().build();
        let pkg = manager
            .publish(
                AssetType::Skill,
                "redis-diagnosis",
                "Redis Diagnosis",
                "1.0.0",
                "core-agent",
                "Diagnose Redis issues",
                serde_json::json!({"steps": []}),
            )
            .await
            .unwrap();
        assert_eq!(pkg.state, PackageState::Published);

        let list = manager
            .list(&MarketplaceQuery {
                asset_type: Some(AssetType::Skill),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn install_increments_downloads() {
        let manager = MarketplaceManager::builder().build();
        let pkg = manager
            .publish(
                AssetType::Agent,
                "test-agent",
                "Test Agent",
                "1.0",
                "author",
                "desc",
                Value::Null,
            )
            .await
            .unwrap();

        let installed = manager.install(pkg.id, "user").await.unwrap();
        assert_eq!(installed.downloads, 1);
    }

    #[tokio::test]
    async fn deprecate_works() {
        let manager = MarketplaceManager::builder().build();
        let pkg = manager
            .publish(
                AssetType::Skill,
                "old-skill",
                "Old Skill",
                "1.0",
                "author",
                "desc",
                Value::Null,
            )
            .await
            .unwrap();
        let deprecated = manager.deprecate(pkg.id, "admin").await.unwrap();
        assert_eq!(deprecated.state, PackageState::Deprecated);
    }
}