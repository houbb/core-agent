use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::SemanticResult;

pub type SemanticMetadata = BTreeMap<String, Value>;

macro_rules! string_enum {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }
            pub fn parse(value: &str) -> Option<Self> {
                match value { $($value => Some(Self::$variant),)+ _ => None }
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EntityType {
    Service,
    Database,
    Component,
    Concept,
    Person,
    System,
    Api,
    Other,
}
string_enum!(EntityType {
    Service => "SERVICE",
    Database => "DATABASE",
    Component => "COMPONENT",
    Concept => "CONCEPT",
    Person => "PERSON",
    System => "SYSTEM",
    Api => "API",
    Other => "OTHER",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RelationType {
    DependsOn,
    Uses,
    Contains,
    CommunicatesWith,
    Implements,
    DeploysOn,
    Other,
}
string_enum!(RelationType {
    DependsOn => "DEPENDS_ON",
    Uses => "USES",
    Contains => "CONTAINS",
    CommunicatesWith => "COMMUNICATES_WITH",
    Implements => "IMPLEMENTS",
    DeploysOn => "DEPLOYS_ON",
    Other => "OTHER",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entity {
    pub id: Uuid,
    pub name: String,
    pub entity_type: EntityType,
    pub attributes: SemanticMetadata,
    pub source_document_id: Option<Uuid>,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Entity {
    pub fn new(name: impl Into<String>, entity_type: EntityType, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            entity_type,
            attributes: BTreeMap::new(),
            source_document_id: None,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> SemanticResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 512 {
            return Err(crate::error::SemanticError::Validation(
                "entity name must be 1..=512 bytes".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub id: Uuid,
    pub source_entity_id: Uuid,
    pub target_entity_id: Uuid,
    pub relation_type: RelationType,
    pub confidence: f64,
    pub attributes: SemanticMetadata,
    pub source_document_id: Option<Uuid>,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Relation {
    pub fn new(
        source_entity_id: Uuid,
        target_entity_id: Uuid,
        relation_type: RelationType,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            source_entity_id,
            target_entity_id,
            relation_type,
            confidence: 0.8,
            attributes: BTreeMap::new(),
            source_document_id: None,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> SemanticResult<()> {
        if self.source_entity_id == self.target_entity_id {
            return Err(crate::error::SemanticError::Validation(
                "relation cannot be self-referential".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(crate::error::SemanticError::Validation(
                "relation confidence must be 0..=1".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticGraph {
    pub entities: HashMap<Uuid, Entity>,
    pub relations: Vec<Relation>,
}

impl SemanticGraph {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
            relations: Vec::new(),
        }
    }

    /// Find all entities reachable from a starting entity via BFS
    pub fn bfs(&self, start_id: Uuid, max_depth: usize) -> Vec<(Entity, Relation, usize)> {
        let mut visited = HashSet::new();
        let mut queue = Vec::new();
        let mut results = Vec::new();
        visited.insert(start_id);
        queue.push((start_id, 0));

        while let Some((current_id, depth)) = queue.pop() {
            if depth >= max_depth {
                continue;
            }
            for relation in &self.relations {
                if relation.source_entity_id == current_id {
                    let target = relation.target_entity_id;
                    if visited.insert(target) {
                        if let Some(entity) = self.entities.get(&target) {
                            results.push((entity.clone(), relation.clone(), depth + 1));
                            queue.push((target, depth + 1));
                        }
                    }
                }
            }
        }
        results
    }
}

impl Default for SemanticGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQuery {
    pub entity_name: Option<String>,
    pub entity_type: Option<EntityType>,
    pub relation_type: Option<RelationType>,
    pub max_depth: usize,
}

impl GraphQuery {
    pub fn new() -> Self {
        Self {
            entity_name: None,
            entity_type: None,
            relation_type: None,
            max_depth: 3,
        }
    }
}

impl Default for GraphQuery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_validation_works() {
        let entity = Entity::new("PaymentService", EntityType::Service, "tester");
        assert!(entity.validate().is_ok());
    }

    #[test]
    fn relation_validation_rejects_self_reference() {
        let id = Uuid::new_v4();
        let relation = Relation::new(id, id, RelationType::DependsOn, "tester");
        assert!(relation.validate().is_err());
    }

    #[test]
    fn bfs_traversal_works() {
        let mut graph = SemanticGraph::new();
        let e1 = Entity::new("OrderService", EntityType::Service, "tester");
        let e2 = Entity::new("PaymentService", EntityType::Service, "tester");
        let e3 = Entity::new("Database", EntityType::Database, "tester");
        let ids = [e1.id, e2.id, e3.id];
        graph.entities.insert(e1.id, e1);
        graph.entities.insert(e2.id, e2);
        graph.entities.insert(e3.id, e3);
        graph.relations.push(Relation::new(ids[0], ids[1], RelationType::DependsOn, "tester"));
        graph.relations.push(Relation::new(ids[1], ids[2], RelationType::Uses, "tester"));

        let results = graph.bfs(ids[0], 3);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].1.relation_type, RelationType::DependsOn);
    }
}