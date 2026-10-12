//! Wiring of the imported `logic-hooks` WIT interface into the runner pipeline.
//!
//! The wit-bindgen import shims
//! (`crate::wasm_exports::antikythera::agent_sdk::logic_hooks`) abort via
//! `unreachable!()` on native targets, so this whole module and every call
//! site is gated on `#[cfg(all(feature = "component", target_family = "wasm"))]`,
//! mirroring the `tool_registry::execute_builtin` wiring in `llm_stream`.
//! Native builds (including the native test suites, which enable the
//! `component` feature) never compile the shim calls.
//!
//! Contract (see `wit/antikythera.wit`, `interface logic-hooks`):
//! - Ok `{"passthrough": true}` (exactly the single-key object) -> use the
//!   SDK default at this pipeline point;
//! - Ok <any other JSON object> -> override the SDK decision at this point;
//! - Err(message) / unparseable / non-object -> fail-closed: abort the
//!   operation and surface the error. A failing hook never falls back.

use antikythera_log::LogLevel;

use super::runner_types::{CommitResult, ToolResultInput};
use super::{AgentRunnerError, wasm_log};
use crate::wasm_agent::types::AgentState;

/// Runs `logic-hooks.prepare-turn` and classifies the result.
///
/// `Ok(None)` = passthrough (keep the SDK default prepared turn);
/// `Ok(Some(value))` = override; `Err` = fail-closed abort.
pub(super) fn prepare_turn_override(
    state: &AgentState,
    request_json: &str,
) -> Result<Option<serde_json::Value>, AgentRunnerError> {
    let session_state_json = state.to_json().map_err(AgentRunnerError::from)?;
    let result = crate::wasm_exports::antikythera::agent_sdk::logic_hooks::prepare_turn(
        request_json,
        &session_state_json,
    );
    classify_hook_result("prepare-turn", result)
}

/// Runs `logic-hooks.decide-action` and classifies the result.
///
/// `Ok(None)` = passthrough (keep the SDK default action decision);
/// `Ok(Some(value))` = override; `Err` = fail-closed abort.
pub(super) fn decide_action_override(
    state: &AgentState,
    llm_response_json: &str,
) -> Result<Option<serde_json::Value>, AgentRunnerError> {
    let session_state_json = state.to_json().map_err(AgentRunnerError::from)?;
    let result = crate::wasm_exports::antikythera::agent_sdk::logic_hooks::decide_action(
        &session_state_json,
        llm_response_json,
    );
    classify_hook_result("decide-action", result)
}

/// Runs `logic-hooks.handle-tool-result` and classifies the result.
///
/// `Ok(None)` = passthrough (keep the SDK default tool-result processing);
/// `Ok(Some(value))` = override; `Err` = fail-closed abort.
pub(super) fn handle_tool_result_override(
    state: &AgentState,
    tool_result_json: &str,
) -> Result<Option<serde_json::Value>, AgentRunnerError> {
    let session_state_json = state.to_json().map_err(AgentRunnerError::from)?;
    let result = crate::wasm_exports::antikythera::agent_sdk::logic_hooks::handle_tool_result(
        &session_state_json,
        tool_result_json,
    );
    classify_hook_result("handle-tool-result", result)
}

/// Applies the `prepare-turn` override to the SDK default prepared turn.
///
/// On passthrough the default encoded JSON is returned byte-identical. On
/// override the hook's object is merged over the default object: fields
/// present in the override are authoritative, absent fields fall back to the
/// SDK default.
/// Composed provider passed through. Precedence A1a: consult the host-supplied `runtime-hooks` at
/// the same pipeline point; it is skipped entirely when `runtime_hooks_enabled` is false.
///
pub(super) fn apply_prepare_turn_override(
    state: &AgentState,
    request_json: &str,
    default_encoded: &str,
) -> Result<String, AgentRunnerError> {
    let Some(override_value) = prepare_turn_override(state, request_json)? else {
        return super::runtime_hooks::apply_prepare_turn_override(
            state,
            request_json,
            default_encoded,
        );
    };

    let default_value: serde_json::Value = serde_json::from_str(default_encoded).map_err(|e| {
        AgentRunnerError::Internal(format!(
            "logic-hook prepare-turn failed: default prepared turn is not JSON: {e}"
        ))
    })?;
    let merged = merge_json_object("prepare-turn", &default_value, &override_value)?;
    let encoded = serde_json::to_string(&merged).map_err(|e| {
        AgentRunnerError::Internal(format!(
            "logic-hook prepare-turn failed: cannot encode merged prepared turn: {e}"
        ))
    })?;
    wasm_log(
        &state.session_id,
        LogLevel::Debug,
        "logic-hook prepare-turn: override applied",
    );
    Ok(encoded)
}

/// Applies the `decide-action` override to the SDK default commit result.
///
/// On passthrough the default commit result is returned unchanged. On
/// override the hook's object is merged over the default commit-result
/// envelope so the SDK's bookkeeping fields (`session_id`, `step`,
/// `fsm_state`) survive a partial override; the merged value is committed as
/// the action result.
/// Composed provider passed through. Precedence A1a: consult the host-supplied `runtime-hooks` at
/// the same pipeline point; it is skipped entirely when `runtime_hooks_enabled` is false.
///
pub(super) fn apply_decide_action_override(
    state: &AgentState,
    llm_response_json: &str,
    default_result: &CommitResult,
) -> Result<CommitResult, AgentRunnerError> {
    let Some(override_value) = decide_action_override(state, llm_response_json)? else {
        return super::runtime_hooks::apply_decide_action_override(
            state,
            llm_response_json,
            default_result,
        );
    };

    let default_value = serde_json::to_value(default_result).map_err(|e| {
        AgentRunnerError::Internal(format!(
            "logic-hook decide-action failed: cannot encode default commit result: {e}"
        ))
    })?;
    let merged = merge_json_object("decide-action", &default_value, &override_value)?;
    let result: CommitResult = serde_json::from_value(merged).map_err(|e| {
        AgentRunnerError::Internal(format!(
            "logic-hook decide-action failed: override is not a valid commit result: {e}"
        ))
    })?;
    wasm_log(
        &state.session_id,
        LogLevel::Debug,
        &format!(
            "logic-hook decide-action: override applied (action={})",
            result.action
        ),
    );
    Ok(result)
}

/// Applies the `handle-tool-result` override in place of the reported tool
/// result.
///
/// On passthrough the reported input is returned unchanged. On override the
/// hook's JSON object is used as the tool result, replacing the reported one.
/// Composed provider passed through. Precedence A1a: consult the host-supplied `runtime-hooks` at
/// the same pipeline point; it is skipped entirely when `runtime_hooks_enabled` is false.
///
pub(super) fn apply_handle_tool_result_override(
    state: &AgentState,
    tool_result_json: &str,
    default_input: &ToolResultInput,
) -> Result<ToolResultInput, AgentRunnerError> {
    let Some(override_value) = handle_tool_result_override(state, tool_result_json)? else {
        return super::runtime_hooks::apply_handle_tool_result_override(
            state,
            tool_result_json,
            default_input,
        );
    };

    let input: ToolResultInput = serde_json::from_value(override_value).map_err(|e| {
        AgentRunnerError::Internal(format!(
            "logic-hook handle-tool-result failed: override is not a valid tool result: {e}"
        ))
    })?;
    wasm_log(
        &state.session_id,
        LogLevel::Debug,
        &format!(
            "logic-hook handle-tool-result: override applied (tool={})",
            input.tool_name
        ),
    );
    Ok(input)
}

/// Merges a hook override over a default JSON object: fields present in the
/// override are authoritative; absent fields keep the SDK default.
fn merge_json_object(
    hook_name: &str,
    default: &serde_json::Value,
    override_value: &serde_json::Value,
) -> Result<serde_json::Value, AgentRunnerError> {
    let Some(default_obj) = default.as_object() else {
        return Err(AgentRunnerError::Internal(format!(
            "logic-hook {hook_name} failed: SDK default is not a JSON object"
        )));
    };
    let Some(override_obj) = override_value.as_object() else {
        return Err(AgentRunnerError::Internal(format!(
            "logic-hook {hook_name} failed: override is not a JSON object"
        )));
    };
    let mut merged = default_obj.clone();
    for (key, value) in override_obj {
        merged.insert(key.clone(), value.clone());
    }
    Ok(serde_json::Value::Object(merged))
}

/// Classifies a hook result per the `logic-hooks` contract.
fn classify_hook_result(
    hook_name: &str,
    result: Result<String, String>,
) -> Result<Option<serde_json::Value>, AgentRunnerError> {
    let payload = result.map_err(|msg| {
        AgentRunnerError::Internal(format!("logic-hook {hook_name} failed: {msg}"))
    })?;
    let value: serde_json::Value = serde_json::from_str(&payload).map_err(|e| {
        AgentRunnerError::Internal(format!(
            "logic-hook {hook_name} failed: returned unparseable JSON: {e}"
        ))
    })?;
    if !value.is_object() {
        return Err(AgentRunnerError::Internal(format!(
            "logic-hook {hook_name} failed: returned non-object JSON"
        )));
    }
    if is_passthrough(&value) {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

/// The passthrough signal is exactly the single-key object
/// `{"passthrough": true}`; any other object is an override.
fn is_passthrough(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|map| {
        map.len() == 1 && map.get("passthrough") == Some(&serde_json::Value::Bool(true))
    })
}
