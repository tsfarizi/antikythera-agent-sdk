//! Split into 5 parts for consistent test organization.
//!
//! Centralized unit tests for the WASM agent processor.
//!
//! Validates the generic JSON format contract (the only format WASM now accepts)
//! and the plain-text fallback.  Provider-native formats (OpenAI, Gemini,
//! Anthropic) are intentionally **not** tested here — that parsing is the
//! host's responsibility via FFI.

use antikythera_sdk::wasm_agent::types::AgentFsmState;
use antikythera_sdk::{
    AgentAction, AgentState, ToolDefinition, ToolParameterSchema, ToolRegistry,
    ToolValidationError, WasmAgentConfig, process_llm_response, validate_tool_call,
};

fn fresh_state() -> AgentState {
    let mut state = AgentState::new(WasmAgentConfig::default());
    state.fsm_state = AgentFsmState::LlmStreaming;
    state
}

include!("processor_tests/generic_call_tool_format.rs");
include!("processor_tests/shorthand_rejected_formats.rs");
include!("processor_tests/openai_reject_step_counter.rs");
include!("processor_tests/tool_call_validation.rs");
include!("processor_tests/tool_registry_prompt.rs");
