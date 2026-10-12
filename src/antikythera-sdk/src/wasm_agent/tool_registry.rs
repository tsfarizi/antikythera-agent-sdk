//! WASM-side tool registry and tool call/result types.
//!
//! These types intentionally mirror `antikythera_core::domain::entities` tool
//! types and `antikythera_core::application::tooling::interface` metadata types
//! but are defined separately because:
//! 1. WASM components cannot share Rust types across the FFI boundary
//! 2. The WASM `ToolCall` and `ToolResult` include a `step_id` field needed for
//!    the streaming protocol that native agents don't require
//! 3. The WASM tool metadata types use `#[serde(default)]` for JSON round-trip
//!    stability, while core types use `#[serde(skip_serializing_if)]` for
//!    compact wire format
//!
//! Conversion between core and WASM types is provided via `From` implementations
//! when the `sdk-core` feature is enabled.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool call record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool name
    pub name: String,
    /// Tool arguments
    pub arguments: serde_json::Value,
    /// Step ID
    pub step_id: u32,
}

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Tool name
    pub name: String,
    /// Success status
    pub success: bool,
    /// Output
    pub output: serde_json::Value,
    /// Error message
    pub error: Option<String>,
    /// Step ID
    pub step_id: u32,
}

/// Icon metadata for a tool, as defined by MCP spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
/// Tool Registry (MCP tool definitions for WASM-side validation).
///
pub struct ToolIcon {
    pub src: String,
    #[serde(default)]
    pub mime_type: Option<String>,
    #[serde(default)]
    pub sizes: Option<Vec<String>>,
}

/// Annotations providing metadata about tool audience, priority, and modification.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolAnnotations {
    #[serde(default)]
    pub audience: Option<Vec<String>>,
    #[serde(default)]
    pub priority: Option<f64>,
    #[serde(default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolExecution {
    #[serde(default)]
    pub task_support: Option<TaskSupport>,
}

/// A single parameter definition within a tool's input schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameterSchema {
    /// Parameter name
    pub name: String,
    /// JSON Schema type string (e.g. "string", "number", "object", "array")
    pub param_type: String,
    /// Human-readable description
    pub description: String,
    /// Whether this parameter is required
    pub required: bool,
}

/// Definition of a single MCP tool, pushed from host to WASM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Tool name as exposed by MCP server
    pub name: String,
    /// Human-readable title for display
    #[serde(default)]
    pub title: Option<String>,
    /// Human-readable description shown to the LLM
    pub description: String,
    /// Individual parameter schemas (may be empty if raw input_schema is used)
    #[serde(default)]
    pub parameters: Vec<ToolParameterSchema>,
    /// Full JSON Schema for the input object (optional; takes precedence for validation)
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
    /// Optional JSON Schema for the output structure
    #[serde(default)]
    pub output_schema: Option<serde_json::Value>,
    /// Optional annotations for audience, priority, etc.
    #[serde(default)]
    pub annotations: Option<ToolAnnotations>,
    /// Optional execution properties (task support)
    #[serde(default)]
    pub execution: Option<ToolExecution>,
}

impl ToolDefinition {
    /// Returns names of required parameters derived from `input_schema` or `parameters`.
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

    /// Renders a compact text line for LLM prompt injection.
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

/// Error from validating a tool call against the registry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolValidationError {
    /// Tool name not found in registry (registry must be non-empty to trigger this)
    UnknownTool { name: String },
    /// A required parameter is absent from the call arguments
    MissingRequiredParam { tool: String, param: String },
}

impl std::fmt::Display for ToolValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownTool { name } => write!(f, "Unknown tool: '{}'", name),
            Self::MissingRequiredParam { tool, param } => {
                write!(f, "Tool '{}': missing required param '{}'", tool, param)
            }
        }
    }
}

/// WASM-side registry of tools available from the MCP server.
///
/// Populated by the host via `register_tools()` before or during session init.
/// When non-empty, all `CallTool` actions are validated against this registry.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    /// Register a tool definition.
    pub fn register(&mut self, tool: ToolDefinition) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Returns `true` if the registry has been populated with at least one tool.
    pub fn is_populated(&self) -> bool {
        !self.tools.is_empty()
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// Validate a tool call: checks unknown tool and missing required params.
    pub fn validate_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<(), ToolValidationError> {
        let def = self
            .tools
            .get(tool_name)
            .ok_or_else(|| ToolValidationError::UnknownTool {
                name: tool_name.to_string(),
            })?;

        for param in def.required_params() {
            let present = arguments
                .as_object()
                .map(|obj| obj.contains_key(param))
                .unwrap_or(false);
            if !present {
                return Err(ToolValidationError::MissingRequiredParam {
                    tool: tool_name.to_string(),
                    param: param.to_string(),
                });
            }
        }

        Ok(())
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns `true` if there are no registered tools.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Returns names of all registered tools, sorted for determinism.
    pub fn tool_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.tools.keys().map(|s| s.as_str()).collect();
        names.sort_unstable();
        names
    }

    /// Renders a compact tool list block for injection into a system prompt.
    ///
    /// Returns `None` when the registry is empty.
    pub fn to_prompt_block(&self) -> Option<String> {
        if self.tools.is_empty() {
            return None;
        }
        let mut lines = vec!["Available tools (* = required param):".to_string()];
        let mut sorted: Vec<&ToolDefinition> = self.tools.values().collect();
        sorted.sort_by_key(|t| t.name.as_str());
        for tool in sorted {
            lines.push(tool.to_prompt_line());
        }
        Some(lines.join("\n"))
    }

    /// Load from a JSON array of `ToolDefinition`.
    pub fn from_json(json: &str) -> Result<Self, String> {
        let defs: Vec<ToolDefinition> =
            serde_json::from_str(json).map_err(|e| format!("Invalid tools JSON: {e}"))?;
        let mut registry = Self::default();
        for def in defs {
            registry.register(def);
        }
        Ok(registry)
    }

    /// Serialize the full registry to a JSON array, sorted by name.
    pub fn to_json(&self) -> Result<String, String> {
        let mut tools: Vec<&ToolDefinition> = self.tools.values().collect();
        tools.sort_by_key(|t| t.name.as_str());
        serde_json::to_string(&tools).map_err(|e| format!("Serialize error: {e}"))
    }
}

#[cfg(feature = "component")]
/// Conversions from core domain types.
///
impl From<antikythera_core::domain::entities::ToolCall> for ToolCall {
    fn from(core_call: antikythera_core::domain::entities::ToolCall) -> Self {
        let arguments =
            serde_json::to_value(&core_call.arguments).unwrap_or(serde_json::Value::Null);
        Self {
            name: core_call.name,
            arguments,
            step_id: 0,
        }
    }
}

#[cfg(feature = "component")]
impl From<antikythera_core::domain::entities::ToolResult> for ToolResult {
    fn from(core_result: antikythera_core::domain::entities::ToolResult) -> Self {
        let output = serde_json::to_value(&core_result.output).unwrap_or(serde_json::Value::Null);
        Self {
            name: core_result.name,
            success: core_result.success,
            output,
            error: core_result.error,
            step_id: 0,
        }
    }
}

#[cfg(feature = "component")]
impl From<antikythera_core::application::tooling::ToolIcon> for ToolIcon {
    fn from(core_icon: antikythera_core::application::tooling::ToolIcon) -> Self {
        Self {
            src: core_icon.src,
            mime_type: core_icon.mime_type,
            sizes: core_icon.sizes,
        }
    }
}

#[cfg(feature = "component")]
impl From<ToolIcon> for antikythera_core::application::tooling::ToolIcon {
    fn from(sdk_icon: ToolIcon) -> Self {
        Self {
            src: sdk_icon.src,
            mime_type: sdk_icon.mime_type,
            sizes: sdk_icon.sizes,
        }
    }
}

#[cfg(feature = "component")]
impl From<antikythera_core::application::tooling::ToolAnnotations> for ToolAnnotations {
    fn from(core_ann: antikythera_core::application::tooling::ToolAnnotations) -> Self {
        Self {
            audience: core_ann.audience,
            priority: core_ann.priority,
            last_modified: core_ann.last_modified,
        }
    }
}

#[cfg(feature = "component")]
impl From<ToolAnnotations> for antikythera_core::application::tooling::ToolAnnotations {
    fn from(sdk_ann: ToolAnnotations) -> Self {
        Self {
            audience: sdk_ann.audience,
            priority: sdk_ann.priority,
            last_modified: sdk_ann.last_modified,
        }
    }
}

#[cfg(feature = "component")]
impl From<antikythera_core::application::tooling::TaskSupport> for TaskSupport {
    fn from(core_ts: antikythera_core::application::tooling::TaskSupport) -> Self {
        match core_ts {
            antikythera_core::application::tooling::TaskSupport::Forbidden => {
                TaskSupport::Forbidden
            }
            antikythera_core::application::tooling::TaskSupport::Optional => TaskSupport::Optional,
            antikythera_core::application::tooling::TaskSupport::Required => TaskSupport::Required,
        }
    }
}

#[cfg(feature = "component")]
impl From<TaskSupport> for antikythera_core::application::tooling::TaskSupport {
    fn from(sdk_ts: TaskSupport) -> Self {
        match sdk_ts {
            TaskSupport::Forbidden => {
                antikythera_core::application::tooling::TaskSupport::Forbidden
            }
            TaskSupport::Optional => antikythera_core::application::tooling::TaskSupport::Optional,
            TaskSupport::Required => antikythera_core::application::tooling::TaskSupport::Required,
        }
    }
}

#[cfg(feature = "component")]
impl From<antikythera_core::application::tooling::ToolExecution> for ToolExecution {
    fn from(core_exec: antikythera_core::application::tooling::ToolExecution) -> Self {
        Self {
            task_support: core_exec.task_support.map(TaskSupport::from),
        }
    }
}

#[cfg(feature = "component")]
impl From<ToolExecution> for antikythera_core::application::tooling::ToolExecution {
    fn from(sdk_exec: ToolExecution) -> Self {
        Self {
            task_support: sdk_exec
                .task_support
                .map(antikythera_core::application::tooling::TaskSupport::from),
        }
    }
}
