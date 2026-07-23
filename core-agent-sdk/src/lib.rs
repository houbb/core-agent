//! Core-Agent SDK — build, test, and publish agents, tools, skills, and plugins.
//!
//! This is the official SDK for third-party developers to interact with the
//! Core-Agent ecosystem. It provides:
//!
//! - **AgentClient** — chat and task execution against any agent runtime
//! - **AgentBuilder** — declarative agent construction
//! - **AgentTool / AgentSkill / AgentPlugin** — trait definitions for custom components
//! - **AgentManifest / PluginManifest** — YAML/JSON manifest types
//! - **PublishClient** — publish agents to the marketplace

mod domain;
mod error;
mod infrastructure;

pub use domain::*;
pub use error::{SdkError, SdkResult};
pub use infrastructure::*;