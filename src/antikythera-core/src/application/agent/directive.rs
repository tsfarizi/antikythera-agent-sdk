//! Agent directive types for parsing LLM responses.

use serde::Deserialize;
use serde_json::Value;

/// Directive extracted from LLM response.
///
/// The `Final` variant preserves the full JSON value returned by the model
/// rather than flattening it to a `String`.  Callers that need a plain text
/// response can call `Value::to_string` or pattern-match on [`Value::String`].
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentDirective {
    /// Final response to user.
    ///
    /// `response` is the raw `"response"` field value from the model, which may
    /// be a JSON object (e.g. `{"content": "...", "data": "step_0"}`), a plain
    /// string, or any other JSON value.  Preserving the full structure allows
    /// callers to extract typed fields without an extra round-trip parse.
    Final { response: Value },
    /// Call a single tool.
    CallTool { tool: String, input: Value },
    /// Call multiple tools in parallel.
    CallTools(Vec<(String, Value)>),
}
