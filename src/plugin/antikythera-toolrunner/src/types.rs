//! Core types for tool definitions, calls, and results.
//!
//! These types are intentionally compatible with MCP wire format and
//! the SDK's `ToolDefinition` / `ToolCall` / `ToolResult` types.

use serde::{Deserialize, Serialize};

/// A single parameter within a tool's input schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameterSchema {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
}

/// Definition of an MCP tool — its metadata, parameters, and schemas.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolDefinition {
    /// Tool name as exposed by MCP server.
    pub name: String,
    /// Human-readable title for display.
    #[serde(default)]
    pub title: Option<String>,
    /// Human-readable description shown to the LLM.
    pub description: String,
    /// Individual parameter schemas.
    #[serde(default)]
    pub parameters: Vec<ToolParameterSchema>,
    /// Full JSON Schema for the input object (takes precedence for validation).
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
    /// Optional JSON Schema for the output structure.
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
}

impl ToolDefinition {
    /// Names of required parameters derived from `input_schema` or `parameters`.
    pub fn required_params(&self) -> Vec<&str> {
        if let Some(schema) = &self.input_schema
            && let Some(required) = schema.get("required").and_then(|v| v.as_array())
        {
            return required.iter().filter_map(|v| v.as_str()).collect();
        }
        self.parameters
            .iter()
            .filter(|p| p.required)
            .map(|p| p.name.as_str())
            .collect()
    }

    /// Compact text line for LLM prompt injection.
    pub fn to_prompt_line(&self) -> String {
        let params: Vec<String> = if let Some(schema) = &self.input_schema {
            if let Some(props) = schema.get("properties").and_then(|v| v.as_object()) {
                props.keys().cloned().collect()
            } else {
                Vec::new()
            }
        } else {
            self.parameters.iter().map(|p| p.name.clone()).collect()
        };

        let required = self.required_params();
        let param_display: Vec<String> = params
            .iter()
            .map(|p| {
                if required.contains(&p.as_str()) {
                    format!("{}*", p)
                } else {
                    p.clone()
                }
            })
            .collect();

        if param_display.is_empty() {
            format!("- `{}`: {}", self.name, self.description)
        } else {
            format!(
                "- `{}` ({}): {}",
                self.name,
                param_display.join(", "),
                self.description
            )
        }
    }
}

/// A tool call request from the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
    #[serde(default)]
    pub step_id: u32,
}

/// A tool execution result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub name: String,
    pub success: bool,
    #[serde(default)]
    pub output: serde_json::Value,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub step_id: u32,
}
