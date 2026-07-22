use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{MarketplacePackage, MarketplaceQuery, MarketplaceSnapshot};
use crate::error::MarketplaceResult;

#[async_trait]
pub trait MarketplaceStore: Send + Sync {
    async fn record(&self, pkg: &MarketplacePackage, actor: &str) -> MarketplaceResult<()>;
    async fn find(&self, id: Uuid) -> MarketplaceResult<Option<MarketplacePackage>>;
    async fn find_by_key(&self, key: &str, version: &str) -> MarketplaceResult<Option<MarketplacePackage>>;
    async fn list(&self, query: &MarketplaceQuery) -> MarketplaceResult<Vec<MarketplacePackage>>;
    async fn count(&self, query: &MarketplaceQuery) -> MarketplaceResult<u64>;
    async fn snapshot(&self) -> MarketplaceResult<MarketplaceSnapshot>;
}

pub trait MarketplaceObserver: Send + Sync {
    fn on_publish(&self, pkg: &MarketplacePackage);
}