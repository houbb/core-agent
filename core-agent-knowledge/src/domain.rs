use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::KnowledgeResult;

pub type KnowledgeMetadata = BTreeMap<String, Value>;

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
pub enum KnowledgeKind {
    Document,
    Code,
    Runtime,
    Business,
    Experience,
}
string_enum!(KnowledgeKind {
    Document => "DOCUMENT",
    Code => "CODE",
    Runtime => "RUNTIME",
    Business => "BUSINESS",
    Experience => "EXPERIENCE",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeSourceKind {
    Manual,
    Document,
    AgentLearning,
    SystemData,
    ExternalApi,
}
string_enum!(KnowledgeSourceKind {
    Manual => "MANUAL",
    Document => "DOCUMENT",
    AgentLearning => "AGENT_LEARNING",
    SystemData => "SYSTEM_DATA",
    ExternalApi => "EXTERNAL_API",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KnowledgeStatus {
    Created,
    Reviewing,
    Published,
    Deprecated,
    Archived,
}
string_enum!(KnowledgeStatus {
    Created => "CREATED",
    Reviewing => "REVIEWING",
    Published => "PUBLISHED",
    Deprecated => "DEPRECATED",
    Archived => "ARCHIVED",
});

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeItem {
    pub id: Uuid,
    pub kind: KnowledgeKind,
    pub title: String,
    pub content: String,
    pub source: KnowledgeSourceKind,
    pub confidence: f64,
    pub owner: String,
    pub tags: Vec<String>,
    pub version: u64,
    pub status: KnowledgeStatus,
    pub document_id: Option<Uuid>,
    pub metadata: KnowledgeMetadata,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl KnowledgeItem {
    pub fn new(
        kind: KnowledgeKind,
        title: impl Into<String>,
        content: impl Into<String>,
        source: KnowledgeSourceKind,
        owner: impl Into<String>,
        actor: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            kind,
            title: title.into(),
            content: content.into(),
            source,
            confidence: 0.8,
            owner: owner.into(),
            tags: Vec::new(),
            version: 1,
            status: KnowledgeStatus::Created,
            document_id: None,
            metadata: BTreeMap::new(),
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> KnowledgeResult<()> {
        if self.title.trim().is_empty() || self.title.len() > 1024 {
            return Err(crate::error::KnowledgeError::Validation(
                "knowledge title must be 1..=1024 bytes".into(),
            ));
        }
        if self.content.is_empty() {
            return Err(crate::error::KnowledgeError::Validation(
                "knowledge content must not be empty".into(),
            ));
        }
        if !(0.0..=1.0).contains(&self.confidence) {
            return Err(crate::error::KnowledgeError::Validation(
                "knowledge confidence must be 0..=1".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnowledgeCategory {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub description: String,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl KnowledgeCategory {
    pub fn new(name: impl Into<String>, actor: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            parent_id: None,
            description: String::new(),
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_item_validation_works() {
        let item = KnowledgeItem::new(
            KnowledgeKind::Document,
            "Payment Architecture",
            "The payment system uses a microservice architecture.",
            KnowledgeSourceKind::Manual,
            "owner",
            "tester",
        );
        assert!(item.validate().is_ok());
        assert_eq!(item.status, KnowledgeStatus::Created);
    }

    #[test]
    fn knowledge_item_rejects_empty_title() {
        let item = KnowledgeItem::new(
            KnowledgeKind::Document,
            "",
            "content",
            KnowledgeSourceKind::Manual,
            "owner",
            "tester",
        );
        assert!(item.validate().is_err());
    }
}