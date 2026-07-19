use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{KernelError, KernelResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RuntimeVersion {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl RuntimeVersion {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub fn satisfies(self, minimum: Self) -> bool {
        self.major == minimum.major && self >= minimum
    }
}

impl Display for RuntimeVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for RuntimeVersion {
    type Err = KernelError;

    fn from_str(value: &str) -> KernelResult<Self> {
        let parts = value.split('.').collect::<Vec<_>>();
        if parts.len() != 3 {
            return Err(KernelError::Validation(
                "runtime version must use major.minor.patch".into(),
            ));
        }
        let parse = |part: &str| {
            part.parse::<u64>()
                .map_err(|_| KernelError::Validation("runtime version is invalid".into()))
        };
        Ok(Self::new(
            parse(parts[0])?,
            parse(parts[1])?,
            parse(parts[2])?,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDependency {
    pub runtime_id: String,
    pub minimum_version: RuntimeVersion,
    pub optional: bool,
}

impl RuntimeDependency {
    pub fn required(runtime_id: impl Into<String>, minimum_version: RuntimeVersion) -> Self {
        Self {
            runtime_id: runtime_id.into(),
            minimum_version,
            optional: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDescriptor {
    pub id: String,
    pub name: String,
    pub version: RuntimeVersion,
    pub dependencies: Vec<RuntimeDependency>,
    pub config_schema_version: u64,
}

impl RuntimeDescriptor {
    pub fn new(id: impl Into<String>, name: impl Into<String>, version: RuntimeVersion) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version,
            dependencies: Vec::new(),
            config_schema_version: 1,
        }
    }

    pub fn validate(&self) -> KernelResult<()> {
        validate_key("runtime id", &self.id)?;
        if self.name.trim().is_empty()
            || self.name.len() > 256
            || self.name.chars().any(char::is_control)
            || self.config_schema_version == 0
            || self.dependencies.len() > 256
        {
            return Err(KernelError::Validation(
                "runtime descriptor fields are invalid".into(),
            ));
        }
        let mut dependencies = BTreeSet::new();
        for dependency in &self.dependencies {
            validate_key("dependency runtime id", &dependency.runtime_id)?;
            if dependency.runtime_id == self.id || !dependencies.insert(&dependency.runtime_id) {
                return Err(KernelError::Validation(
                    "runtime dependencies must be unique and cannot reference self".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeStatus {
    Registered,
    Initialized,
    Running,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelStatus {
    Created,
    Running,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LifecycleOperation {
    Init,
    Start,
    Stop,
    Reload,
}

impl LifecycleOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Init => "INIT",
            Self::Start => "START",
            Self::Stop => "STOP",
            Self::Reload => "RELOAD",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LifecycleContext {
    pub runtime_id: String,
    pub operation: LifecycleOperation,
    pub config_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KernelEventKind {
    Registered,
    Initialized,
    Started,
    Stopped,
    Reloaded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelEvent {
    pub id: Uuid,
    pub runtime_id: String,
    pub kind: KernelEventKind,
    pub message: String,
    pub occurred_at: DateTime<Utc>,
}

impl KernelEvent {
    pub fn new(
        runtime_id: impl Into<String>,
        kind: KernelEventKind,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            runtime_id: runtime_id.into(),
            kind,
            message: message.into(),
            occurred_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeHealth {
    pub runtime_id: String,
    pub healthy: bool,
    pub message: String,
    pub checked_at: DateTime<Utc>,
}

impl RuntimeHealth {
    pub fn healthy(runtime_id: impl Into<String>) -> Self {
        Self {
            runtime_id: runtime_id.into(),
            healthy: true,
            message: "healthy".into(),
            checked_at: Utc::now(),
        }
    }
}

pub(crate) fn validate_key(label: &str, value: &str) -> KernelResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b':' | b'/')
        })
    {
        return Err(KernelError::Validation(format!(
            "{label} must be a safe bounded identifier"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatible_version_requires_same_major_and_minimum() {
        assert!(RuntimeVersion::new(1, 2, 0).satisfies(RuntimeVersion::new(1, 1, 9)));
        assert!(!RuntimeVersion::new(2, 0, 0).satisfies(RuntimeVersion::new(1, 1, 9)));
    }

    #[test]
    fn descriptor_rejects_self_dependency() {
        let mut descriptor = RuntimeDescriptor::new("tool", "Tool", RuntimeVersion::new(1, 0, 0));
        descriptor.dependencies.push(RuntimeDependency::required(
            "tool",
            RuntimeVersion::new(1, 0, 0),
        ));
        assert!(descriptor.validate().is_err());
    }
}
