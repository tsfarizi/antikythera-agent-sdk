//! Tool execution engine.
//!
//! `ToolRunner` combines a `ToolRegistry` (definitions) with a handler map
//! (builtin tools) to execute tool calls in-process.

use std::collections::HashMap;
use std::sync::Arc;

use crate::error::ToolRunnerError;
use crate::handler::{ToolHandler, ToolHandlerFn};
use crate::registry::ToolRegistry;
use crate::types::{ToolCall, ToolDefinition, ToolResult};

/// MCP tool runner — dispatches tool calls to builtin handlers.
///
/// ```text
/// execute("get_weather", args)
///   ├── registry has definition? → validate args
///   ├── handler registered?      → call handler(args)
///   └── no handler               → Err(HostRequired)
/// ```
pub struct ToolRunner {
    registry: ToolRegistry,
    handlers: HashMap<String, Arc<dyn ToolHandler>>,
}

impl ToolRunner {
    /// Create an empty tool runner.
    pub fn new() -> Self {
        Self {
            registry: ToolRegistry::default(),
            handlers: HashMap::new(),
        }
    }

    /// Register a tool definition (metadata only, no handler).
    pub fn register_tool(&mut self, tool: ToolDefinition) {
        self.registry.register(tool);
    }

    /// Register a builtin tool handler (function pointer).
    ///
    /// If no tool definition exists for this name, a minimal one is created.
    /// Auto-register a definition if one doesn't exist yet.
    ///
    pub fn register_handler(&mut self, name: &str, handler: ToolHandlerFn) {
        self.handlers.insert(name.to_string(), Arc::new(handler));

        if self.registry.get(name).is_none() {
            self.registry.register(ToolDefinition {
                name: name.to_string(),
                description: format!("Builtin tool: {}", name),
                ..Default::default()
            });
        }
    }

    /// Register a dynamic tool handler (trait object with captured state).
    pub fn register_handler_dyn(&mut self, name: &str, handler: Arc<dyn ToolHandler>) {
        self.handlers.insert(name.to_string(), handler);

        if self.registry.get(name).is_none() {
            self.registry.register(ToolDefinition {
                name: name.to_string(),
                description: format!("Builtin tool: {}", name),
                ..Default::default()
            });
        }
    }

    /// Load tool definitions from a JSON array (replaces existing definitions).
    pub fn load_definitions(&mut self, json: &str) -> Result<usize, ToolRunnerError> {
        let defs: Vec<ToolDefinition> =
            serde_json::from_str(json).map_err(|e| ToolRunnerError::Registry(e.to_string()))?;
        let count = defs.len();
        for def in defs {
            self.registry.register(def);
        }
        Ok(count)
    }

    /// Access the tool registry.
    pub fn registry(&self) -> &ToolRegistry {
        &self.registry
    }

    /// Check if a tool has a registered builtin handler.
    pub fn is_builtin(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// List all registered tool definitions.
    pub fn list_tools(&self) -> Vec<&ToolDefinition> {
        self.registry
            .tool_names()
            .iter()
            .filter_map(|name| self.registry.get(name))
            .collect()
    }

    /// Generate the tool prompt block for LLM system prompts.
    pub fn tools_prompt(&self) -> Option<String> {
        self.registry.to_prompt_block()
    }

    /// Execute a tool call.
    ///
    /// 1. Validates against registry (if populated).
    /// 2. Looks up builtin handler.
    /// 3. Calls handler and wraps result in `ToolResult`.
    pub fn execute(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<ToolResult, ToolRunnerError> {
        self.registry.validate_call(name, &arguments)?;

        let handler = self
            .handlers
            .get(name)
            .ok_or_else(|| ToolRunnerError::HostRequired {
                tool: name.to_string(),
            })?;

        match handler.call(&arguments) {
            Ok(output) => Ok(ToolResult {
                name: name.to_string(),
                success: true,
                output,
                error: None,
                step_id: 0,
            }),
            Err(message) => Ok(ToolResult {
                name: name.to_string(),
                success: false,
                output: serde_json::Value::Null,
                error: Some(message),
                step_id: 0,
            }),
        }
    }

    /// Execute a `ToolCall` and return a `ToolResult`.
    pub fn execute_call(&self, call: &ToolCall) -> Result<ToolResult, ToolRunnerError> {
        let mut result = self.execute(&call.name, call.arguments.clone())?;
        result.step_id = call.step_id;
        Ok(result)
    }

    /// Try to execute a tool call.
    ///
    /// Returns `Ok(Some(result))` if builtin, `Ok(None)` if host-required,
    /// or `Err` on validation failure.
    pub fn try_execute(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<Option<ToolResult>, ToolRunnerError> {
        if self.handlers.contains_key(name) {
            Ok(Some(self.execute(name, arguments)?))
        } else {
            Ok(None)
        }
    }
}

impl Default for ToolRunner {
    fn default() -> Self {
        Self::new()
    }
}
