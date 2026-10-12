//! # logic-core-template
//!
//! Starter crate for a host-authored drop-in `runner` component
//! (`antikythera:agent-sdk/runner`, world `logic-core-component`).
//!
//! The exported `runner` interface is IDENTICAL to the composite SDK's
//! (world `antikythera-agent-sdk`): the same 16 functions, the same
//! JSON-string semantics (payload contract in
//! `contracts/shared/payload_contract.golden.json`), so a host that loads
//! the composite SDK loads this component with zero host code changes.
//!
//! ## How to author a logic core
//!
//! Edit ONLY the `custom_*` functions below. The WIT export adapter in
//! `component.rs` (compiled with the `component` feature) is not touched.
//!
//! Every `custom_*` hook defaults to `None`, which means "use the template
//! default for this runner function". The adapter maps each function as
//! follows:
//!
//! | runner function | custom hook | `None` → template default |
//! |---|---|---|
//! | init | [`custom_init`] | create/reuse the in-memory session, return the session id |
//! | prepare-user-turn | [`custom_prepare_turn`] | `Err` not-implemented |
//! | commit-llm-response | [`custom_commit_response`] | `Err` not-implemented |
//! | commit-llm-stream | [`custom_commit_stream`] | `Err` not-implemented |
//! | process-llm-response-for-session | [`custom_process_llm_response`] | `Err` not-implemented |
//! | process-tool-result-for-session | [`custom_process_tool_result`] | `Err` not-implemented |
//! | append-llm-chunk | [`custom_append_chunk`] | `Err` not-implemented |
//! | drain-events | [`custom_drain_events`] | `Err` not-implemented |
//! | get-state | — (fixed store plumbing) | in-memory state JSON, or `Session not found: <id>` |
//! | reset-session | — (fixed store plumbing) | drop the session, `Ok(existed)` |
//! | sweep-idle-sessions | [`custom_sweep_idle`] | `Err` not-implemented |
//! | register-tools | [`custom_register_tools`] | `Err` not-implemented |
//! | get-tools-prompt | [`custom_get_tools_prompt`] | `Err` not-implemented |
//! | set-context-policy | [`custom_set_context_policy`] | `Err` not-implemented |
//! | get-telemetry-snapshot | [`custom_get_telemetry`] | `Err` not-implemented |
//! | get-slo-snapshot | [`custom_get_slo`] | `Err` not-implemented |
//!
//! The structured not-implemented error is the single consistent format
//! `{"error":"not implemented","function":"<kebab-case-name>"}`, produced by
//! the adapter in `component.rs` whenever a `custom_*` hook is `None` and no
//! template default exists. Host code can detect "template hole"
//! deterministically by checking for the `"error"` field.
//!
//! The template ships with exactly the minimal in-memory state the task
//! requires (`init` / `get-state` / `reset-session`); every other runner
//! function is a hook the host author fills in.
//!
//! Build the component:
//!
//! ```text
//! cargo build -p logic-core-template --release --target wasm32-wasip2 --no-default-features --features component
//! ```
//!
//! The artifact exports `antikythera:agent-sdk/runner@1.0.0` (16 functions).
//! The world declares optional `host-imports` and `tool-registry` imports.
//! The `host_*` helpers below bind the `host-imports` interface; the import
//! appears in the artifact only when a `custom_*` hook actually calls one of
//! them (the component encoder prunes unreferenced imports).

/// Custom `init` behavior.
///
/// An implementation parses configuration, creates a session, and returns its identifier.
/// `None` = template default: parse `config-json` leniently, create/reuse an
/// in-memory session, and return the session id — the same return contract as
/// the SDK runner (a bare id string, not a JSON object).
pub fn custom_init(_config_json: &str) -> Option<String> {
    None
}

/// Custom `prepare-user-turn` behavior.
///
/// An implementation assembles the session messages, system prompt, and tool catalogue.
/// `None` = `Err` not-implemented (`{"error":"not implemented","function":"prepare-user-turn"}`).
pub fn custom_prepare_turn(_request_json: &str) -> Option<String> {
    None
}

/// Custom `commit-llm-response` behavior.
///
/// An implementation processes the model response and returns a CommitResult envelope
/// with action, content, tool_name, tool_input, session_id, step, and fsm_state fields.
/// `None` = `Err` not-implemented.
pub fn custom_commit_response(
    _prepared_turn_json: &str,
    _llm_response_json: &str,
) -> Option<String> {
    None
}

/// Custom `commit-llm-stream` behavior.
///
/// An implementation joins buffered session chunks and commits the complete response.
/// `None` = `Err` not-implemented.
pub fn custom_commit_stream(_prepared_turn_json: &str) -> Option<String> {
    None
}

/// Custom `process-llm-response-for-session` behavior.
///
/// An implementation returns a serialized AgentAction:
/// `{"action":"call_tool",...}` | `{"action":"final","response":...}` | `{"action":"retry","error":...}`).
/// `None` = `Err` not-implemented.
pub fn custom_process_llm_response(_session_id: &str, _llm_response_json: &str) -> Option<String> {
    None
}

/// Custom `process-tool-result-for-session` behavior.
///
/// An implementation consumes ToolResultInput fields tool_name, success, output_json,
/// error_message, and correlation_id, then returns session_id, step, next_message,
/// and tool_result in the result envelope.
/// `None` = `Err` not-implemented.
pub fn custom_process_tool_result(_session_id: &str, _tool_result_json: &str) -> Option<String> {
    None
}

/// Custom `append-llm-chunk` behavior.
///
/// An implementation buffers streaming chunks for the specified session.
/// `None` = `Err` not-implemented.
pub fn custom_append_chunk(
    _session_id: &str,
    _chunk: &str,
    _correlation_id: Option<&str>,
) -> Option<bool> {
    None
}

/// Custom `drain-events` behavior.
///
/// An implementation returns a JSON array of StreamEvent records:
/// `{seq, session_id, step, correlation_id, kind, payload}`).
/// `None` = `Err` not-implemented.
pub fn custom_drain_events(_session_id: &str) -> Option<String> {
    None
}

/// Custom `sweep-idle-sessions` behavior.
///
/// An implementation removes idle sessions and returns the number removed.
/// `None` = `Err` not-implemented.
pub fn custom_sweep_idle(_now_unix_ms: Option<i64>) -> Option<u32> {
    None
}

/// Custom `register-tools` behavior.
///
/// An implementation validates ToolDefinition records, stores the catalogue,
/// and returns the number of registered tools.
/// `None` = `Err` not-implemented.
pub fn custom_register_tools(_tools_json: &str) -> Option<u32> {
    None
}

/// Custom `get-tools-prompt` behavior.
///
/// An implementation formats the registered tools for inclusion in the system prompt.
/// `None` = `Err` not-implemented.
pub fn custom_get_tools_prompt() -> Option<String> {
    None
}

/// Custom `set-context-policy` behavior.
///
/// An implementation applies the context policy in a `{"policy": {...}}` envelope.
/// `None` = `Err` not-implemented.
pub fn custom_set_context_policy(_policy_json: &str) -> Option<bool> {
    None
}

/// Custom `get-telemetry-snapshot` behavior.
///
/// An implementation returns a TelemetrySnapshot JSON object containing:
/// session_id/correlation_id/counters/total_prepare_latency_ms/total_commit_latency_ms/fsm_state).
/// `None` = `Err` not-implemented.
pub fn custom_get_telemetry(_session_id: &str) -> Option<String> {
    None
}

/// Custom `get-slo-snapshot` behavior.
///
/// An implementation returns a SloSnapshot JSON object containing:
/// session_id/correlation_id/success_rate/tool_error_rate/retry_ratio/p95_*).
/// `None` = `Err` not-implemented.
pub fn custom_get_slo(_session_id: &str) -> Option<String> {
    None
}

/// Call the host LLM service (`host-imports.call-llm`).
///
/// `request_json` mirrors the WIT `llm-request` record with snake_case keys:
/// `provider`, `model`, `session_id`, `messages_json` (the messages payload;
/// missing defaults to `[]`), `force_json`, `temperature`, `max_tokens`,
/// `schema_name`, `metadata_json`. Returns the `llm-response` record
/// serialized to JSON (snake_case keys).
#[cfg(feature = "component")]
/// Host-imports plumbing (component build only) The `host_*` helpers bind the `host-imports` WIT
/// interface (`crate::component::antikythera::agent_sdk::host_imports`): call-llm, save-state,
/// load-state, emit-tool-call, log-message. A host-imports call is the ONLY escape hatch a logic
/// core has to the outside world — the host grants permission by wiring the import; there is no
/// other way out of the component. The helpers are compiled only in the `component` build, so a
/// native build never references the wit-bindgen import shims (which abort via `unreachable!()`
/// on non-wasm targets). JSON keys follow the generated Rust field names (snake_case), matching
/// the JSON-string convention used across the runner surface.
///
pub fn host_call_llm(request_json: &str) -> Result<String, String> {
    use crate::component::antikythera::agent_sdk::host_imports::call_llm;
    use crate::component::antikythera::agent_sdk::vocabulary::LlmRequest;

    let request: serde_json::Value = serde_json::from_str(request_json)
        .map_err(|e| format!("host_call_llm: invalid request-json: {e}"))?;

    let llm_request = LlmRequest {
        provider: request
            .get("provider")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        model: request
            .get("model")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        session_id: request
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        messages_json: request
            .get("messages_json")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("[]")
            .to_string(),
        force_json: request
            .get("force_json")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        temperature: request
            .get("temperature")
            .and_then(serde_json::Value::as_f64)
            .map(|f| f as f32),
        max_tokens: request
            .get("max_tokens")
            .and_then(serde_json::Value::as_u64)
            .map(|u| u as u32),
        schema_name: request
            .get("schema_name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        metadata_json: request
            .get("metadata_json")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    };

    let response = call_llm(&llm_request).map_err(|e| format!("host_call_llm: {e}"))?;

    serde_json::to_string(&serde_json::json!({
        "content": response.content,
        "model": response.model,
        "session_id": response.session_id,
        "message_json": response.message_json,
        "tokens_used": response.tokens_used,
        "finish_reason": response.finish_reason,
        "raw_response_json": response.raw_response_json,
    }))
    .map_err(|e| format!("host_call_llm: cannot encode llm-response: {e}"))
}

/// Persist state with the host (`host-imports.save-state`).
#[cfg(feature = "component")]
pub fn host_save_state(context_id: &str, state_json: &str) -> Result<(), String> {
    use crate::component::antikythera::agent_sdk::host_imports::save_state;
    save_state(context_id, state_json).map_err(|e| format!("host_save_state: {e}"))
}

/// Load state previously saved by the host (`host-imports.load-state`).
///
/// `Ok(None)` means no state exists under `context_id`.
#[cfg(feature = "component")]
pub fn host_load_state(context_id: &str) -> Result<Option<String>, String> {
    use crate::component::antikythera::agent_sdk::host_imports::load_state;
    load_state(context_id).map_err(|e| format!("host_load_state: {e}"))
}

/// Ask the host to execute a tool (`host-imports.emit-tool-call`).
///
/// Returns the `tool-execution-result` record serialized to JSON
/// (`tool_name`, `success`, `output_json`, `error_message`, `step_id`).
#[cfg(feature = "component")]
/// `session-id` is optional in WIT; an empty host string maps to `None`.
///
pub fn host_emit_tool_call(
    tool_name: &str,
    arguments_json: &str,
    session_id: &str,
    step_id: u32,
) -> Result<String, String> {
    use crate::component::antikythera::agent_sdk::host_imports::emit_tool_call;
    use crate::component::antikythera::agent_sdk::vocabulary::ToolCallEvent;

    let event = ToolCallEvent {
        tool_name: tool_name.to_string(),
        arguments_json: arguments_json.to_string(),
        session_id: (!session_id.is_empty()).then(|| session_id.to_string()),
        step_id,
    };

    let result = emit_tool_call(&event).map_err(|e| format!("host_emit_tool_call: {e}"))?;

    serde_json::to_string(&serde_json::json!({
        "tool_name": result.tool_name,
        "success": result.success,
        "output_json": result.output_json,
        "error_message": result.error_message,
        "step_id": result.step_id,
    }))
    .map_err(|e| format!("host_emit_tool_call: cannot encode tool-execution-result: {e}"))
}

/// Send a log line to the host (`host-imports.log-message`).
///
/// `level` is one of `"debug" | "info" | "warn" | "error"`; `timestamp` is
/// left `None` (the host stamps the event).
#[cfg(feature = "component")]
pub fn host_log(level: &str, message: &str) {
    use crate::component::antikythera::agent_sdk::host_imports::log_message;
    use crate::component::antikythera::agent_sdk::vocabulary::LogEvent;

    let event = LogEvent {
        level: level.to_string(),
        message: message.to_string(),
        timestamp: None,
    };
    log_message(&event);
}

/// WIT export layer for the component world (`logic-core-component`).
#[cfg(feature = "component")]
pub mod component;
