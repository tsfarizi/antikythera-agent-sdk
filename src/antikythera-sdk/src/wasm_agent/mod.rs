//! WASM Agent Module
//!
//! WASM component that processes LLM responses from host.
//! WASM does NOT call LLM APIs directly.
//!
//! ## Architecture
//!
//! ```text
//! wasm_agent/
//! ├── mod.rs         # Module exports
//! ├── types.rs       # Agent types (state, messages, actions)
//! ├── processor.rs   # LLM response processing logic
//! └── runner.rs      # Agent runner (session management, lifecycle)
//! ```
//!
//! ## Host Responsibility
//!
//! The host (TypeScript/Python/Go) handles:
//! - LLM API calls (OpenAI, Anthropic, Gemini, Ollama)
//! - API key management
//! - Rate limiting & retry logic
//! - MCP tool execution
//!
//! ## WASM Responsibility
//!
//! WASM handles:
//! - Agent FSM logic
//! - JSON parsing & validation
//! - Schema enforcement
//! - Prompt building
//! - Tool call extraction

pub mod processor;
pub mod runner;
pub mod types;

pub use types::{
    AgentAction, AgentConfig as WasmAgentConfig, AgentMessage, AgentState, ContextPolicy,
    ContextSummary, PromptVariables, SloSnapshot, StreamEvent, StreamEventKind, TelemetrySnapshot,
    ToolCall, ToolDefinition, ToolParameterSchema, ToolRegistry, ToolResult, ToolValidationError,
    TruncationStrategy,
};

pub use processor::{
    build_llm_messages, build_system_prompt, process_llm_response, process_tool_result,
    validate_json_schema, validate_tool_call,
};

pub use runner::{
    AgentRunnerError, AgentRunnerRuntime, append_llm_chunk, commit_llm_response, commit_llm_stream,
    drain_events, get_slo_snapshot, get_state, get_telemetry_snapshot, get_tools_prompt, init,
    new_session_id, prepare_user_turn, process_llm_response_for_session,
    process_tool_result_for_session, register_tools, reset_session, set_context_policy,
    sweep_idle_sessions,
};
