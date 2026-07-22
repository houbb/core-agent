use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::error::{MarketplaceError, MarketplaceResult};

// ── Asset Type ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssetType {
    Agent,
    Skill,
    Plugin,
    Workflow,
    Template,
    Prompt,
    Mcp,
}

impl AssetType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "AGENT",
            Self::Skill => "SKILL",
            Self::Plugin => "PLUGIN",
            Self::Workflow => "WORKFLOW",
            Self::Template => "TEMPLATE",
            Self::Prompt => "PROMPT",
            Self::Mcp => "MCP",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "AGENT" => Some(Self::Agent),
            "SKILL" => Some(Self::Skill),
            "PLUGIN" => Some(Self::Plugin),
            "WORKFLOW" => Some(Self::Workflow),
            "TEMPLATE" => Some(Self::Template),
            "PROMPT" => Some(Self::Prompt),
            "MCP" => Some(Self::Mcp),
            _ => None,
        }
    }
}

// ── Package State ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PackageState {
    Draft,
    Published,
    Deprecated,
    Archived,
}

impl PackageState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Published => "PUBLISHED",
            Self::Deprecated => "DEPRECATED",
            Self::Archived => "ARCHIVED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "DRAFT" => Some(Self::Draft),
            "PUBLISHED" => Some(Self::Published),
            "DEPRECATED" => Some(Self::Deprecated),
            "ARCHIVED" => Some(Self::Archived),
            _ => None,
        }
    }
}

// ── Marketplace Package ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplacePackage {
    pub id: Uuid,
    pub asset_type: AssetType,
    pub name: String,
    pub key: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub state: PackageState,
    pub rating: f64,
    pub downloads: u64,
    pub tags: Vec<String>,
    pub content: Value,
    pub metadata: BTreeMap<String, String>,
    pub version_count: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MarketplacePackage {
    pub fn new(
        asset_type: AssetType,
        key: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        author: impl Into<String>,
        description: impl Into<String>,
        content: Value,
    ) -> MarketplaceResult<Self> {
        let key = key.into();
        let name = name.into();
        let version = version.into();
        let author = author.into();
        let description = description.into();

        if key.trim().is_empty() || key.len() > 256 {
            return Err(MarketplaceError::Validation(
                "key must contain 1..=256 characters".into(),
            ));
        }
        if name.trim().is_empty() || name.len() > 256 {
            return Err(MarketplaceError::Validation(
                "name must contain 1..=256 characters".into(),
            ));
        }
        if version.trim().is_empty() || version.len() > 64 {
            return Err(MarketplaceError::Validation(
                "version must contain 1..=64 characters".into(),
            ));
        }
        if author.trim().is_empty() || author.len() > 256 {
            return Err(MarketplaceError::Validation(
                "author must contain 1..=256 characters".into(),
            ));
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            asset_type,
            name,
            key,
            version,
            author: author.clone(),
            description,
            state: PackageState::Draft,
            rating: 0.0,
            downloads: 0,
            tags: Vec::new(),
            content,
            metadata: BTreeMap::new(),
            version_count: 1,
            actor: author.clone(),
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate(&self) -> MarketplaceResult<()> {
        if self.version_count == 0 || self.updated_at < self.created_at {
            return Err(MarketplaceError::Validation(
                "invalid version or timestamps".into(),
            ));
        }
        if !(0.0..=5.0).contains(&self.rating) {
            return Err(MarketplaceError::Validation(
                "rating must be 0.0..=5.0".into(),
            ));
        }
        Ok(())
    }
}

// ── Marketplace Query ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MarketplaceQuery {
    pub asset_type: Option<AssetType>,
    pub state: Option<PackageState>,
    pub author: Option<String>,
    pub tags: Option<Vec<String>>,
    pub rating_min: Option<f64>,
    pub search: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

impl Default for MarketplaceQuery {
    fn default() -> Self {
        Self {
            asset_type: None,
            state: None,
            author: None,
            tags: None,
            rating_min: None,
            search: None,
            limit: 100,
            offset: 0,
        }
    }
}

impl MarketplaceQuery {
    pub fn validate(&self) -> MarketplaceResult<()> {
        if self.limit == 0 || self.limit > 10000 {
            return Err(MarketplaceError::Validation(
                "limit must be within 1..=10000".into(),
            ));
        }
        Ok(())
    }
}

// ── Marketplace Snapshot ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceSnapshot {
    pub total_packages: u64,
    pub by_type: BTreeMap<String, u64>,
    pub by_state: BTreeMap<String, u64>,
    pub total_downloads: u64,
    pub avg_rating: f64,
}

// ── Validation ────────────────────────────────────────────────────────

pub(crate) fn validate_actor(value: &str) -> MarketplaceResult<()> {
    if value.trim().is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(MarketplaceError::Validation(
            "actor must contain 1..=256 safe characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_package() {
        let pkg = MarketplacePackage::new(
            AssetType::Skill,
            "redis-diagnosis",
            "Redis Diagnosis",
            "1.0.0",
            "core-agent",
            "Redis diagnosis skill",
            serde_json::json!({"steps": []}),
        )
        .unwrap();
        assert!(pkg.validate().is_ok());
    }

    #[test]
    fn empty_key_rejected() {
        assert!(
            MarketplacePackage::new(AssetType::Agent, "", "name", "1.0", "author", "desc", Value::Null)
                .is_err()
        );
    }

    #[test]
    fn asset_type_roundtrip() {
        for t in &[AssetType::Agent, AssetType::Skill, AssetType::Plugin] {
            assert_eq!(AssetType::parse(t.as_str()), Some(*t));
        }
    }

    #[test]
    fn rating_bounds() {
        let mut pkg = MarketplacePackage::new(
            AssetType::Skill,
            "test",
            "Test",
            "1.0",
            "author",
            "desc",
            Value::Null,
        )
        .unwrap();
        pkg.rating = 5.5;
        assert!(pkg.validate().is_err());
        pkg.rating = 4.5;
        assert!(pkg.validate().is_ok());
    }
}