use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::domain::validate_key;
use crate::{KernelError, KernelResult};

const MAX_CONFIG_BYTES: usize = 256 * 1024;
const MAX_CONFIG_ITEMS: usize = 256;

pub type KernelConfig = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub runtime_id: String,
    pub revision: u64,
    pub values: KernelConfig,
}

impl ConfigSnapshot {
    pub fn empty(runtime_id: impl Into<String>) -> Self {
        Self {
            runtime_id: runtime_id.into(),
            revision: 1,
            values: BTreeMap::new(),
        }
    }

    pub fn validate(&self) -> KernelResult<()> {
        validate_key("configuration runtime id", &self.runtime_id)?;
        if self.revision == 0 || self.values.len() > MAX_CONFIG_ITEMS {
            return Err(KernelError::Validation(
                "configuration revision or item count is invalid".into(),
            ));
        }
        let value = serde_json::to_value(&self.values)
            .map_err(|error| KernelError::Validation(error.to_string()))?;
        reject_sensitive(&value, 0)?;
        if serde_json::to_vec(&value)
            .map_err(|error| KernelError::Validation(error.to_string()))?
            .len()
            > MAX_CONFIG_BYTES
        {
            return Err(KernelError::Validation(
                "configuration exceeds 256 KiB".into(),
            ));
        }
        Ok(())
    }
}

fn reject_sensitive(value: &Value, depth: usize) -> KernelResult<()> {
    if depth > 32 {
        return Err(KernelError::Validation(
            "configuration nesting exceeds 32".into(),
        ));
    }
    match value {
        Value::Object(values) => {
            for (key, value) in values {
                let normalized = key.to_ascii_lowercase().replace('-', "_");
                if matches!(
                    normalized.as_str(),
                    "password"
                        | "secret"
                        | "api_key"
                        | "access_token"
                        | "refresh_token"
                        | "private_key"
                ) || normalized.ends_with("_secret")
                    || normalized.ends_with("_password")
                {
                    return Err(KernelError::Validation(
                        "configuration contains sensitive material".into(),
                    ));
                }
                reject_sensitive(value, depth + 1)?;
            }
        }
        Value::Array(values) => {
            for value in values {
                reject_sensitive(value, depth + 1)?;
            }
        }
        Value::String(value) if value.chars().any(char::is_control) => {
            return Err(KernelError::Validation(
                "configuration contains control characters".into(),
            ));
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_rejects_nested_secret() {
        let mut snapshot = ConfigSnapshot::empty("tool");
        snapshot
            .values
            .insert("provider".into(), serde_json::json!({"api_key":"x"}));
        assert!(snapshot.validate().is_err());
    }
}
