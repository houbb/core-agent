use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::{Entity, EntityType, GraphQuery, Relation, RelationType, SemanticGraph};
use crate::error::SemanticResult;
use core_agent_document::DocumentAST;

// ── EntityExtractor ──

pub trait EntityExtractor: Send + Sync {
    fn extract_entities(&self, ast: &DocumentAST) -> SemanticResult<Vec<Entity>>;
}

// ── RelationExtractor ──

pub trait RelationExtractor: Send + Sync {
    fn extract_relations(&self, entities: &[Entity], ast: &DocumentAST) -> SemanticResult<Vec<Relation>>;
}

// ── GraphStore ──

#[async_trait]
pub trait GraphStore: Send + Sync {
    async fn save_entity(&self, entity: &Entity, actor: &str) -> SemanticResult<()>;
    async fn find_entity(&self, id: Uuid) -> SemanticResult<Option<Entity>>;
    async fn search_entities(&self, name_query: &str) -> SemanticResult<Vec<Entity>>;
    async fn list_entities(&self) -> SemanticResult<Vec<Entity>>;

    async fn save_relation(&self, relation: &Relation, actor: &str) -> SemanticResult<()>;
    async fn find_relations(&self, entity_id: Uuid) -> SemanticResult<Vec<Relation>>;
    async fn list_relations(&self) -> SemanticResult<Vec<Relation>>;

    async fn load_graph(&self) -> SemanticResult<SemanticGraph>;
}

pub type SharedGraphStore = Arc<dyn GraphStore>;