use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{PluginError, PluginResult};

/// Plugin manifest — analogous to package.json for VS Code extensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub permissions: BTreeSet<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl PluginManifest {
    pub fn from_yaml(value: &str) -> PluginResult<Self> {
        let manifest: Self = serde_yaml::from_str(value)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn from_json(value: &str) -> PluginResult<Self> {
        let manifest: Self = serde_json::from_str(value)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> PluginResult<()> {
        if self.name.trim().is_empty() || self.name.len() > 128 {
            return Err(PluginError::Validation("plugin name is invalid".into()));
        }
        if self.version.trim().is_empty() || self.version.len() > 64 {
            return Err(PluginError::Validation("plugin version is invalid".into()));
        }
        if self.author.trim().is_empty() || self.author.len() > 256 {
            return Err(PluginError::Validation("plugin author is invalid".into()));
        }
        if self.tools.len() > 256 {
            return Err(PluginError::Validation("too many tools".into()));
        }
        if self.skills.len() > 256 {
            return Err(PluginError::Validation("too many skills".into()));
        }
        if self.agents.len() > 64 {
            return Err(PluginError::Validation("too many agents".into()));
        }
        for tool in &self.tools {
            if tool.trim().is_empty() || tool.len() > 128 {
                return Err(PluginError::Validation("invalid tool key".into()));
            }
        }
        for skill in &self.skills {
            if skill.trim().is_empty() || skill.len() > 128 {
                return Err(PluginError::Validation("invalid skill key".into()));
            }
        }
        for agent in &self.agents {
            if agent.trim().is_empty() || agent.len() > 128 {
                return Err(PluginError::Validation("invalid agent key".into()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PluginState {
    Installed,
    Enabled,
    Disabled,
    Uninstalled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plugin {
    pub id: Uuid,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub manifest: PluginManifest,
    pub state: PluginState,
    pub source_path: String,
    pub checksum_sha256: String,
    pub version_count: u64,
    pub actor: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Plugin {
    pub fn install(
        manifest: PluginManifest,
        source_path: impl Into<String>,
        checksum: impl Into<String>,
        actor: impl Into<String>,
    ) -> PluginResult<Self> {
        manifest.validate()?;
        let now = Utc::now();
        Ok(Self {
            id: Uuid::new_v4(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            author: manifest.author.clone(),
            manifest,
            state: PluginState::Installed,
            source_path: source_path.into(),
            checksum_sha256: checksum.into(),
            version_count: 1,
            actor: actor.into(),
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate(&self) -> PluginResult<()> {
        self.manifest.validate()?;
        if self.version_count == 0 || self.updated_at < self.created_at {
            return Err(PluginError::Validation(
                "plugin version or timestamps are invalid".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_yaml() {
        let yaml = r#"
name: rca-assistant
version: 1.0.0
description: RCA investigation assistant
author: core-agent
tools:
  - log.query
  - metric.query
skills:
  - database-slow-query-analysis
"#;
        let manifest = PluginManifest::from_yaml(yaml).unwrap();
        assert_eq!(manifest.name, "rca-assistant");
        assert_eq!(manifest.tools.len(), 2);
        assert_eq!(manifest.skills.len(), 1);
    }

    #[test]
    fn manifest_rejects_empty_name() {
        let manifest = PluginManifest {
            name: String::new(),
            version: "1.0.0".into(),
            description: "test".into(),
            author: "me".into(),
            tools: Vec::new(),
            skills: Vec::new(),
            agents: Vec::new(),
            permissions: BTreeSet::new(),
            metadata: BTreeMap::new(),
        };
        assert!(manifest.validate().is_err());
    }
}