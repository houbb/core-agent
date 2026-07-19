use std::fmt::{Display, Formatter};

use serde::{Deserialize, Serialize};

use crate::error::{ToolError, ToolRuntimeResult};

/// A normalized dotted capability path, for example `filesystem.read`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolCapability(String);

impl ToolCapability {
    pub fn new(value: impl AsRef<str>) -> ToolRuntimeResult<Self> {
        let normalized = value.as_ref().trim().to_ascii_lowercase();
        if normalized.is_empty()
            || normalized.split('.').any(|segment| {
                segment.is_empty()
                    || !segment
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
            })
        {
            return Err(ToolError::InvalidArgument(
                "capability must be a dotted path of ASCII identifiers".into(),
            ));
        }
        Ok(Self(normalized))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_same_or_descendant_of(&self, parent: &Self) -> bool {
        self == parent
            || self
                .0
                .strip_prefix(&parent.0)
                .is_some_and(|suffix| suffix.starts_with('.'))
    }
}

impl Display for ToolCapability {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_is_normalized_and_hierarchical() {
        let parent = ToolCapability::new("FileSystem").unwrap();
        let child = ToolCapability::new("filesystem.Read").unwrap();
        assert_eq!(child.as_str(), "filesystem.read");
        assert!(child.is_same_or_descendant_of(&parent));
        assert!(!parent.is_same_or_descendant_of(&child));
    }

    #[test]
    fn capability_rejects_empty_segments() {
        assert!(ToolCapability::new("filesystem..read").is_err());
    }
}
