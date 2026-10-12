//! # Antikythera SDK
//!
//! Server-side WASM component framework for the MCP client.

pub use antikythera_core::application::agent::{Agent, AgentOptions, AgentOutcome};
pub use antikythera_core::application::client::{ClientConfig, McpClient};
pub use antikythera_core::config::AppConfig;

#[cfg(not(target_arch = "wasm32"))]
/// Multi-agent orchestration types (native only - requires Send futures).
///
pub use antikythera_core::application::agent::multi_agent::{
    AgentProfile, AgentRegistry, AgentRole, AgentTask, BudgetSnapshot, CancellationToken,
    ContextId, ErrorKind, ManagedSession, MemoryConfig, OrchestratorBudget,
    OrchestratorSessionManager, PipelineResult, RetryCondition, RoutingDecision, SessionError,
    SyncMemoryProvider, TaskExecutionMetadata, TaskResult, TaskRetryPolicy,
};

/// Agent orchestration helpers (native only)
#[cfg(not(target_arch = "wasm32"))]
pub mod agents;

#[cfg(not(target_arch = "wasm32"))]
pub use agents::{
    OrchestratorContext, OrchestratorMonitorSnapshot, OrchestratorOptions, TaskResultDetail,
};

/// Prompt Management feature slice
pub mod prompts;

pub use prompts::{
    mcp_get_all_prompts, mcp_get_template, mcp_get_tool_guidance, mcp_reset_template,
    mcp_update_template, mcp_update_tool_guidance,
};

/// TOML Configuration feature slice
pub mod config;

pub use config::{
    CONFIG_PATH as TOML_CONFIG_PATH, config_from_toml, config_to_toml,
    load_config as load_toml_config, save_config as save_toml_config,
};

/// Session Management module
pub mod session;

pub use session::{
    BatchExport, BatchLogExport, Message, MessageRole, Session, SessionExport, SessionLogExport,
    SessionSummary,
};

/// SDK Logging module
pub mod sdk_logging;

pub use sdk_logging::{
    ConfigFfiLogger, clear_sdk_loggers, clear_sdk_session_logs, get_latest_sdk_logs,
    get_sdk_logger, get_sdk_logs_json, query_sdk_logs, subscribe_sdk_logs,
};

/// Shared FFI helper utilities
pub mod ffi_helpers;

/// WASM Runtime — Wasmtime-based WASM agent runner (host-side).
#[cfg(feature = "wasm-sandbox")]
pub mod wasm_runtime;

/// WASM Agent Module (processes LLM responses from host)
#[cfg(feature = "component")]
pub mod wasm_agent;

/// WIT export layer for the component world (`antikythera:agent-sdk/runner`)
#[cfg(feature = "component")]
pub mod wasm_exports;

#[cfg(feature = "component")]
pub use wasm_agent::{
    AgentAction, AgentMessage, AgentRunnerError, AgentState, ContextPolicy, ContextSummary,
    PromptVariables, SloSnapshot, StreamEvent, StreamEventKind, TelemetrySnapshot, ToolCall,
    ToolDefinition, ToolParameterSchema, ToolRegistry, ToolResult, ToolValidationError,
    TruncationStrategy, WasmAgentConfig, append_llm_chunk, build_llm_messages, build_system_prompt,
    commit_llm_response, commit_llm_stream, drain_events, get_slo_snapshot,
    get_state as get_agent_state, get_telemetry_snapshot, get_tools_prompt,
    init as init_agent_runner, prepare_user_turn, process_llm_response,
    process_llm_response_for_session, process_tool_result, process_tool_result_for_session,
    register_tools, reset_session as reset_agent_session, set_context_policy, sweep_idle_sessions,
    validate_json_schema, validate_tool_call,
};

pub use antikythera_core::infrastructure::model::{
    HostModelClient, HostModelResponse, HostModelTransport,
};

/// SDK version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
