//! chilli-mcp: Model Context Protocol (MCP) JSON-RPC client library.

pub mod client;
pub mod plugin_host;
pub mod protocol;
pub mod tool_bridge;

pub use client::{McpClient, McpClientError};
pub use plugin_host::{
    CapabilityBoundary, McpPluginHost, PluginCategory, PluginConfig, PluginHostError,
};
pub use protocol::*;
pub use tool_bridge::{McpToolAdapter, McpToolDefinition, McpToolRegistry};
