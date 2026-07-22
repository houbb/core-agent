use std::collections::BTreeMap;
use std::sync::RwLock;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{
    MarketplacePackage, MarketplaceQuery, MarketplaceSnapshot, PackageState, validate_actor,
};
use crate::error::{MarketplaceError, MarketplaceResult};
use crate::infrastructure::MarketplaceStore;

#[derive(Default)]
pub struct InMemoryMarketplaceStore {
    packages: RwLock<Vec<MarketplacePackage>>,
}

#[async_trait]
impl MarketplaceStore for InMemoryMarketplaceStore {
    async fn record(&self, pkg: &MarketplacePackage, actor: &str) -> MarketplaceResult<()> {
        validate_actor(actor)?;
        pkg.validate()?;
        let mut packages = self
            .packages
            .write()
            .map_err(|_| MarketplaceError::Internal("lock poisoned".into()))?;
        if packages.iter().any(|p| p.id == pkg.id)
            || packages
                .iter()
                .any(|p| p.key == pkg.key && p.version == pkg.version)
        {
            return Err(MarketplaceError::Conflict("package already exists".into()));
        }
        packages.push(pkg.clone());
        Ok(())
    }

    async fn find(&self, id: Uuid) -> MarketplaceResult<Option<MarketplacePackage>> {
        let packages = self
            .packages
            .read()
            .map_err(|_| MarketplaceError::Internal("lock poisoned".into()))?;
        Ok(packages.iter().find(|p| p.id == id).cloned())
    }

    async fn find_by_key(
        &self,
        key: &str,
        version: &str,
    ) -> MarketplaceResult<Option<MarketplacePackage>> {
        let packages = self
            .packages
            .read()
            .map_err(|_| MarketplaceError::Internal("lock poisoned".into()))?;
        Ok(packages
            .iter()
            .find(|p| p.key == key && p.version == version)
            .cloned())
    }

    async fn list(&self, query: &MarketplaceQuery) -> MarketplaceResult<Vec<MarketplacePackage>> {
        query.validate()?;
        let packages = self
            .packages
            .read()
            .map_err(|_| MarketplaceError::Internal("lock poisoned".into()))?;
        Ok(packages
            .iter()
            .filter(|p| {
                query.asset_type.map_or(true, |t| p.asset_type == t)
                    && query.state.map_or(true, |s| p.state == s)
                    && query
                        .author
                        .as_ref()
                        .map_or(true, |a| p.author == *a)
                    && query.rating_min.map_or(true, |r| p.rating >= r)
                    && query.search.as_ref().map_or(true, |s| {
                        p.name.contains(s)
                            || p.key.contains(s)
                            || p.description.contains(s)
                    })
                    && query.tags.as_ref().map_or(true, |t| {
                        t.iter().all(|tag| p.tags.contains(tag))
                    })
            })
            .skip(query.offset)
            .take(query.limit)
            .cloned()
            .collect())
    }

    async fn count(&self, query: &MarketplaceQuery) -> MarketplaceResult<u64> {
        let packages = self
            .packages
            .read()
            .map_err(|_| MarketplaceError::Internal("lock poisoned".into()))?;
        Ok(packages
            .iter()
            .filter(|p| query.asset_type.map_or(true, |t| p.asset_type == t))
            .count() as u64)
    }

    async fn snapshot(&self) -> MarketplaceResult<MarketplaceSnapshot> {
        let packages = self
            .packages
            .read()
            .map_err(|_| MarketplaceError::Internal("lock poisoned".into()))?;
        let mut by_type = BTreeMap::new();
        let mut by_state = BTreeMap::new();
        let mut downloads = 0u64;
        let mut rating_sum = 0.0;

        for p in packages.iter() {
            *by_type
                .entry(p.asset_type.as_str().to_string())
                .or_insert(0u64) += 1;
            *by_state
                .entry(p.state.as_str().to_string())
                .or_insert(0u64) += 1;
            downloads += p.downloads;
            rating_sum += p.rating;
        }

        let avg = if packages.is_empty() {
            0.0
        } else {
            rating_sum / packages.len() as f64
        };

        Ok(MarketplaceSnapshot {
            total_packages: packages.len() as u64,
            by_type,
            by_state,
            total_downloads: downloads,
            avg_rating: (avg * 100.0).round() / 100.0,
        })
    }
}

pub struct NoopMarketplaceObserver;

impl crate::infrastructure::MarketplaceObserver for NoopMarketplaceObserver {
    fn on_publish(&self, _pkg: &MarketplacePackage) {}
}