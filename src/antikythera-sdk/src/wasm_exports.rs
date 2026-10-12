//! Wit-bindgen emits pre-2024-edition export shims; the unsafe-op-in- unsafe-fn lint fires inside
//! the generated code, not our wrapper.
//!
//! The export layer (Guest impl, unit struct, private helpers) is dead on native targets:
//! `export!` is wasm-gated, so only the generated bindings are consumed there.
//!
//! WIT export names (`antikythera:agent-sdk/runner@1.0.0#...`) contain `:`/`/`/`@`/`#`, which
//! break the GNU ld version script rustc generates for native cdylibs (`; expected, but got :`).
//! The symbols are meaningful only on wasm component targets, so `export!` is gated on the target
//! family; the generated bindings above remain available on native (tests enable `component`).
//!
//! WIT export layer for the Antikythera WASM component.
//!
//! Thin adapter between the WIT world `antikythera:agent-sdk/antikythera-agent-sdk`
//! (interface `runner`) and the existing `crate::wasm_agent::runner` API.
//! No business logic lives here: JSON strings pass through unmodified and
//! `AgentRunnerError` is flattened to `String` via `Display`.

#![allow(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

wit_bindgen::generate!({
    world: "antikythera-agent-sdk",
    path: "../../wit/antikythera.wit",
});

use crate::wasm_agent::runner;

struct Runner;

impl exports::antikythera::agent_sdk::runner::Guest for Runner {
    fn init(config_json: String) -> Result<String, String> {
        runner::init(&config_json).map_err(|e| e.to_string())
    }

    fn prepare_user_turn(request_json: String) -> Result<String, String> {
        runner::prepare_user_turn(&request_json).map_err(|e| e.to_string())
    }

    fn commit_llm_response(
        prepared_turn_json: String,
        llm_response_json: String,
    ) -> Result<String, String> {
        runner::commit_llm_response(&prepared_turn_json, &llm_response_json)
            .map_err(|e| e.to_string())
    }

    fn commit_llm_stream(prepared_turn_json: String) -> Result<String, String> {
        runner::commit_llm_stream(&prepared_turn_json).map_err(|e| e.to_string())
    }

    fn process_llm_response_for_session(
        session_id: String,
        llm_response_json: String,
    ) -> Result<String, String> {
        runner::process_llm_response_for_session(&session_id, &llm_response_json)
            .map_err(|e| e.to_string())
    }

    fn process_tool_result_for_session(
        session_id: String,
        tool_result_json: String,
    ) -> Result<String, String> {
        runner::process_tool_result_for_session(&session_id, &tool_result_json)
            .map_err(|e| e.to_string())
    }

    fn append_llm_chunk(
        session_id: String,
        chunk: String,
        correlation_id: Option<String>,
    ) -> Result<bool, String> {
        runner::append_llm_chunk(&session_id, &chunk, correlation_id.as_deref())
            .map_err(|e| e.to_string())
    }

    fn drain_events(session_id: String) -> Result<String, String> {
        runner::drain_events(&session_id).map_err(|e| e.to_string())
    }

    fn get_state(session_id: String) -> Result<String, String> {
        runner::get_state(&session_id).map_err(|e| e.to_string())
    }

    fn reset_session(session_id: String) -> Result<bool, String> {
        runner::reset_session(&session_id).map_err(|e| e.to_string())
    }

    fn sweep_idle_sessions(now_unix_ms: Option<i64>) -> Result<u32, String> {
        runner::sweep_idle_sessions(now_unix_ms).map_err(|e| e.to_string())
    }

    fn register_tools(tools_json: String) -> Result<u32, String> {
        runner::register_tools(&tools_json).map_err(|e| e.to_string())
    }

    fn get_tools_prompt() -> Result<String, String> {
        runner::get_tools_prompt().map_err(|e| e.to_string())
    }

    fn set_context_policy(policy_json: String) -> Result<bool, String> {
        runner::set_context_policy(&policy_json).map_err(|e| e.to_string())
    }

    fn get_telemetry_snapshot(session_id: String) -> Result<String, String> {
        runner::get_telemetry_snapshot(&session_id).map_err(|e| e.to_string())
    }

    fn get_slo_snapshot(session_id: String) -> Result<String, String> {
        runner::get_slo_snapshot(&session_id).map_err(|e| e.to_string())
    }
}

#[cfg(target_family = "wasm")]
export!(Runner);
