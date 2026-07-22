use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::domain::{
    Entity, EntityType, GraphQuery, Relation, RelationType, SemanticGraph,
};
use crate::error::{SemanticError, SemanticResult};
use crate::infrastructure::{EntityExtractor, GraphStore, RelationExtractor};
use core_agent_document::DocumentAST;

// ── DefaultEntityExtractor ──

pub struct DefaultEntityExtractor;

impl EntityExtractor for DefaultEntityExtractor {
    fn extract_entities(&self, ast: &DocumentAST) -> SemanticResult<Vec<Entity>> {
        let mut entities = Vec::new();
        let mut seen = HashSet::new();

        // Extract from section headings
        for section in &ast.sections {
            let name = section.heading.trim();
            if name.is_empty() || seen.contains(name) {
                continue;
            }
            // Identify likely entity types
            let entity_type = guess_entity_type(name);
            if entity_type != EntityType::Other || name.chars().any(|c| c.is_uppercase()) {
                seen.insert(name.to_string());
                entities.push(Entity::new(name, entity_type, "extractor"));
            }
        }

        // Extract from code blocks (service names, API endpoints)
        for block in &ast.code_blocks {
            for line in block.code.lines() {
                let line = line.trim();
                if line.starts_with("class ") || line.starts_with("struct ") {
                    if let Some(name) = line.split_whitespace().nth(1) {
                        let name = name.trim_end_matches(&['{', ':', ';', ' '][..]);
                        if !seen.contains(name) {
                            seen.insert(name.to_string());
                            entities.push(Entity::new(name, EntityType::Component, "extractor"));
                        }
                    }
                }
                if line.contains("depends on") || line.contains("calls") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    for part in parts {
                        let clean = part.trim_end_matches(&['.', ',', ';', '(', ')'][..]);
                        if !clean.is_empty()
                            && clean.chars().next().unwrap().is_uppercase()
                            && !seen.contains(clean)
                        {
                            seen.insert(clean.to_string());
                            entities.push(Entity::new(clean, guess_entity_type(clean), "extractor"));
                        }
                    }
                }
            }
        }

        // Extract from links
        for link in &ast.links {
            let name = link.text.trim();
            if !name.is_empty() && !seen.contains(name) && name.chars().any(|c| c.is_uppercase()) {
                seen.insert(name.to_string());
                entities.push(Entity::new(name, EntityType::Other, "extractor"));
            }
        }

        Ok(entities)
    }
}

fn guess_entity_type(name: &str) -> EntityType {
    let lower = name.to_lowercase();
    if lower.contains("service") || lower.contains("api") || lower.ends_with("api") {
        EntityType::Service
    } else if lower.contains("db") || lower.contains("database") || lower.contains("sql") {
        EntityType::Database
    } else if lower.contains("system") {
        EntityType::System
    } else if lower.contains("component") || lower.contains("module") {
        EntityType::Component
    } else {
        EntityType::Other
    }
}

// ── DefaultRelationExtractor ──

pub struct DefaultRelationExtractor;

impl RelationExtractor for DefaultRelationExtractor {
    fn extract_relations(
        &self,
        entities: &[Entity],
        ast: &DocumentAST,
    ) -> SemanticResult<Vec<Relation>> {
        let mut relations = Vec::new();
        let entity_map: HashMap<&str, &Entity> = entities
            .iter()
            .map(|e| (e.name.as_str(), e))
            .collect();

        // Extract from code blocks
        for block in &ast.code_blocks {
            for line in block.code.lines() {
                let line = line.trim().to_lowercase();

                // "X depends on Y" -> depends_on
                if let Some(caps) = line.find("depends on") {
                    let before = &line[..caps].trim();
                    let after = &line[caps + "depends on".len()..].trim();
                    let src = entity_map.keys().find(|k| before.contains(&k.to_lowercase()));
                    let tgt = entity_map.keys().find(|k| after.contains(&k.to_lowercase()));
                    if let (Some(src), Some(tgt)) = (src, tgt) {
                        if let (Some(s), Some(t)) = (entity_map.get(src), entity_map.get(tgt)) {
                            relations.push(Relation::new(s.id, t.id, RelationType::DependsOn, "extractor"));
                        }
                    }
                }

                // "X calls Y" or "X uses Y" -> uses
                for keyword in &["calls", "uses", "invokes"] {
                    if let Some(pos) = line.find(keyword) {
                        let before = &line[..pos].trim();
                        let after = &line[pos + keyword.len()..].trim();
                        let src = entity_map.keys().find(|k| before.contains(&k.to_lowercase()));
                        let tgt = entity_map.keys().find(|k| after.contains(&k.to_lowercase()));
                        if let (Some(src), Some(tgt)) = (src, tgt) {
                            if let (Some(s), Some(t)) = (entity_map.get(src), entity_map.get(tgt)) {
                                relations.push(Relation::new(s.id, t.id, RelationType::Uses, "extractor"));
                            }
                        }
                    }
                }

                // "X communicates with Y"
                if line.contains("communicates with") {
                    let pos = line.find("communicates with").unwrap();
                    let before = &line[..pos].trim();
                    let after = &line[pos + "communicates with".len()..].trim();
                    let src = entity_map.keys().find(|k| before.contains(&k.to_lowercase()));
                    let tgt = entity_map.keys().find(|k| after.contains(&k.to_lowercase()));
                    if let (Some(src), Some(tgt)) = (src, tgt) {
                        if let (Some(s), Some(t)) = (entity_map.get(src), entity_map.get(tgt)) {
                            relations.push(Relation::new(s.id, t.id, RelationType::CommunicatesWith, "extractor"));
                        }
                    }
                }
            }
        }

        // Extract from section content (heading-based relations)
        for section in &ast.sections {
            let lower = section.content.to_lowercase();
            let heading = section.heading.to_lowercase();
            let src = entity_map.keys().find(|k| heading.contains(&k.to_lowercase()));
            for keyword in &["depends on", "uses", "calls", "contains"] {
                if let Some(pos) = lower.find(keyword) {
                    let before = &lower[..pos].trim();
                    let after = &lower[pos + keyword.len()..].trim();
                    let tgt = entity_map.keys().find(|k| after.contains(&k.to_lowercase()));
                    if let (Some(src), Some(tgt)) = (src, tgt) {
                        if let (Some(s), Some(t)) = (entity_map.get(src), entity_map.get(tgt)) {
                            let rtype = if *keyword == "depends on" {
                                RelationType::DependsOn
                            } else if *keyword == "contains" {
                                RelationType::Contains
                            } else {
                                RelationType::Uses
                            };
                            relations.push(Relation::new(s.id, t.id, rtype, "extractor"));
                        }
                    }
                }
            }
        }

        Ok(relations)
    }
}

// ── InMemoryGraphStore ──

#[derive(Default)]
struct InMemoryState {
    entities: HashMap<Uuid, Entity>,
    relations: Vec<Relation>,
}

#[derive(Default)]
pub struct InMemoryGraphStore {
    state: RwLock<InMemoryState>,
}

impl InMemoryGraphStore {
    fn read(&self) -> SemanticResult<std::sync::RwLockReadGuard<'_, InMemoryState>> {
        self.state
            .read()
            .map_err(|_| SemanticError::Internal("store lock poisoned".into()))
    }

    fn write(&self) -> SemanticResult<std::sync::RwLockWriteGuard<'_, InMemoryState>> {
        self.state
            .write()
            .map_err(|_| SemanticError::Internal("store lock poisoned".into()))
    }
}

#[async_trait]
impl GraphStore for InMemoryGraphStore {
    async fn save_entity(&self, entity: &Entity, actor: &str) -> SemanticResult<()> {
        if actor.trim().is_empty() {
            return Err(SemanticError::Validation("actor must not be empty".into()));
        }
        entity.validate()?;
        let mut state = self.write()?;
        state.entities.insert(entity.id, entity.clone());
        Ok(())
    }

    async fn find_entity(&self, id: Uuid) -> SemanticResult<Option<Entity>> {
        Ok(self.read()?.entities.get(&id).cloned())
    }

    async fn search_entities(&self, name_query: &str) -> SemanticResult<Vec<Entity>> {
        let needle = name_query.to_lowercase();
        let mut results: Vec<Entity> = self
            .read()?
            .entities
            .values()
            .filter(|e| e.name.to_lowercase().contains(&needle))
            .cloned()
            .collect();
        results.sort_by_key(|e| (e.name.clone(), e.id));
        Ok(results)
    }

    async fn list_entities(&self) -> SemanticResult<Vec<Entity>> {
        let mut entities: Vec<Entity> = self.read()?.entities.values().cloned().collect();
        entities.sort_by_key(|e| (e.name.clone(), e.id));
        Ok(entities)
    }

    async fn save_relation(&self, relation: &Relation, actor: &str) -> SemanticResult<()> {
        if actor.trim().is_empty() {
            return Err(SemanticError::Validation("actor must not be empty".into()));
        }
        relation.validate()?;
        let mut state = self.write()?;
        state.relations.push(relation.clone());
        Ok(())
    }

    async fn find_relations(&self, entity_id: Uuid) -> SemanticResult<Vec<Relation>> {
        Ok(self
            .read()?
            .relations
            .iter()
            .filter(|r| r.source_entity_id == entity_id || r.target_entity_id == entity_id)
            .cloned()
            .collect())
    }

    async fn list_relations(&self) -> SemanticResult<Vec<Relation>> {
        Ok(self.read()?.relations.clone())
    }

    async fn load_graph(&self) -> SemanticResult<SemanticGraph> {
        let state = self.read()?;
        Ok(SemanticGraph {
            entities: state.entities.clone(),
            relations: state.relations.clone(),
        })
    }
}