//! # logic-core-host-example
//!
//! Host-llm-agent: a standalone logic core that runs a FULL CUSTOM LOOP
//! through the `host-imports` escape hatch (`call-llm`, `save-state` /
//! `load-state`, `emit-tool-call`, `log-message`). It is a filled-in sibling
//! of `logic-core-template` and `logic-core-example` (same world
//! `logic-core-component`, same exported `runner`), differing from the
//! echo-agent by reaching the HOST for the LLM and for tool execution instead
//! of computing everything deterministically inside the component.
//!
//! ## Activation proof
//!
//! The template's `custom_*` hooks are all `None`, so the component encoder
//! PRUNES the `host-imports` import: `wasm-tools component wit
//! <logic_core_template.wasm>` shows `export runner` + WASI imports only.
//! This crate's hooks actually CALL the `host_*` helpers, so the same
//! command on THIS artifact shows
//! `import antikythera:agent-sdk/host-imports@1.0.0;` — the import survives
//! pruning because a function references it. The `tool-registry` import stays
//! pruned (this core never touches the toolrunner).
//!
//! ## Loop semantics (host-llm-agent)
//!
//! | runner function | custom hook | behavior |
//! |---|---|---|
//! | init | [`custom_init`] | `host_load_state(config.session_id)` → reuse; absent → build the in-example session state (`session_id` + `fsm_state` + `step` + minimal `message_history`), `host_save_state`, `host_log("info", "session created")` |
//! | prepare-user-turn | [`custom_prepare_turn`] | builds the 9-key `PreparedTurn` envelope (system + user messages) like the echo-agent, `host_log("debug", "prepared turn")` |
//! | commit-llm-response | [`custom_commit_response`] | `host_call_llm` with the prepared messages → parse `content`; prompt containing `"tool"` → `action=call_tool` (`tool_name="echo"`, `tool_input={}`); otherwise `action=final` (`content=<llm content>`); `host_save_state`; returns the 7-key `CommitResult` envelope |
//! | process-tool-result-for-session | [`custom_process_tool_result`] | `host_emit_tool_call(tool_name, "{}", session_id, step)` → tool-execution-result JSON; `host_save_state`; returns a final `CommitResult` whose `content` summarizes the tool result |
//! | every other runner function | all remaining hooks | `None` → template default (`Err` not-implemented) |
//!
//! The session state is NOT the template's in-memory store: `custom_init`
//! returns `Some(id)`, which bypasses `default_init`, so `get-state` /
//! `reset-session` report `Session not found`. State lives in the HOST via
//! `save-state` / `load-state`; every hook re-reads it at entry and
//! re-persists at exit — the component itself is stateless between runner
//! calls.
//!
//! ## Host-failure policy
//!
//! A failed host call in `commit-llm-response` /
//! `process-tool-result-for-session` surfaces as the error envelope
//! `{"error":"<detail>","function":"<name>"}` (the same `"error"`-field
//! convention as the template's structured not-implemented error), so host
//! code detects the failure deterministically. `init` cannot express an
//! error (its return contract is a bare id), so a `load-state` failure there
//! degrades to a fresh session and is surfaced via `host_log("warn"|"error")`.
//!
//! The native rlib (no `component` feature) compiles the four active hooks as
//! `None` stubs: the wit-bindgen import shims abort on non-wasm targets, so
//! the loop only runs inside the wasm component.
//!
//! Build the component:
//!
//! ```text
//! cargo component build -p logic-core-host-example --release --target wasm32-wasip2 --no-default-features --features component
//! ```
//!
//! The artifact exports `antikythera:agent-sdk/runner@1.0.0` (16 functions)
//! and imports `antikythera:agent-sdk/host-imports@1.0.0` (activation proof).

/// Custom `init` behavior (host-llm-agent).
///
/// `host_load_state(config.session_id)`: `Ok(Some)` → reuse the persisted
/// session; `Ok(None)` → build the initial in-example session state
/// (`session_id` + `fsm_state:"created"` + `step:0` + empty
/// `message_history`), `host_save_state` it, `host_log("info",
/// "session created")`; `Err` → the host could not answer `load-state`, so
/// degrade to a fresh session (surfaced via `host_log`) because `init`'s
/// return contract is a bare id and cannot carry an error.
#[cfg(feature = "component")]
pub fn custom_init(config_json: &str) -> Option<String> {
    host_loop::init_session(config_json)
}

/// Native-build stub: the full loop requires the `component` escape hatch.
/// `None` falls back to the template default in-memory store.
#[cfg(not(feature = "component"))]
pub fn custom_init(_config_json: &str) -> Option<String> {
    None
}

/// Custom `prepare-user-turn` behavior (host-llm-agent).
///
/// Builds the `PreparedTurn` envelope (9 keys, `messages_json` =
/// `[system, user]` from the request, `step` re-read from the host-side
/// session state) and emits `host_log("debug", "prepared turn")`.
#[cfg(feature = "component")]
pub fn custom_prepare_turn(request_json: &str) -> Option<String> {
    host_loop::prepare_turn(request_json)
}

/// Native-build stub: `None` = `Err` not-implemented (template default).
#[cfg(not(feature = "component"))]
pub fn custom_prepare_turn(_request_json: &str) -> Option<String> {
    None
}

/// Custom `commit-llm-response` behavior (host-llm-agent).
///
/// Calls `host_call_llm` with the prepared turn's messages, parses `content`
/// from the returned `llm-response` JSON, then derives the action
/// deterministically: a prompt containing `"tool"` → `call_tool`
/// (`tool_name="echo"`, `tool_input={}`); anything else → `final`
/// (`content=<llm content>`). Persists the updated session state via
/// `host_save_state` and returns the full 7-key `CommitResult` envelope.
/// The `llm_response_json` parameter forwarded by the runner is ignored by
/// design: the host-imports call is the loop's source of truth.
#[cfg(feature = "component")]
pub fn custom_commit_response(
    prepared_turn_json: &str,
    _llm_response_json: &str,
) -> Option<String> {
    host_loop::commit_response(prepared_turn_json)
}

/// Native-build stub: `None` = `Err` not-implemented (template default).
#[cfg(not(feature = "component"))]
pub fn custom_commit_response(
    _prepared_turn_json: &str,
    _llm_response_json: &str,
) -> Option<String> {
    None
}

/// Custom `process-tool-result-for-session` behavior (host-llm-agent).
///
/// Parses the `ToolResultInput` envelope, asks the host to execute the tool
/// via `host_emit_tool_call(tool_name, "{}", session_id, step)`, persists the
/// updated session state via `host_save_state`, and returns a final
/// `CommitResult` whose `content` summarizes the tool execution result.
#[cfg(feature = "component")]
pub fn custom_process_tool_result(session_id: &str, tool_result_json: &str) -> Option<String> {
    host_loop::process_tool_result(session_id, tool_result_json)
}

/// Native-build stub: `None` = `Err` not-implemented (template default).
#[cfg(not(feature = "component"))]
pub fn custom_process_tool_result(_session_id: &str, _tool_result_json: &str) -> Option<String> {
    None
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

/// Full custom-loop implementation of the four active hooks.
#[cfg(feature = "component")]
/// Host-loop implementation (component build only) The four active hooks delegate here. Every
/// function re-reads the session state from the HOST at entry (`host_load_state`), transforms it,
/// and re-persists at exit (`host_save_state`): the component holds no session state between
/// runner calls. Host failures return the structured error envelope
/// `{"error":"<detail>","function":"<name>"}` so host code detects them deterministically.
///
mod host_loop {
    use super::{host_call_llm, host_emit_tool_call, host_load_state, host_log, host_save_state};

    /// Initial in-example session state (AgentState-ish, deliberately NOT the
    /// template's in-memory store so save/load through the host is meaningful).
    fn fresh_session_state(session_id: &str) -> String {
        serde_json::json!({
            "session_id": session_id,
            "fsm_state": "created",
            "step": 0,
            "message_history": [],
        })
        .to_string()
    }

    /// Re-read the session state from the host; `Ok(None)` becomes the fresh
    /// initial state. `Err` propagates the host failure.
    fn load_or_fresh(session_id: &str) -> Result<serde_json::Value, String> {
        match host_load_state(session_id)? {
            Some(state_json) => serde_json::from_str(&state_json)
                .map_err(|e| format!("cannot parse persisted session state: {e}")),
            None => serde_json::from_str(&fresh_session_state(session_id))
                .map_err(|e| format!("cannot parse fresh session state: {e}")),
        }
    }

    /// Structured failure envelope (same `"error"`-field convention as the
    /// template's not-implemented error).
    fn error_envelope(function: &str, detail: &str) -> String {
        serde_json::json!({ "error": detail, "function": function }).to_string()
    }

    pub fn init_session(config_json: &str) -> Option<String> {
        let config: serde_json::Value = serde_json::from_str(config_json).ok()?;

        let session_id = config
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| "host-llm-agent-session".to_string());

        match host_load_state(&session_id) {
            Ok(Some(_)) => {
                host_log("info", "session reused");
            }
            Ok(None) => match host_save_state(&session_id, &fresh_session_state(&session_id)) {
                Ok(()) => host_log("info", "session created"),
                Err(e) => host_log(
                    "error",
                    &format!("session created but save-state failed: {e}"),
                ),
            },
            Err(e) => {
                host_log(
                    "warn",
                    &format!("load-state failed, creating fresh session: {e}"),
                );
                if let Err(e2) = host_save_state(&session_id, &fresh_session_state(&session_id)) {
                    host_log(
                        "error",
                        &format!("session created but save-state failed: {e2}"),
                    );
                }
            }
        }

        Some(session_id)
    }

    /// Re-read the persisted step so the prepared turn reports the current step of the host-side
    /// session (fresh session → 0). A load failure here defaults to 0; real failures surface in
    /// the commit hook.
    ///
    pub fn prepare_turn(request_json: &str) -> Option<String> {
        let request: serde_json::Value = serde_json::from_str(request_json).ok()?;

        let prompt = request
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let system_prompt = request
            .get("system_prompt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("You are a deterministic host-llm-agent.")
            .to_string();
        let session_id = request
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("host-llm-agent-session")
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

        let step = load_or_fresh(&session_id)
            .map(|state| {
                state
                    .get("step")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0) as u32
            })
            .unwrap_or(0);

        let messages_json = serde_json::json!([
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": prompt },
        ])
        .to_string();

        let prepared = serde_json::json!({
            "session_id": session_id,
            "step": step,
            "prompt": prompt,
            "system_prompt": system_prompt,
            "force_json": force_json,
            "metadata_json": metadata_json,
            "correlation_id": correlation_id,
            "summary_handoff": null,
            "messages_json": messages_json,
        });

        host_log("debug", "prepared turn");
        Some(prepared.to_string())
    }

    /// Full custom loop: the LLM is reached through the host-imports escape hatch; the
    /// `llm_response_json` the runner forwards is ignored by design.
    ///
    /// Deterministic action derivation: prompt containing "tool" → call the echo tool; anything
    /// else → final with the LLM content.
    ///
    /// Persist: append the assistant message to the host-side session state.
    ///
    pub fn commit_response(prepared_turn_json: &str) -> Option<String> {
        let prepared: serde_json::Value = serde_json::from_str(prepared_turn_json).ok()?;

        let session_id = prepared
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("host-llm-agent-session")
            .to_string();
        let prompt = prepared
            .get("prompt")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let messages_json = prepared
            .get("messages_json")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("[]");
        let force_json = prepared
            .get("force_json")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let llm_request = serde_json::json!({
            "session_id": session_id,
            "messages_json": messages_json,
            "force_json": force_json,
        })
        .to_string();

        let llm_response_json = match host_call_llm(&llm_request) {
            Ok(json) => json,
            Err(e) => {
                return Some(error_envelope(
                    "commit-llm-response",
                    &format!("host_call_llm: {e}"),
                ));
            }
        };

        let llm: serde_json::Value = match serde_json::from_str(&llm_response_json) {
            Ok(value) => value,
            Err(e) => {
                return Some(error_envelope(
                    "commit-llm-response",
                    &format!("cannot parse llm-response: {e}"),
                ));
            }
        };
        let content = llm
            .get("content")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();

        let wants_tool = prompt.contains("tool");
        let (action, tool_name, tool_input, fsm_state) = if wants_tool {
            (
                "call_tool",
                Some("echo".to_string()),
                Some(serde_json::json!({})),
                "awaiting_tool",
            )
        } else {
            ("final", None, None, "final")
        };

        let state = match load_or_fresh(&session_id) {
            Ok(state) => state,
            Err(e) => {
                return Some(error_envelope(
                    "commit-llm-response",
                    &format!("load-state: {e}"),
                ));
            }
        };
        let step = state
            .get("step")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        let next_step = step + 1;

        let mut history = state
            .get("message_history")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        if let Some(array) = history.as_array_mut() {
            array.push(serde_json::json!({
                "role": "assistant",
                "content": if wants_tool {
                    format!("call_tool:{}", tool_name.as_deref().unwrap_or("echo"))
                } else {
                    content.to_string()
                },
            }));
        }

        let updated = serde_json::json!({
            "session_id": session_id,
            "fsm_state": fsm_state,
            "step": next_step,
            "message_history": history,
        });
        if let Err(e) = host_save_state(&session_id, &updated.to_string()) {
            host_log("error", &format!("save-state failed: {e}"));
        }

        host_log("debug", "committed response");

        Some(
            serde_json::json!({
                "action": action,
                "content": if wants_tool {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(content.to_string())
                },
                "session_id": session_id,
                "step": next_step,
                "tool_name": tool_name,
                "tool_input": tool_input,
                "fsm_state": fsm_state,
            })
            .to_string(),
        )
    }

    /// Ask the host to execute the tool (escape hatch); echo takes no args.
    ///
    pub fn process_tool_result(session_id: &str, tool_result_json: &str) -> Option<String> {
        let input: serde_json::Value = serde_json::from_str(tool_result_json).ok()?;

        let tool_name = input
            .get("tool_name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("echo")
            .to_string();

        let state = match load_or_fresh(session_id) {
            Ok(state) => state,
            Err(e) => {
                return Some(error_envelope(
                    "process-tool-result-for-session",
                    &format!("load-state: {e}"),
                ));
            }
        };
        let step = state
            .get("step")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;

        let execution_json = match host_emit_tool_call(&tool_name, "{}", session_id, step) {
            Ok(json) => json,
            Err(e) => {
                return Some(error_envelope(
                    "process-tool-result-for-session",
                    &format!("host_emit_tool_call: {e}"),
                ));
            }
        };

        let execution: serde_json::Value = match serde_json::from_str(&execution_json) {
            Ok(value) => value,
            Err(e) => {
                return Some(error_envelope(
                    "process-tool-result-for-session",
                    &format!("cannot parse tool-execution-result: {e}"),
                ));
            }
        };
        let executed_name = execution
            .get("tool_name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&tool_name);
        let success = execution
            .get("success")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let output = execution
            .get("output_json")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let next_step = step + 1;

        let summary = format!("tool {executed_name} -> success: {success}, output: {output}");

        let mut history = state
            .get("message_history")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        if let Some(array) = history.as_array_mut() {
            array.push(serde_json::json!({
                "role": "tool",
                "content": summary,
                "tool_name": executed_name,
                "success": success,
                "output": output,
            }));
        }

        let updated = serde_json::json!({
            "session_id": session_id,
            "fsm_state": "final",
            "step": next_step,
            "message_history": history,
        });
        if let Err(e) = host_save_state(session_id, &updated.to_string()) {
            host_log("error", &format!("save-state failed: {e}"));
        }

        host_log("debug", "processed tool result");

        Some(
            serde_json::json!({
                "action": "final",
                "content": summary,
                "session_id": session_id,
                "step": next_step,
                "tool_name": serde_json::Value::Null,
                "tool_input": serde_json::Value::Null,
                "fsm_state": "final",
            })
            .to_string(),
        )
    }
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
