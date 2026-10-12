//! Split by concern to keep file size manageable and improve readability.
//!
//! Centralized tests for the WASM agent runner.
//!
//! Covers: session lifecycle, commit flows (plain text + structured tool call),
//! streaming commit, telemetry counters, global context-policy update,
//! and rolling summarization with the `KeepBalanced` truncation strategy.

use antikythera_sdk::wasm_agent::runner::{
    AgentRunnerRuntime, append_llm_chunk, commit_llm_response, commit_llm_stream, drain_events,
    get_state, get_telemetry_snapshot, get_tools_prompt, init, new_session_id, prepare_user_turn,
    register_tools, set_context_policy, sweep_idle_sessions,
};

include!("runner_tests/session_config_prepare.rs");
include!("runner_tests/plain_text_commit.rs");
include!("runner_tests/structured_tool_call.rs");
include!("runner_tests/stream_drain_events.rs");
include!("runner_tests/telemetry_counters.rs");
include!("runner_tests/context_policy_global.rs");
include!("runner_tests/keep_balanced_truncation.rs");
include!("runner_tests/register_tools_prompt.rs");
include!("runner_tests/unknown_tool_error.rs");
include!("runner_tests/missing_param_error.rs");
include!("runner_tests/valid_tool_passes.rs");
include!("runner_tests/empty_registry_any_tool.rs");
include!("runner_tests/capacity_pressure_archive.rs");
include!("runner_tests/archived_session_restore.rs");
include!("runner_tests/archived_session_unavailable.rs");
include!("runner_tests/sweep_idle_sessions.rs");
include!("runner_tests/p95_session_id_utils.rs");
