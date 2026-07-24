use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("skill I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid skill limit: {0}")]
    InvalidLimit(String),
    #[error("skill not found: {0}")]
    SkillNotFound(String),
    #[error("invalid skill {path}: {reason}")]
    InvalidSkill { path: String, reason: String },
    #[error("duplicate skill {name} at same precedence: {first} and {second}")]
    DuplicateSkill { name: String, first: String, second: String },
    #[error("skill changed after discovery: {0}")]
    SkillChanged(String),
    #[error("skill exceeds limit {kind}: {limit}")]
    LimitExceeded { kind: String, limit: usize },
    #[error("skill is not UTF-8: {0}")]
    InvalidUtf8(String),
    #[error("skill validation error: {0}")]
    Validation(String),
    #[error("tool not found in skill: {0}")]
    ToolNotFound(String),
    #[error("skill resolution failed: {0}")]
    ResolutionFailed(String),
    #[error("skill serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("skill YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

pub type SkillResult<T> = Result<T, SkillError>;