//! MCP Runtime — external capability connection protocol (Model Context Protocol).
//!
//! Provides:
//! - `McpClient` — stdio-based JSON-RPC transport for MCP servers
//! - `McpToolProvider` — discover remote tools and wrap them as [`ToolProvider`]
//! - `McpServerConfig` — typed configuration for each MCP server
//! - `discover_mcp_servers` — layered config discovery (global + project)
//! - Permission and security controls

mod client;
mod config;
mod constants;
mod error;
mod provider;

pub use client::McpClient;
pub use config::{discover_mcp_servers, McpServerConfig};
pub use error::{McpRuntimeError, McpRuntimeResult};
pub use provider::McpToolProvider;