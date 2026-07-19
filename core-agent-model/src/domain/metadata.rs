use std::collections::BTreeMap;

use crate::error::{ModelError, ModelResult};

/// Audit metadata is content-free. Secret-looking keys are rejected at every
/// persistence entry point instead of relying on best-effort redaction.
pub(crate) fn validate_metadata(
    metadata: &BTreeMap<String, String>,
    field: &str,
) -> ModelResult<()> {
    if let Some(key) = metadata.keys().find(|key| is_sensitive_key(key)) {
        return Err(ModelError::InvalidArgument(format!(
            "{field} must not contain sensitive key: {key}"
        )));
    }
    Ok(())
}

pub(crate) fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase().replace('-', "_");
    let compact = normalized.replace('_', "");
    matches!(
        normalized.as_str(),
        "authorization"
            | "proxy_authorization"
            | "api_key"
            | "access_key"
            | "secret_key"
            | "access_token"
            | "refresh_token"
            | "client_secret"
            | "password"
            | "passwd"
            | "credential"
            | "credentials"
            | "cookie"
            | "set_cookie"
            | "private_key"
            | "bearer"
            | "authentication"
    ) || normalized.ends_with("_password")
        || normalized.ends_with("_secret")
        || normalized.ends_with("_token")
        || normalized.ends_with("_api_key")
        || matches!(compact.as_str(), "apikey" | "privatekey" | "accesstoken")
}

const AUDIT_KEYS: &[&str] = &[
    "trace_id",
    "correlation_id",
    "tenant_id",
    "user_id",
    "project_id",
    "workspace_id",
    "session_id",
    "conversation_id",
    "task_id",
    "purpose",
    "environment",
    "region",
];

pub(crate) fn audit_metadata(metadata: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    metadata
        .iter()
        .filter(|(key, _)| AUDIT_KEYS.contains(&key.as_str()))
        .map(|(key, value)| {
            (
                key.clone(),
                value
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(256)
                    .collect(),
            )
        })
        .collect()
}

pub(crate) fn validate_audit_metadata(metadata: &BTreeMap<String, String>) -> ModelResult<()> {
    validate_metadata(metadata, "usage metadata")?;
    if let Some(key) = metadata
        .keys()
        .find(|key| !AUDIT_KEYS.contains(&key.as_str()))
    {
        return Err(ModelError::InvalidArgument(format!(
            "usage metadata key is not allowlisted: {key}"
        )));
    }
    if metadata
        .values()
        .any(|value| value.chars().count() > 256 || value.chars().any(char::is_control))
    {
        return Err(ModelError::InvalidArgument(
            "usage metadata values must be at most 256 non-control characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_secret_keys_without_false_positive_for_keyboard() {
        assert!(is_sensitive_key("provider_api_key"));
        assert!(is_sensitive_key("apikey"));
        assert!(is_sensitive_key("proxy_authorization"));
        assert!(is_sensitive_key("Authorization"));
        assert!(!is_sensitive_key("keyboard_layout"));
    }
}
