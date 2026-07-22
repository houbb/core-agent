use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::domain::{KnowledgeCategory, KnowledgeItem, KnowledgeStatus};
use crate::error::{KnowledgeError, KnowledgeResult};
use crate::infrastructure::{KnowledgeSearch, KnowledgeStore};

// ── InMemoryKnowledgeStore ──

#[derive(Default)]
struct InMemoryState {
    items: HashMap<Uuid, KnowledgeItem>,
    categories: HashMap<Uuid, KnowledgeCategory>,
}

#[derive(Default)]
pub struct InMemoryKnowledgeStore {
    state: RwLock<InMemoryState>,
}

impl InMemoryKnowledgeStore {
    fn read(&self) -> KnowledgeResult<std::sync::RwLockReadGuard<'_, InMemoryState>> {
        self.state
            .read()
            .map_err(|_| KnowledgeError::Internal("store lock poisoned".into()))
    }

    fn write(&self) -> KnowledgeResult<std::sync::RwLockWriteGuard<'_, InMemoryState>> {
        self.state
            .write()
            .map_err(|_| KnowledgeError::Internal("store lock poisoned".into()))
    }
}

#[async_trait]
impl KnowledgeStore for InMemoryKnowledgeStore {
    async fn save_item(&self, item: &KnowledgeItem, actor: &str) -> KnowledgeResult<()> {
        if actor.trim().is_empty() {
            return Err(KnowledgeError::Validation("actor must not be empty".into()));
        }
        let mut state = self.write()?;
        state.items.insert(item.id, item.clone());
        Ok(())
    }

    async fn find_item(&self, id: Uuid) -> KnowledgeResult<Option<KnowledgeItem>> {
        Ok(self.read()?.items.get(&id).cloned())
    }

    async fn list_items(&self) -> KnowledgeResult<Vec<KnowledgeItem>> {
        let mut items: Vec<_> = self.read()?.items.values().cloned().collect();
        items.sort_by_key(|i| (std::cmp::Reverse(i.updated_at), i.id));
        Ok(items)
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: KnowledgeStatus,
        version: u64,
        actor: &str,
    ) -> KnowledgeResult<()> {
        if actor.trim().is_empty() {
            return Err(KnowledgeError::Validation("actor must not be empty".into()));
        }
        let mut state = self.write()?;
        let item = state
            .items
            .get_mut(&id)
            .ok_or_else(|| KnowledgeError::NotFound(id.to_string()))?;
        if item.version != version {
            return Err(KnowledgeError::Conflict(format!(
                "expected version {version}, found {}",
                item.version
            )));
        }
        item.status = status;
        item.version = version.saturating_add(1);
        item.updated_at = Utc::now();
        item.actor = actor.into();
        Ok(())
    }

    async fn delete_item(&self, id: Uuid, actor: &str) -> KnowledgeResult<()> {
        if actor.trim().is_empty() {
            return Err(KnowledgeError::Validation("actor must not be empty".into()));
        }
        let mut state = self.write()?;
        state
            .items
            .remove(&id)
            .ok_or_else(|| KnowledgeError::NotFound(id.to_string()))?;
        Ok(())
    }

    async fn save_category(&self, category: &KnowledgeCategory, actor: &str) -> KnowledgeResult<()> {
        if actor.trim().is_empty() {
            return Err(KnowledgeError::Validation("actor must not be empty".into()));
        }
        let mut state = self.write()?;
        state.categories.insert(category.id, category.clone());
        Ok(())
    }

    async fn list_categories(&self) -> KnowledgeResult<Vec<KnowledgeCategory>> {
        let mut cats: Vec<_> = self.read()?.categories.values().cloned().collect();
        cats.sort_by_key(|c| (c.name.clone(), c.id));
        Ok(cats)
    }
}

// ── DefaultKnowledgeSearch ──

pub struct DefaultKnowledgeSearch {
    store: Arc<dyn KnowledgeStore>,
}

impl DefaultKnowledgeSearch {
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl KnowledgeSearch for DefaultKnowledgeSearch {
    async fn search(
        &self,
        query: &str,
        _namespace: &str,
        top_k: usize,
    ) -> KnowledgeResult<Vec<KnowledgeItem>> {
        let needle = query.to_lowercase();
        let items: Vec<KnowledgeItem> = self.store.list_items().await?;
        let mut scored: Vec<(KnowledgeItem, usize)> = items
            .into_iter()
            .filter_map(|item| {
                let title = item.title.to_lowercase();
                let content = item.content.to_lowercase();
                let mut score = 0;
                if title.contains(&needle) {
                    score += 10;
                }
                if content.contains(&needle) {
                    score += 5;
                }
                if item.tags.iter().any(|t: &String| t.to_lowercase().contains(&needle)) {
                    score += 3;
                }
                if score > 0 { Some((item, score)) } else { None }
            })
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.truncate(top_k);
        Ok(scored.into_iter().map(|(item, _)| item).collect())
    }
}