// async_trait emits a message-less `#[must_use]` on async trait methods; the
// generated boxed future is itself `#[must_use]`, so clippy::double_must_use
// fires on every async method here. The attribute lives inside the macro
// expansion and cannot carry a message from this side.
#![allow(clippy::double_must_use)]

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::error::ToolInvokeError;

/// MCP protocol version used during `initialize` handshake.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

/// Icon metadata for a tool, as defined by MCP spec.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolIcon {
    pub src: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sizes: Option<Vec<String>>,
}

/// Annotations providing metadata about tool audience, priority, and modification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ToolAnnotations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
}

/// Task execution support level as defined by MCP spec.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskSupport {
    Forbidden,
    Optional,
    Required,
}

/// Execution-related properties for a tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ToolExecution {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_support: Option<TaskSupport>,
}

/// Metadata about a tool discovered from an MCP server's `tools/list` response.
#[derive(Debug, Clone)]
pub struct ServerToolInfo {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub icons: Option<Vec<ToolIcon>>,
    pub input_schema: Option<Value>,
    pub output_schema: Option<Value>,
    pub annotations: Option<ToolAnnotations>,
    pub execution: Option<ToolExecution>,
}

#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait ToolServerInterface: Send + Sync {
    async fn invoke_tool(
        &self,
        server: &str,
        tool: &str,
        arguments: Value,
    ) -> Result<Value, ToolInvokeError>;

    async fn server_instructions(&self, server: &str) -> Option<String>;

    async fn tool_metadata(&self, server: &str, tool: &str) -> Option<ServerToolInfo>;
}
