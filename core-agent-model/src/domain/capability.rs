use serde::{Deserialize, Serialize};

/// Capabilities declared by a Model Profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelCapability {
    Chat,
    Streaming,
    Embedding,
    Vision,
    ToolCall,
    Thinking,
    Image,
    Audio,
}

impl ModelCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "CHAT",
            Self::Streaming => "STREAMING",
            Self::Embedding => "EMBEDDING",
            Self::Vision => "VISION",
            Self::ToolCall => "TOOL_CALL",
            Self::Thinking => "THINKING",
            Self::Image => "IMAGE",
            Self::Audio => "AUDIO",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_has_stable_name() {
        assert_eq!(ModelCapability::ToolCall.as_str(), "TOOL_CALL");
    }
}
