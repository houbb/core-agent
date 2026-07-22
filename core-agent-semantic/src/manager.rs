use std::sync::Arc;

use uuid::Uuid;

use crate::domain::{
    Entity, EntityType, GraphQuery, Relation, RelationType, SemanticGraph,
};
use crate::error::SemanticResult;
use crate::infrastructure::{EntityExtractor, RelationExtractor, SharedGraphStore};
use core_agent_document::DocumentAST;

pub struct SemanticManagerBuilder {
    store: SharedGraphStore,
    entity_extractor: Arc<dyn EntityExtractor>,
    relation_extractor: Arc<dyn RelationExtractor>,
}

impl Default for SemanticManagerBuilder {
    fn default() -> Self {
        Self {
            store: Arc::new(crate::defaults::InMemoryGraphStore::default()),
            entity_extractor: Arc::new(crate::defaults::DefaultEntityExtractor),
            relation_extractor: Arc::new(crate::defaults::DefaultRelationExtractor),
        }
    }
}

impl SemanticManagerBuilder {
    pub fn store(mut self, value: SharedGraphStore) -> Self {
        self.store = value;
        self
    }

    pub fn entity_extractor(mut self, value: Arc<dyn EntityExtractor>) -> Self {
        self.entity_extractor = value;
        self
    }

    pub fn relation_extractor(mut self, value: Arc<dyn RelationExtractor>) -> Self {
        self.relation_extractor = value;
        self
    }

    pub fn build(self) -> SemanticManager {
        SemanticManager {
            store: self.store,
            entity_extractor: self.entity_extractor,
            relation_extractor: self.relation_extractor,
        }
    }
}

pub struct SemanticManager {
    store: SharedGraphStore,
    entity_extractor: Arc<dyn EntityExtractor>,
    relation_extractor: Arc<dyn RelationExtractor>,
}

impl SemanticManager {
    pub fn builder() -> SemanticManagerBuilder {
        SemanticManagerBuilder::default()
    }

    pub fn new(store: SharedGraphStore) -> Self {
        Self::builder().store(store).build()
    }

    /// Extract entities and relations from a document AST and store them
    pub async fn extract_from_document(
        &self,
        document_id: Uuid,
        ast: &DocumentAST,
        actor: &str,
    ) -> SemanticResult<(Vec<Entity>, Vec<Relation>)> {
        let entities = self.entity_extractor.extract_entities(ast)?;
        let entity_ids: Vec<Entity> = entities
            .into_iter()
            .map(|mut e| {
                e.source_document_id = Some(document_id);
                e.actor = actor.into();
                e
            })
            .collect();
        for entity in &entity_ids {
            self.store.save_entity(entity, actor).await?;
        }
        let relations = self.relation_extractor.extract_relations(&entity_ids, ast)?;
        let relations: Vec<Relation> = relations
            .into_iter()
            .map(|mut r| {
                r.source_document_id = Some(document_id);
                r.actor = actor.into();
                r
            })
            .collect();
        for relation in &relations {
            self.store.save_relation(relation, actor).await?;
        }
        Ok((entity_ids, relations))
    }

    /// Query the graph: find related entities via BFS
    pub async fn query_graph(&self, query: &GraphQuery) -> SemanticResult<Vec<(Entity, Relation, usize)>> {
        let graph = self.store.load_graph().await?;
        if let Some(name) = &query.entity_name {
            let entities = self.store.search_entities(name).await?;
            if let Some(start) = entities.first() {
                return Ok(graph.bfs(start.id, query.max_depth));
            }
        }
        Ok(Vec::new())
    }

    /// Find all related entities from a starting entity
    pub async fn find_related(
        &self,
        entity_id: Uuid,
        max_depth: usize,
    ) -> SemanticResult<Vec<(Entity, Relation, usize)>> {
        let graph = self.store.load_graph().await?;
        Ok(graph.bfs(entity_id, max_depth))
    }

    pub async fn get_entity(&self, id: Uuid) -> SemanticResult<Option<Entity>> {
        self.store.find_entity(id).await
    }

    pub async fn get_relations(&self, entity_id: Uuid) -> SemanticResult<Vec<Relation>> {
        self.store.find_relations(entity_id).await
    }

    pub async fn search_entities(&self, name_query: &str) -> SemanticResult<Vec<Entity>> {
        self.store.search_entities(name_query).await
    }

    pub async fn get_graph_summary(&self) -> SemanticResult<(usize, usize)> {
        let entities = self.store.list_entities().await?;
        let relations = self.store.list_relations().await?;
        Ok((entities.len(), relations.len()))
    }

    /// Get reference to store (for testing)
    pub fn store_ref(&self) -> SharedGraphStore {
        self.store.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_agent_document::DocumentAST;

    #[tokio::test]
    async fn extract_from_document_works() {
        let manager = SemanticManager::builder().build();
        let mut ast = DocumentAST::new();
        ast.sections.push(core_agent_document::DocumentSection::new(
            "PaymentService", 1, "PaymentService depends on Database",
        ));
        ast.sections.push(core_agent_document::DocumentSection::new(
            "OrderService", 1, "OrderService calls PaymentService",
        ));

        let (entities, relations) = manager
            .extract_from_document(Uuid::new_v4(), &ast, "tester")
            .await
            .unwrap();
        assert!(!entities.is_empty());
        assert!(!relations.is_empty());
    }

    #[tokio::test]
    async fn bfs_traversal_returns_related() {
        let manager = SemanticManager::builder().build();
        let mut ast = DocumentAST::new();
        ast.sections.push(core_agent_document::DocumentSection::new(
            "OrderService", 1, "OrderService depends on PaymentService",
        ));
        ast.sections.push(core_agent_document::DocumentSection::new(
            "PaymentService", 1, "PaymentService uses Database",
        ));

        let doc_id = Uuid::new_v4();
        manager.extract_from_document(doc_id, &ast, "tester").await.unwrap();

        let order = manager.search_entities("OrderService").await.unwrap();
        assert!(!order.is_empty());

        let related = manager.find_related(order[0].id, 3).await.unwrap();
        assert!(!related.is_empty());
    }

    #[tokio::test]
    async fn graph_summary_counts() {
        let manager = SemanticManager::builder().build();
        let (entities, rels) = manager.get_graph_summary().await.unwrap();
        assert_eq!(entities, 0);
        assert_eq!(rels, 0);
    }
}