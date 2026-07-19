use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::domain::{validate_metadata, WorkspaceMetadata};
use crate::error::{WorkspaceError, WorkspaceResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GraphNodeKind {
    Workspace,
    Project,
    Environment,
    Resource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GraphRelation {
    Contains,
    DetectedIn,
    BelongsTo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub kind: GraphNodeKind,
    pub label: String,
    pub uri: Option<String>,
    pub metadata: WorkspaceMetadata,
}

impl GraphNode {
    pub fn new(
        id: impl Into<String>,
        kind: GraphNodeKind,
        label: impl Into<String>,
        uri: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            label: label.into(),
            uri,
            metadata: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub relation: GraphRelation,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

impl WorkspaceGraph {
    pub fn validate(&self) -> WorkspaceResult<()> {
        let mut ids = BTreeSet::new();
        for node in &self.nodes {
            if node.id.trim().is_empty() || node.label.trim().is_empty() || !ids.insert(&node.id) {
                return Err(WorkspaceError::Validation(
                    "graph nodes need unique non-empty IDs and labels".into(),
                ));
            }
            validate_metadata(&node.metadata)?;
        }
        if self
            .edges
            .iter()
            .any(|edge| !ids.contains(&edge.source) || !ids.contains(&edge.target))
        {
            return Err(WorkspaceError::Validation(
                "graph edge references an unknown node".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSearchHit {
    pub node_id: String,
    pub kind: GraphNodeKind,
    pub label: String,
    pub uri: Option<String>,
    pub score: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_rejects_dangling_edges() {
        let graph = WorkspaceGraph {
            nodes: vec![GraphNode::new(
                "workspace:1",
                GraphNodeKind::Workspace,
                "one",
                None,
            )],
            edges: vec![GraphEdge {
                source: "workspace:1".into(),
                target: "resource:missing".into(),
                relation: GraphRelation::Contains,
            }],
        };
        assert!(graph.validate().is_err());
    }
}
