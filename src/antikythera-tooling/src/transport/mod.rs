//! MCP Transport Abstraction Layer (Port)
//!
//! This module defines the `McpTransport` port trait for MCP communication.
//! Concrete implementations live in `infrastructure::transport`.

// async_trait emits a message-less `#[must_use]` on async trait methods; the
// generated boxed future is itself `#[must_use]`, so clippy::double_must_use
// fires on every async method here. The attribute lives inside the macro
// expansion and cannot carry a message from this side.
#![allow(clippy::double_must_use)]

use async_trait::async_trait;
use serde_json::Value;

use super::error::ToolInvokeError;
use super::interface::ServerToolInfo;

/// Transport trait for MCP communication.
///
/// Implementations handle the low-level communication with MCP servers,
/// whether via STDIO (subprocess) or HTTP.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait McpTransport: Send + Sync {
    /// Connect to the server and perform initialization handshake.
    async fn connect(&self) -> Result<(), ToolInvokeError>;

    /// Send a JSON-RPC request and wait for response.
    async fn send_request(&self, method: &str, params: Value) -> Result<Value, ToolInvokeError>;

    /// Send a JSON-RPC notification (no response expected).
    async fn send_notification(&self, method: &str, params: Value) -> Result<(), ToolInvokeError>;

    /// Call a tool on the server.
    async fn call_tool(&self, tool: &str, arguments: Value) -> Result<Value, ToolInvokeError>;

    /// Get server instructions (from initialize response).
    async fn instructions(&self) -> Option<String>;

    /// Get tool metadata from cache.
    async fn tool_metadata(&self, tool: &str) -> Option<ServerToolInfo>;

    /// List all tools from cache.
    async fn list_tools(&self) -> Vec<ServerToolInfo>;

    /// Get server name.
    fn server_name(&self) -> &str;

    /// Check if the transport is connected.
    async fn is_connected(&self) -> bool;

    /// Disconnect from the server.
    async fn disconnect(&self);
}
