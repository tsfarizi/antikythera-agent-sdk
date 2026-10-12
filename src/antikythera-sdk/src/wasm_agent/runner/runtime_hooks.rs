//! The pure classification/merge/precedence machinery below is compiled on native for the unit
//! tests; in native non-test builds it is consumed only by the cfg-gated wasm wrappers further
//! down, so dead-code analysis flags it.
//!
//! Wiring of the imported `runtime-hooks` WIT interface into the runner pipeline.
//!
//! The host supplies this interface at runtime (wasmtime linker on the
//! server, jco import object on the client), so the wit-bindgen import shims
//! (`crate::wasm_exports::antikythera::agent_sdk::runtime_hooks`) abort via
//! `unreachable!()` on native targets. Only the shim-calling wrappers are
//! gated on `#[cfg(all(feature = "component", target_family = "wasm"))]`;
//! the classification/merge/precedence functions below are pure and compile
//! on native so the native test suites can exercise the full contract.
//!
//! Precedence (A1a, see `wit/antikythera.wit`, `interface runtime-hooks`):
//! the composed `logic-hooks` provider is consulted FIRST; this module is
//! entered only when the composed provider returned the passthrough signal.
//! The composed apply functions in `super::logic_hooks` delegate here exactly
//! on that path, and the `apply_*` functions below always run with a composed
//! decision of `HookDecision::Passthrough`.
//!
//! Contract (identical to `logic-hooks`):
//! - Ok `{"passthrough": true}` (exactly the single-key object) -> use the
//!   SDK default at this pipeline point;
//! - Ok any other JSON object -> override the SDK decision at this point;
//! - Err(message) / unparseable / non-object -> fail-closed: abort the
//!   operation and surface the error. A failing hook never falls back.

#![allow(dead_code)]

use super::AgentRunnerError;
#[cfg(all(feature = "component", target_family = "wasm"))]
use super::runner_types::{CommitResult, ToolResultInput};
#[cfg(all(feature = "component", target_family = "wasm"))]
use super::wasm_log;
#[cfg(all(feature = "component", target_family = "wasm"))]
use crate::wasm_agent::types::AgentState;
#[cfg(all(feature = "component", target_family = "wasm"))]
use antikythera_log::LogLevel;

/// Classification of a hook result per the `runtime-hooks` contract.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum HookDecision {
    /// Ok `{"passthrough": true}` — use the SDK default at this pipeline point.
    Passthrough,
    /// Ok any other JSON object — override the SDK decision at this point.
    Override(serde_json::Value),
    /// Err / unparseable / non-object — fail-closed abort.
    Failed(String),
}

/// Pure: classifies a hook result per the `runtime-hooks` contract.
pub(super) fn classify_decision(hook_name: &str, result: Result<String, String>) -> HookDecision {
    let payload = match result {
        Ok(payload) => payload,
        Err(msg) => {
            return HookDecision::Failed(format!("runtime-hook {hook_name} failed: {msg}"));
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&payload) {
        Ok(value) => value,
        Err(e) => {
            return HookDecision::Failed(format!(
                "runtime-hook {hook_name} failed: returned unparseable JSON: {e}"
            ));
        }
    };
    if !value.is_object() {
        return HookDecision::Failed(format!(
            "runtime-hook {hook_name} failed: returned non-object JSON"
        ));
    }
    if is_passthrough(&value) {
        HookDecision::Passthrough
    } else {
        HookDecision::Override(value)
    }
}

/// The passthrough signal is exactly the single-key object
/// `{"passthrough": true}`; any other object is an override.
fn is_passthrough(value: &serde_json::Value) -> bool {
    value.as_object().is_some_and(|map| {
        map.len() == 1 && map.get("passthrough") == Some(&serde_json::Value::Bool(true))
    })
}

/// Pure: merges a runtime-hook override over a default JSON object; fields
/// present in the override are authoritative, absent fields keep the SDK
/// default. Identical merge semantics to the composed `logic-hooks` path.
pub(super) fn merge_json_object(
    hook_name: &str,
    default: &serde_json::Value,
    override_value: &serde_json::Value,
) -> Result<serde_json::Value, AgentRunnerError> {
    let Some(default_obj) = default.as_object() else {
        return Err(AgentRunnerError::Internal(format!(
            "runtime-hook {hook_name} failed: SDK default is not a JSON object"
        )));
    };
    let Some(override_obj) = override_value.as_object() else {
        return Err(AgentRunnerError::Internal(format!(
            "runtime-hook {hook_name} failed: override is not a JSON object"
        )));
    };
    let mut merged = default_obj.clone();
    for (key, value) in override_obj {
        merged.insert(key.clone(), value.clone());
    }
    Ok(serde_json::Value::Object(merged))
}

/// Pure: precedence A1a resolver — the single source of truth for the
/// "runtime-hooks runs only when the composed provider passed through" rule.
///
/// The composed `logic-hooks` decision is authoritative: an override is used
/// as-is and the runtime call is never made; a composed failure aborts. The
/// runtime hook is consulted only when the composed decision is passthrough
/// AND runtime hooks are enabled (`runtime_hooks_enabled` config flag). The
/// runtime call is injected as a closure so native tests verify the precedence
/// without touching the wit-bindgen shim (which aborts off wasm).
///
/// `Ok(None)` = final passthrough; `Ok(Some(value))` = override to apply;
/// `Err` = fail-closed abort.
pub(super) fn resolve_precedence(
    runtime_enabled: bool,
    composed: HookDecision,
    runtime_call: impl FnOnce() -> HookDecision,
) -> Result<Option<serde_json::Value>, AgentRunnerError> {
    let final_decision = match composed {
        HookDecision::Failed(msg) => return Err(AgentRunnerError::Internal(msg)),
        HookDecision::Override(value) => return Ok(Some(value)),
        HookDecision::Passthrough => {
            if !runtime_enabled {
                HookDecision::Passthrough
            } else {
                runtime_call()
            }
        }
    };
    match final_decision {
        HookDecision::Failed(msg) => Err(AgentRunnerError::Internal(msg)),
        HookDecision::Override(value) => Ok(Some(value)),
        HookDecision::Passthrough => Ok(None),
    }
}

#[cfg(all(feature = "component", target_family = "wasm"))]
fn call_prepare_turn(state: &AgentState, request_json: &str) -> HookDecision {
    let session_state_json = match state.to_json() {
        Ok(json) => json,
        Err(e) => {
            return HookDecision::Failed(format!("runtime-hook prepare-turn failed: {e}"));
        }
    };
    let result = crate::wasm_exports::antikythera::agent_sdk::runtime_hooks::prepare_turn(
        request_json,
        &session_state_json,
    );
    classify_decision("prepare-turn", result)
}

#[cfg(all(feature = "component", target_family = "wasm"))]
fn call_decide_action(state: &AgentState, llm_response_json: &str) -> HookDecision {
    let session_state_json = match state.to_json() {
        Ok(json) => json,
        Err(e) => {
            return HookDecision::Failed(format!("runtime-hook decide-action failed: {e}"));
        }
    };
    let result = crate::wasm_exports::antikythera::agent_sdk::runtime_hooks::decide_action(
        &session_state_json,
        llm_response_json,
    );
    classify_decision("decide-action", result)
}

#[cfg(all(feature = "component", target_family = "wasm"))]
fn call_handle_tool_result(state: &AgentState, tool_result_json: &str) -> HookDecision {
    let session_state_json = match state.to_json() {
        Ok(json) => json,
        Err(e) => {
            return HookDecision::Failed(format!("runtime-hook handle-tool-result failed: {e}"));
        }
    };
    let result = crate::wasm_exports::antikythera::agent_sdk::runtime_hooks::handle_tool_result(
        &session_state_json,
        tool_result_json,
    );
    classify_decision("handle-tool-result", result)
}

/// Applies the `prepare-turn` runtime override to the SDK default prepared
/// turn. Invoked only when the composed `logic-hooks` provider passed through
/// (precedence A1a); skipped when `runtime_hooks_enabled` is false. On
/// passthrough the default encoded JSON is returned byte-identical; on
/// override the hook's object is merged over the default object with the same
/// semantics as the composed path. A hook failure aborts the turn.
#[cfg(all(feature = "component", target_family = "wasm"))]
pub(super) fn apply_prepare_turn_override(
    state: &AgentState,
    request_json: &str,
    default_encoded: &str,
) -> Result<String, AgentRunnerError> {
    let Some(override_value) = resolve_precedence(
        state.config.runtime_hooks_enabled,
        HookDecision::Passthrough,
        || call_prepare_turn(state, request_json),
    )?
    else {
        return Ok(default_encoded.to_string());
    };

    let default_value: serde_json::Value = serde_json::from_str(default_encoded).map_err(|e| {
        AgentRunnerError::Internal(format!(
            "runtime-hook prepare-turn failed: default prepared turn is not JSON: {e}"
        ))
    })?;
    let merged = merge_json_object("prepare-turn", &default_value, &override_value)?;
    let encoded = serde_json::to_string(&merged).map_err(|e| {
        AgentRunnerError::Internal(format!(
            "runtime-hook prepare-turn failed: cannot encode merged prepared turn: {e}"
        ))
    })?;
    wasm_log(
        &state.session_id,
        LogLevel::Debug,
        "runtime-hook prepare-turn: override applied",
    );
    Ok(encoded)
}

/// Applies the `decide-action` runtime override to the SDK default commit
/// result. Invoked only when the composed `logic-hooks` provider passed
/// through (precedence A1a); skipped when `runtime_hooks_enabled` is false.
/// On override the hook's object is merged over the default commit-result
/// envelope so the SDK's bookkeeping fields survive a partial override; the
/// merged value is committed as the action result.
#[cfg(all(feature = "component", target_family = "wasm"))]
pub(super) fn apply_decide_action_override(
    state: &AgentState,
    llm_response_json: &str,
    default_result: &CommitResult,
) -> Result<CommitResult, AgentRunnerError> {
    let Some(override_value) = resolve_precedence(
        state.config.runtime_hooks_enabled,
        HookDecision::Passthrough,
        || call_decide_action(state, llm_response_json),
    )?
    else {
        return Ok(default_result.clone());
    };

    let default_value = serde_json::to_value(default_result).map_err(|e| {
        AgentRunnerError::Internal(format!(
            "runtime-hook decide-action failed: cannot encode default commit result: {e}"
        ))
    })?;
    let merged = merge_json_object("decide-action", &default_value, &override_value)?;
    let result: CommitResult = serde_json::from_value(merged).map_err(|e| {
        AgentRunnerError::Internal(format!(
            "runtime-hook decide-action failed: override is not a valid commit result: {e}"
        ))
    })?;
    wasm_log(
        &state.session_id,
        LogLevel::Debug,
        &format!(
            "runtime-hook decide-action: override applied (action={})",
            result.action
        ),
    );
    Ok(result)
}

/// Returns the raw `decide-action` runtime override for the dry-run pipeline
/// point (`process_llm_response_for_session`), which commits the hook's JSON
/// object directly instead of merging into a `CommitResult`. Invoked only
/// when the composed `logic-hooks` provider passed through (precedence A1a);
/// skipped when `runtime_hooks_enabled` is false.
#[cfg(all(feature = "component", target_family = "wasm"))]
pub(super) fn decide_action_raw_override(
    state: &AgentState,
    llm_response_json: &str,
) -> Result<Option<serde_json::Value>, AgentRunnerError> {
    resolve_precedence(
        state.config.runtime_hooks_enabled,
        HookDecision::Passthrough,
        || call_decide_action(state, llm_response_json),
    )
}

/// Applies the `handle-tool-result` runtime override in place of the reported
/// tool result. Invoked only when the composed `logic-hooks` provider passed
/// through (precedence A1a); skipped when `runtime_hooks_enabled` is false.
/// On override the hook's JSON object is used as the tool result, replacing
/// the reported one.
#[cfg(all(feature = "component", target_family = "wasm"))]
pub(super) fn apply_handle_tool_result_override(
    state: &AgentState,
    tool_result_json: &str,
    default_input: &ToolResultInput,
) -> Result<ToolResultInput, AgentRunnerError> {
    let Some(override_value) = resolve_precedence(
        state.config.runtime_hooks_enabled,
        HookDecision::Passthrough,
        || call_handle_tool_result(state, tool_result_json),
    )?
    else {
        return Ok(default_input.clone());
    };

    let input: ToolResultInput = serde_json::from_value(override_value).map_err(|e| {
        AgentRunnerError::Internal(format!(
            "runtime-hook handle-tool-result failed: override is not a valid tool result: {e}"
        ))
    })?;
    wasm_log(
        &state.session_id,
        LogLevel::Debug,
        &format!(
            "runtime-hook handle-tool-result: override applied (tool={})",
            input.tool_name
        ),
    );
    Ok(input)
}
