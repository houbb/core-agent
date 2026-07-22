use std::sync::Arc;

use chrono::Utc;
use uuid::Uuid;

use crate::domain::{
    KnowledgeCategory, KnowledgeItem, KnowledgeKind, KnowledgeSourceKind, KnowledgeStatus,
};
use crate::error::KnowledgeResult;
use crate::infrastructure::{KnowledgeSearch, SharedKnowledgeStore};

pub struct KnowledgeManagerBuilder {
    store: SharedKnowledgeStore,
    search: Arc<dyn KnowledgeSearch>,
}

impl Default for KnowledgeManagerBuilder {
    fn default() -> Self {
        let store = Arc::new(crate::defaults::InMemoryKnowledgeStore::default());
        let search = Arc::new(crate::defaults::DefaultKnowledgeSearch::new(store.clone()));
        Self { store, search }
    }
}

impl KnowledgeManagerBuilder {
    pub fn store(mut self, value: SharedKnowledgeStore) -> Self {
        self.store = value;
        self
    }

    pub fn search(mut self, value: Arc<dyn KnowledgeSearch>) -> Self {
        self.search = value;
        self
    }

    pub fn build(self) -> KnowledgeManager {
        KnowledgeManager {
            store: self.store,
            search: self.search,
        }
    }
}

pub struct KnowledgeManager {
    store: SharedKnowledgeStore,
    search: Arc<dyn KnowledgeSearch>,
}

impl KnowledgeManager {
    pub fn builder() -> KnowledgeManagerBuilder {
        KnowledgeManagerBuilder::default()
    }

    pub fn new(store: SharedKnowledgeStore) -> Self {
        Self::builder().store(store).build()
    }

    pub async fn create_knowledge(
        &self,
        item: &KnowledgeItem,
        actor: &str,
    ) -> KnowledgeResult<KnowledgeItem> {
        let mut item = item.clone();
        item.validate()?;
        item.status = KnowledgeStatus::Created;
        item.actor = actor.into();
        self.store.save_item(&item, actor).await?;
        Ok(item)
    }

    pub async fn publish_knowledge(&self, id: Uuid, actor: &str) -> KnowledgeResult<KnowledgeItem> {
        let item = self
            .store
            .find_item(id)
            .await?
            .ok_or_else(|| crate::error::KnowledgeError::NotFound(id.to_string()))?;
        self.store
            .update_status(id, KnowledgeStatus::Published, item.version, actor)
            .await?;
        self.store.find_item(id).await.map(|o| o.unwrap())
    }

    pub async fn archive_knowledge(&self, id: Uuid, actor: &str) -> KnowledgeResult<KnowledgeItem> {
        let item = self
            .store
            .find_item(id)
            .await?
            .ok_or_else(|| crate::error::KnowledgeError::NotFound(id.to_string()))?;
        self.store
            .update_status(id, KnowledgeStatus::Archived, item.version, actor)
            .await?;
        self.store.find_item(id).await.map(|o| o.unwrap())
    }

    pub async fn search_knowledge(
        &self,
        query: &str,
        namespace: &str,
        top_k: usize,
    ) -> KnowledgeResult<Vec<KnowledgeItem>> {
        self.search.search(query, namespace, top_k).await
    }

    pub async fn list_knowledge(&self) -> KnowledgeResult<Vec<KnowledgeItem>> {
        self.store.list_items().await
    }

    pub async fn get_knowledge(&self, id: Uuid) -> KnowledgeResult<Option<KnowledgeItem>> {
        self.store.find_item(id).await
    }

    pub async fn delete_knowledge(&self, id: Uuid, actor: &str) -> KnowledgeResult<()> {
        self.store.delete_item(id, actor).await
    }

    pub async fn import_from_document(
        &self,
        document_id: Uuid,
        title: &str,
        content: &str,
        actor: &str,
    ) -> KnowledgeResult<KnowledgeItem> {
        let mut item = KnowledgeItem::new(
            KnowledgeKind::Document,
            title,
            content,
            KnowledgeSourceKind::Document,
            actor,
            actor,
        );
        item.document_id = Some(document_id);
        item.status = KnowledgeStatus::Published;
        item.actor = actor.into();
        self.store.save_item(&item, actor).await?;
        Ok(item)
    }

    pub async fn create_category(
        &self,
        name: &str,
        parent_id: Option<Uuid>,
        actor: &str,
    ) -> KnowledgeResult<KnowledgeCategory> {
        let mut category = KnowledgeCategory::new(name, actor);
        category.parent_id = parent_id;
        self.store.save_category(&category, actor).await?;
        Ok(category)
    }

    pub async fn get_knowledge_tree(&self) -> KnowledgeResult<Vec<KnowledgeCategory>> {
        self.store.list_categories().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn knowledge_crud_lifecycle() {
        let manager = KnowledgeManager::builder().build();
        let item = KnowledgeItem::new(
            KnowledgeKind::Document,
            "Test Knowledge",
            "Test content",
            KnowledgeSourceKind::Manual,
            "owner",
            "tester",
        );
        let created = manager.create_knowledge(&item, "tester").await.unwrap();
        assert_eq!(created.status, KnowledgeStatus::Created);

        let published = manager.publish_knowledge(created.id, "publisher").await.unwrap();
        assert_eq!(published.status, KnowledgeStatus::Published);

        let archived = manager.archive_knowledge(published.id, "archiver").await.unwrap();
        assert_eq!(archived.status, KnowledgeStatus::Archived);
    }

    #[tokio::test]
    async fn search_finds_matching_knowledge() {
        let manager = KnowledgeManager::builder().build();
        let item = KnowledgeItem::new(
            KnowledgeKind::Document,
            "Payment Gateway",
            "Handles payment processing",
            KnowledgeSourceKind::Manual,
            "owner",
            "tester",
        );
        manager.create_knowledge(&item, "tester").await.unwrap();
        let results = manager.search_knowledge("payment", "default", 10).await.unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn import_from_document_creates_published() {
        let manager = KnowledgeManager::builder().build();
        let item = manager
            .import_from_document(Uuid::new_v4(), "Doc Import", "Imported content", "tester")
            .await
            .unwrap();
        assert_eq!(item.status, KnowledgeStatus::Published);
        assert!(item.document_id.is_some());
    }
}