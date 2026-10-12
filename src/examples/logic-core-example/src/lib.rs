//! # logic-core-example
//!
//! Deterministic drop-in logic core probe: a filled-in copy of
//! `logic-core-template` proving that a host loads the SDK composite OR this
//! core with ZERO host code changes (world `logic-core-component`, export
//! `antikythera:agent-sdk/runner`).
//!
//! The exported `runner` interface is IDENTICAL to the composite SDK's
//! (the same 16 functions, the same JSON-string semantics in
//! `tests/rust/fixtures/contracts/payload_contract.golden.json`), so the swap proof runs
//! one host script against both artifacts.
//!
//! ## Probe semantics (echo-agent, no LLM)
//!
//! | runner function | custom hook | this crate's behavior |
//! |---|---|---|
//! | init | [`custom_init`] | `None` → template default: lenient config parse, create/reuse the in-memory session, return the bare session id |
//! | prepare-user-turn | [`custom_prepare_turn`] | builds the `PreparedTurn` envelope with `messages_json` = `[system, user]` from the request |
//! | commit-llm-response | [`custom_commit_response`] | always commits the deterministic final envelope `echo-agent-done` for the session id carried by the prepared turn |
//! | every other runner function | all remaining `custom_*` hooks | `None` → `Err` not-implemented (template default) |
//!
//! The prepared turn and commit result are PURE functions of their inputs —
//! deterministic, no LLM, no host imports. The SDK composite, by contrast,
//! implements real prepare/commit semantics (FSM transitions, message
//! history). The swap proof demonstrates contract identity, not behavior
//! identity: same host code, same API, different behavior on purpose.
//!
//! Build the component:
//!
//! ```text
//! cargo component build -p logic-core-example --release --target wasm32-wasip2 --no-default-features --features component
//! ```
//!
//! The artifact exports `antikythera:agent-sdk/runner@1.0.0` (16 functions).

/// Custom `init` behavior — deliberately delegates to the template default.
///
/// Returning `None` reuses `default_init` in `component.rs`: it parses
/// `config-json` leniently, stores an `AgentState`-shaped record in the
/// in-memory store under the session id (reuse-or-create), and returns the
/// bare id string — the same return contract as the SDK runner.
///
/// A `Some(id)` return would bypass the store entirely (the adapter maps
/// `Some` straight to `Ok(id)`), so `get-state` on that id would fail with
/// `Session not found`; the deterministic session creation therefore MUST
/// flow through the template default.
pub fn custom_init(_config_json: &str) -> Option<String> {
    None
}

/// Custom `prepare-user-turn` behavior.
///
/// Deterministic prepared-turn builder (echo-agent): parses the request
/// envelope (same `PrepareUserTurnInput` shape the SDK accepts — `prompt`
/// required, `session_id` / `system_prompt` / `force_json` / `metadata_json`
/// / `correlation_id` optional) and returns the `PreparedTurn` JSON:
///
/// ```json
/// {
///   "correlation_id": "...",
///   "force_json": false,
///   "messages_json": "[{\"role\":\"system\",...},{\"role\":\"user\",...}]",
///   "metadata_json": "...",
///   "prompt": "...",
///   "session_id": "...",
///   "step": 0,
///   "summary_handoff": null,
///   "system_prompt": "..."
/// }
/// ```
///
/// Field set matches `tests/rust/fixtures/contracts/payload_contract.golden.json` and
/// `antikythera-sdk/src/wasm_agent/runner/runner_types.rs::PreparedTurn`.
/// `None` would be the template default (`Err` not-implemented).
pub fn custom_prepare_turn(request_json: &str) -> Option<String> {
    let request: serde_json::Value = serde_json::from_str(request_json).ok()?;

    let prompt = request
        .get("prompt")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let system_prompt = request
        .get("system_prompt")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("You are a deterministic echo-agent.")
        .to_string();
    let session_id = request
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("echo-session")
        .to_string();
    let force_json = request
        .get("force_json")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let metadata_json = request
        .get("metadata_json")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let correlation_id = request
        .get("correlation_id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    let messages_json = serde_json::json!([
        { "role": "system", "content": system_prompt },
        { "role": "user", "content": prompt },
    ])
    .to_string();

    let prepared = serde_json::json!({
        "session_id": session_id,
        "step": 0,
        "prompt": prompt,
        "system_prompt": system_prompt,
        "force_json": force_json,
        "metadata_json": metadata_json,
        "correlation_id": correlation_id,
        "summary_handoff": null,
        "messages_json": messages_json,
    });

    Some(prepared.to_string())
}

/// Custom `commit-llm-response` behavior.
///
/// Deterministic final commit (echo-agent): ignores the LLM response and
/// returns the fixed `CommitResult` envelope for the session id carried by
/// the prepared turn:
///
/// ```json
/// { "action": "final", "content": "echo-agent-done",
///   "session_id": "<id>", "step": 0, "fsm_state": "final" }
/// ```
///
/// `action`/`content`/`session_id`/`step`/`fsm_state` are the subset of
/// `CommitResult` keys the swap-proof asserts on; the SDK composite's
/// commit envelope carries the same keys (`tool_name`/`tool_input` are
/// `null` there too for a final action). `None` would be the template
/// default (`Err` not-implemented).
pub fn custom_commit_response(
    prepared_turn_json: &str,
    _llm_response_json: &str,
) -> Option<String> {
    let prepared: serde_json::Value = serde_json::from_str(prepared_turn_json).ok()?;
    let session_id = prepared
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("echo-session")
        .to_string();

    let commit = serde_json::json!({
        "action": "final",
        "content": "echo-agent-done",
        "session_id": session_id,
        "step": 0,
        "fsm_state": "final",
    });

    Some(commit.to_string())
}

/// Custom `commit-llm-stream` behavior.
///
/// `None` = `Err` not-implemented.
pub fn custom_commit_stream(_prepared_turn_json: &str) -> Option<String> {
    None
}

/// Custom `process-llm-response-for-session` behavior.
///
/// `None` = `Err` not-implemented.
pub fn custom_process_llm_response(_session_id: &str, _llm_response_json: &str) -> Option<String> {
    None
}

/// Custom `process-tool-result-for-session` behavior.
///
/// `None` = `Err` not-implemented.
pub fn custom_process_tool_result(_session_id: &str, _tool_result_json: &str) -> Option<String> {
    None
}

/// Custom `append-llm-chunk` behavior.
///
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
/// `None` = `Err` not-implemented.
pub fn custom_drain_events(_session_id: &str) -> Option<String> {
    None
}

/// Custom `sweep-idle-sessions` behavior.
///
/// `None` = `Err` not-implemented.
pub fn custom_sweep_idle(_now_unix_ms: Option<i64>) -> Option<u32> {
    None
}

/// Custom `register-tools` behavior.
///
/// `None` = `Err` not-implemented.
pub fn custom_register_tools(_tools_json: &str) -> Option<u32> {
    None
}

/// Custom `get-tools-prompt` behavior.
///
/// `None` = `Err` not-implemented.
pub fn custom_get_tools_prompt() -> Option<String> {
    None
}

/// Custom `set-context-policy` behavior.
///
/// `None` = `Err` not-implemented.
pub fn custom_set_context_policy(_policy_json: &str) -> Option<bool> {
    None
}

/// Custom `get-telemetry-snapshot` behavior.
///
/// `None` = `Err` not-implemented.
pub fn custom_get_telemetry(_session_id: &str) -> Option<String> {
    None
}

/// Custom `get-slo-snapshot` behavior.
///
/// `None` = `Err` not-implemented.
pub fn custom_get_slo(_session_id: &str) -> Option<String> {
    None
}

/// WIT export layer for the component world (`logic-core-component`).
#[cfg(feature = "component")]
pub mod component;
