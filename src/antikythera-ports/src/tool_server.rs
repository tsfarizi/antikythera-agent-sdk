//! Port: Tool Server
//!
//! Application defines these traits. Infrastructure (transport adapters) implements them.

// async_trait emits a message-less `#[must_use]` on async trait methods; the
// generated boxed future is itself `#[must_use]`, so clippy::double_must_use
// fires on every async method here. The attribute lives inside the macro
// expansion and cannot carry a message from this side.
#![allow(clippy::double_must_use)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// MCP protocol version used during `initialize` handshake.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// Annotations providing metadata about tool audience, priority, and modification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ToolAnnotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destructive_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idempotent_hint: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_world_hint: Option<bool>,
}

/// Execution-related properties for a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecution {
    pub call_id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub started_at: String,
}

/// Icon metadata for a tool, as defined by MCP spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolIcon {
    pub mime_type: String,
    pub data: String,
}

/// Metadata about a tool discovered from an MCP server's `tools/list` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub annotations: Option<ToolAnnotations>,
}

/// Task execution support level as defined by MCP spec.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskSupport {
    None,
    Single,
    Parallel,
}

/// Port trait for tool server interaction.
#[async_trait]
pub trait ToolServerInterface: Send + Sync {
    async fn list_tools(&self) -> Result<Vec<ServerToolInfo>, String>;
    async fn invoke_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    fn supports_tasks(&self) -> TaskSupport;
    fn protocol_version(&self) -> &str;
}
