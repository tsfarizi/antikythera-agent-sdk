//! Tool registry — catalog of tool definitions with validation and prompt generation.

use std::collections::HashMap;

use crate::error::ToolRunnerError;
use crate::types::ToolDefinition;

/// Registry of tool definitions.
///
/// Stores tool metadata, validates calls against definitions,
/// and generates LLM prompt blocks from registered tools.
#[derive(Debug, Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    /// Register a tool definition. Replaces any existing definition with the same name.
    pub fn register(&mut self, tool: ToolDefinition) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Returns `true` if no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Names of all registered tools, sorted for determinism.
    pub fn tool_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.tools.keys().map(|s| s.as_str()).collect();
        names.sort_unstable();
        names
    }

    /// Validate a tool call: checks unknown tool and missing required params.
    /// Empty registry = skip validation (tools not yet loaded).
    ///
    pub fn validate_call(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
    ) -> Result<(), ToolRunnerError> {
        if self.tools.is_empty() {
            return Ok(());
        }

        let def = self
            .tools
            .get(tool_name)
            .ok_or_else(|| ToolRunnerError::NotFound {
                name: tool_name.to_string(),
            })?;

        for param in def.required_params() {
            let present = arguments
                .as_object()
                .map(|obj| obj.contains_key(param))
                .unwrap_or(false);
            if !present {
                return Err(ToolRunnerError::MissingParam {
                    tool: tool_name.to_string(),
                    param: param.to_string(),
                });
            }
        }

        Ok(())
    }

    /// Render a compact tool list block for injection into a system prompt.
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
    pub fn from_json(json: &str) -> Result<Self, ToolRunnerError> {
        let defs: Vec<ToolDefinition> =
            serde_json::from_str(json).map_err(|e| ToolRunnerError::Registry(e.to_string()))?;
        let mut registry = Self::default();
        for def in defs {
            registry.register(def);
        }
        Ok(registry)
    }

    /// Serialize the full registry to a JSON array, sorted by name.
    pub fn to_json(&self) -> Result<String, ToolRunnerError> {
        let mut tools: Vec<&ToolDefinition> = self.tools.values().collect();
        tools.sort_by_key(|t| t.name.as_str());
        serde_json::to_string(&tools).map_err(|e| ToolRunnerError::Registry(e.to_string()))
    }
}
