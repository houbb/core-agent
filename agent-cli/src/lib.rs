//! Official, Runtime-thin AgentOS terminal client.

mod app;
mod client;
mod command;
mod config;
mod domain;
mod embedded;
mod error;
mod http;
mod professional;
mod renderer;

pub use app::{CliApplication, CommandOutput};
pub use client::{AgentClient, EventStream, TerminalAgentClient};
pub use command::{Cli, CliCommand};
pub use config::{CliConfig, LocalSessionState, PermissionsConfig};
pub use domain::*;
pub use embedded::EmbeddedAgentClient;
pub use error::{CliError, CliResult};
pub use http::{HttpAgentClient, SseDecoder, SseFrame};
pub use professional::*;
pub use renderer::{Renderer, TerminalRenderer};
