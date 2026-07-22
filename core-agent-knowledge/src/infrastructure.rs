use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{KnowledgeCategory, KnowledgeItem, KnowledgeStatus};
use crate::error::KnowledgeResult;

// ── KnowledgeStore ──

#[async_trait]
pub trait KnowledgeStore: Send + Sync {
    async fn save_item(&self, item: &KnowledgeItem, actor: &str) -> KnowledgeResult<()>;
    async fn find_item(&self, id: Uuid) -> KnowledgeResult<Option<KnowledgeItem>>;
    async fn list_items(&self) -> KnowledgeResult<Vec<KnowledgeItem>>;
    async fn update_status(
        &self,
        id: Uuid,
        status: KnowledgeStatus,
        version: u64,
        actor: &str,
    ) -> KnowledgeResult<()>;
    async fn delete_item(&self, id: Uuid, actor: &str) -> KnowledgeResult<()>;

    async fn save_category(&self, category: &KnowledgeCategory, actor: &str) -> KnowledgeResult<()>;
    async fn list_categories(&self) -> KnowledgeResult<Vec<KnowledgeCategory>>;
}

#[async_trait]
pub trait KnowledgeSearch: Send + Sync {
    async fn search(
        &self,
        query: &str,
        namespace: &str,
        top_k: usize,
    ) -> KnowledgeResult<Vec<KnowledgeItem>>;
}

pub type SharedKnowledgeStore = Arc<dyn KnowledgeStore>;