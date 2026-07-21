/// MCP protocol version used by the client.
pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

/// Maximum bytes for MCP configuration file.
pub const MAX_MCP_CONFIG_BYTES: u64 = 64 * 1024;

/// Maximum number of MCP servers.
pub const MAX_MCP_SERVERS: usize = 32;

/// Maximum number of tools from a single MCP server.
pub const MAX_MCP_TOOLS: usize = 512;

/// Maximum bytes for a single MCP JSON-RPC message (1 MiB).
pub const MAX_MCP_MESSAGE_BYTES: usize = 1024 * 1024;

/// Default timeout for MCP requests.
pub const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 30_000;