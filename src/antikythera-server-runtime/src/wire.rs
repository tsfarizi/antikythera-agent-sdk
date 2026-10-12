//! Wire shapes for the Runtime Bridge protocol.
//!
//! Every shape is serialized exactly as the golden contract
//! (`contracts/shared/wire_protocol.golden.json`) spells it. Field names are
//! per-shape: `llm-request`/`llm-response` are snake_case on the wire, the
//! tool shapes are kebab-case, the envelope mixes `type` with snake_case
//! correlation/session/client ids. No field may be added beyond the golden
//! file (acceptance invariant 5).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// `llm-request` vocabulary record.
#[derive(Debug, Clone, Serialize, Deserialize)]
/// Llm-request / llm-response (wire names are snake_case, matching the golden).
///
pub struct LlmRequest {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub session_id: Option<String>,
    pub messages_json: String,
    pub force_json: bool,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub schema_name: Option<String>,
    pub metadata_json: Option<String>,
}

/// `llm-response` vocabulary record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub model: Option<String>,
    pub session_id: Option<String>,
    pub message_json: Option<String>,
    pub tokens_used: Option<u32>,
    pub finish_reason: Option<String>,
    pub raw_response_json: Option<String>,
}

/// `tool-call-event` vocabulary record.
#[derive(Debug, Clone, Serialize, Deserialize)]
/// Tool-call-event / tool-execution-result (wire names are kebab-case).
///
pub struct ToolCallEvent {
    #[serde(rename = "tool-name")]
    pub tool_name: String,
    #[serde(rename = "arguments-json")]
    pub arguments_json: String,
    #[serde(rename = "session-id")]
    pub session_id: Option<String>,
    #[serde(rename = "step-id")]
    pub step_id: u32,
}

/// `tool-execution-result` vocabulary record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    #[serde(rename = "tool-name")]
    pub tool_name: String,
    pub success: bool,
    #[serde(rename = "output-json")]
    pub output_json: String,
    #[serde(rename = "error-message")]
    pub error_message: Option<String>,
    #[serde(rename = "step-id")]
    pub step_id: u32,
}

/// `log-event` vocabulary record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    pub level: String,
    pub message: String,
    pub timestamp: Option<String>,
}

/// SSE event envelope (`type` + snake_case ids + opaque `payload`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope {
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(rename = "correlation_id")]
    pub correlation_id: Option<String>,
    #[serde(rename = "session_id")]
    pub session_id: Option<String>,
    #[serde(rename = "client_id")]
    pub client_id: String,
    pub payload: Value,
}

/// POST-back body for `POST /antikythera/v1/events/{correlation-id}/response`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostbackBody {
    #[serde(rename = "correlation_id")]
    pub correlation_id: String,
    pub ok: bool,
    pub payload: Value,
    pub error: Option<String>,
}

/// `hook-request` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
/// Event payload shapes (snake_case, per golden).
///
pub struct HookRequestPayload {
    pub hook: String,
    #[serde(rename = "session_state_json")]
    pub session_state_json: String,
    #[serde(rename = "input_json")]
    pub input_json: String,
}

/// `llm-token` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTokenPayload {
    #[serde(rename = "session_id")]
    pub session_id: Option<String>,
    pub chunk: String,
    #[serde(rename = "correlation_id")]
    pub correlation_id: Option<String>,
}

/// `lifecycle` payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecyclePayload {
    pub signal: String,
}

/// Canonical tool definition used by `GET /tools` and `register-tools`.
#[derive(Debug, Clone, Serialize, Deserialize)]
/// ToolDefinition (registry shape, snake_case per golden).
///
pub struct ToolDefinition {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
}

impl ToolDefinition {
    /// Build a minimal definition with an empty object input schema.
    pub fn simple(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: None,
            description: description.into(),
            parameters: Vec::new(),
            input_schema: Some(json!({"type": "object", "properties": {}, "required": []})),
            output_schema: None,
        }
    }
}

/// Maps a wire `tool-execution-result` into the runner `ToolResultInput`
/// JSON shape: `{tool_name, success, output_json, error_message?,
/// correlation_id?}`. `step_id` is dropped (the runner derives it from
/// session state); `output_json` is required and forwarded verbatim.
/// Mapping wire → runner contract (WIRE_PROTOCOL §6).
///
pub fn tool_execution_result_to_runner_input(
    result: &ToolExecutionResult,
    correlation_id: Option<String>,
) -> Value {
    json!({
        "tool_name": result.tool_name,
        "success": result.success,
        "output_json": result.output_json,
        "error_message": result.error_message,
        "correlation_id": correlation_id,
    })
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-server-runtime/wire.tests.rs"]
mod tests;
