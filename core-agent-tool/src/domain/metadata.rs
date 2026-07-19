use std::collections::BTreeMap;

use crate::error::{ToolError, ToolRuntimeResult};

const AUDIT_KEYS: &[&str] = &[
    "trace_id",
    "correlation_id",
    "tenant_id",
    "user_id",
    "subject_id",
    "project_id",
    "workspace_id",
    "session_id",
    "conversation_id",
    "task_id",
    "purpose",
    "environment",
    "region",
];

pub(crate) fn validate_metadata(
    metadata: &BTreeMap<String, String>,
    context: &str,
) -> ToolRuntimeResult<()> {
    if metadata.len() > 64 {
        return Err(ToolError::InvalidArgument(format!(
            "{context} must contain at most 64 entries"
        )));
    }
    if let Some(key) = metadata.keys().find(|key| is_sensitive_key(key)) {
        return Err(ToolError::InvalidArgument(format!(
            "{context} must not contain sensitive key {key}"
        )));
    }
    if metadata
        .iter()
        .any(|(key, value)| key.trim().is_empty() || key.len() > 128 || value.len() > 4096)
    {
        return Err(ToolError::InvalidArgument(format!(
            "{context} contains an empty/oversized key or value"
        )));
    }
    Ok(())
}

fn is_sensitive_key(key: &str) -> bool {
    let segments = key
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|segment| !segment.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    if segments.iter().any(|segment| {
        matches!(
            segment.as_str(),
            "password" | "passwd" | "authorization" | "cookie" | "secret" | "credential"
        )
    }) {
        return true;
    }
    let normalized: String = key
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    [
        "apikey",
        "accesstoken",
        "refreshtoken",
        "password",
        "passwd",
        "authorization",
        "proxyauthorization",
        "cookie",
        "setcookie",
        "privatekey",
        "clientsecret",
        "credential",
        "secret",
    ]
    .iter()
    .any(|candidate| normalized == *candidate || normalized.ends_with(candidate))
}

pub(crate) fn audit_metadata(metadata: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    metadata
        .iter()
        .filter(|(key, _)| AUDIT_KEYS.contains(&key.as_str()))
        .map(|(key, value)| {
            let clean: String = value
                .chars()
                .filter(|ch| !ch.is_control())
                .take(256)
                .collect();
            (key.clone(), clean)
        })
        .collect()
}

pub(crate) fn validate_audit_metadata(
    metadata: &BTreeMap<String, String>,
) -> ToolRuntimeResult<()> {
    validate_metadata(metadata, "execution metadata")?;
    if metadata
        .keys()
        .any(|key| !AUDIT_KEYS.contains(&key.as_str()))
    {
        return Err(ToolError::InvalidArgument(
            "execution metadata contains a non-allowlisted key".into(),
        ));
    }
    if metadata
        .values()
        .any(|value| value.len() > 256 || value.chars().any(|character| character.is_control()))
    {
        return Err(ToolError::InvalidArgument(
            "execution metadata values must be at most 256 non-control characters".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_keys_are_rejected_without_keyboard_false_positive() {
        let safe = BTreeMap::from([("keyboard_layout".into(), "us".into())]);
        assert!(validate_metadata(&safe, "metadata").is_ok());
        let secret = BTreeMap::from([("proxy_authorization".into(), "secret".into())]);
        assert!(validate_metadata(&secret, "metadata").is_err());
        let nested = BTreeMap::from([("db_password_hash".into(), "secret".into())]);
        assert!(validate_metadata(&nested, "metadata").is_err());
    }

    #[test]
    fn audit_metadata_is_allowlisted_and_sanitized() {
        let metadata = BTreeMap::from([
            ("trace_id".into(), "a\n123".into()),
            ("custom".into(), "discard".into()),
        ]);
        assert_eq!(
            audit_metadata(&metadata),
            BTreeMap::from([("trace_id".into(), "a123".into())])
        );
    }
}
