use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::types::{
    AgentConfig, AgentState, StreamEvent, StreamEventKind, TelemetryCounters, TelemetrySnapshot,
    ToolRegistry,
};

use crate::sdk_logging::get_sdk_logger;
use antikythera_log::LogLevel;

mod context_manager;
mod llm_stream;
#[cfg(all(feature = "component", target_family = "wasm"))]
mod logic_hooks;
/// No cfg gate on this module: it holds the pure classification/merge/ precedence functions
/// compiled and unit-tested on native; only its shim-calling wrappers are gated on the
/// component+wasm cfg.
///
mod runner_telemetry;
mod runner_types;
mod runtime_hooks;
mod session_lifecycle;
#[cfg(test)]
#[path = "../../../../../tests/rust/unit/antikythera-sdk/wasm_agent/runner/tests.rs"]
mod tests;
mod tool_pipeline;
use runner_types::*;

pub(super) fn wasm_log(session_id: &str, level: LogLevel, message: &str) {
    get_sdk_logger(session_id).log_with_source(level, "wasm_agent", message);
}

/// Errors raised by the WASM agent runner during session lifecycle operations.
///
/// Returned by all public runner functions to signal failures in session
/// management, LLM interaction, tool execution, and configuration.
#[derive(Debug, Clone)]
pub enum AgentRunnerError {
    SessionNotFound(String),
    SessionArchived(String),
    ValidationFailed(String),
    ToolFailed(String),
    ConfigurationFailed(String),
    Internal(String),
}

impl From<AgentRunnerError> for String {
    fn from(e: AgentRunnerError) -> Self {
        e.to_string()
    }
}

impl std::fmt::Display for AgentRunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionNotFound(id) => write!(f, "Session not found: {id}"),
            Self::SessionArchived(id) => write!(f, "Session archived: {id}"),
            Self::ValidationFailed(msg) => write!(f, "Validation failed: {msg}"),
            Self::ToolFailed(msg) => write!(f, "Tool failed: {msg}"),
            Self::ConfigurationFailed(msg) => write!(f, "Configuration failed: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
        }
    }
}

impl std::error::Error for AgentRunnerError {}

impl From<String> for AgentRunnerError {
    fn from(msg: String) -> Self {
        AgentRunnerError::Internal(msg)
    }
}

struct SessionRuntime {
    state: AgentState,
    pending_llm_chunks: Vec<String>,
    events: Vec<StreamEvent>,
    seq: u64,
    last_touched_ms: i64,
    prepare_latencies_ms: Vec<u64>,
    commit_latencies_ms: Vec<u64>,
    telemetry: TelemetrySnapshot,
}

impl SessionRuntime {
    fn new(config: AgentConfig) -> Self {
        let session_id = config.session_id.clone();
        let now_ms = now_unix_ms();
        Self {
            state: AgentState::new(config),
            pending_llm_chunks: Vec::new(),
            events: Vec::new(),
            seq: 0,
            last_touched_ms: now_ms,
            prepare_latencies_ms: Vec::new(),
            commit_latencies_ms: Vec::new(),
            telemetry: TelemetrySnapshot {
                session_id,
                correlation_id: None,
                counters: TelemetryCounters::default(),
                total_prepare_latency_ms: 0,
                total_commit_latency_ms: 0,
                fsm_state: String::new(),
            },
        }
    }

    fn touch(&mut self, now_ms: i64) {
        self.last_touched_ms = now_ms;
    }

    fn emit_event(
        &mut self,
        kind: StreamEventKind,
        correlation_id: Option<String>,
        payload: serde_json::Value,
    ) {
        self.seq += 1;
        if correlation_id.is_some() {
            self.telemetry.correlation_id = correlation_id.clone();
        }
        self.events.push(StreamEvent {
            seq: self.seq,
            session_id: self.state.session_id.clone(),
            step: self.state.current_step,
            correlation_id,
            kind,
            payload,
        });
    }
}

/// Core WASM agent runtime holding all in-memory sessions, event queues,
/// tool registry, and default configuration for the agent runner.
///
/// Accessed through a global `OnceLock<Mutex<…>>` to provide a singleton
/// runtime for WASM FFI callers.
pub struct AgentRunnerRuntime {
    sessions: HashMap<String, SessionRuntime>,
    archived_sessions: HashMap<String, ArchivedSessionRecord>,
    pending_events: HashMap<String, Vec<StreamEvent>>,
    pending_event_seq: HashMap<String, u64>,
    default_config: AgentConfig,
    max_in_memory_sessions: usize,
    /// Tool definitions pushed from the host (MCP server capabilities).
    known_tools: ToolRegistry,
    /// Optional in-process tool executor for builtin tools.
    #[cfg(feature = "toolrunner")]
    pub toolrunner: Option<antikythera_toolrunner::ToolRunner>,
}

impl Default for AgentRunnerRuntime {
    fn default() -> Self {
        Self {
            sessions: HashMap::new(),
            archived_sessions: HashMap::new(),
            pending_events: HashMap::new(),
            pending_event_seq: HashMap::new(),
            default_config: AgentConfig::default(),
            max_in_memory_sessions: 128,
            known_tools: ToolRegistry::default(),
            #[cfg(feature = "toolrunner")]
            toolrunner: None,
        }
    }
}

impl AgentRunnerRuntime {
    /// Install an in-process tool runner for executing builtin tools without
    /// host round-trips.
    #[cfg(feature = "toolrunner")]
    pub fn set_toolrunner(&mut self, runner: antikythera_toolrunner::ToolRunner) {
        self.toolrunner = Some(runner);
    }

    fn configure(&mut self, config_json: &str) -> Result<String, AgentRunnerError> {
        let input: RunnerConfigInput = serde_json::from_str(config_json).map_err(|e| {
            AgentRunnerError::ConfigurationFailed(format!("Invalid config-json: {e}"))
        })?;

        if let Some(value) = input.max_steps {
            self.default_config.max_steps = value;
        }
        if let Some(value) = input.verbose {
            self.default_config.verbose = value;
        }
        if let Some(value) = input.auto_execute_tools {
            self.default_config.auto_execute_tools = value;
        }
        if let Some(value) = input.runtime_hooks_enabled {
            self.default_config.runtime_hooks_enabled = value;
        }
        if let Some(value) = input.session_timeout_secs {
            self.default_config.session_timeout_secs = value;
        }
        if let Some(value) = input.max_in_memory_sessions {
            self.max_in_memory_sessions = value.max(1);
        }
        if let Some(policy) = input.context_policy {
            self.default_config.context_policy = policy;
        }

        let session_id = input.session_id.unwrap_or_else(new_session_id);
        let mut config = self.default_config.clone();
        config.session_id = session_id.clone();
        self.sessions.entry(session_id.clone()).or_insert_with(|| {
            wasm_log("runtime", LogLevel::Info, "Session created");
            SessionRuntime::new(config)
        });

        let _ = self.enforce_capacity(Some(&session_id), None)?;

        Ok(session_id)
    }
}

fn runtime() -> &'static Mutex<AgentRunnerRuntime> {
    static RUNTIME: OnceLock<Mutex<AgentRunnerRuntime>> = OnceLock::new();
    RUNTIME.get_or_init(|| Mutex::new(AgentRunnerRuntime::default()))
}

fn with_runtime<T>(
    f: impl FnOnce(&mut AgentRunnerRuntime) -> Result<T, AgentRunnerError>,
) -> Result<T, AgentRunnerError> {
    let mut guard = runtime().lock().map_err(|_| {
        wasm_log("runtime", LogLevel::Error, "Runtime lock poisoned");
        AgentRunnerError::Internal("AgentRunner runtime lock poisoned".to_string())
    })?;
    f(&mut guard)
}

static SESSION_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn new_session_id() -> String {
    let ts_ns = antikythera_log::wasm_compat::now_timestamp_nanos();
    let seq = SESSION_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("session-{ts_ns}-{seq}")
}

pub(super) fn now_unix_ms() -> i64 {
    antikythera_log::wasm_compat::now_unix_ms()
}

pub fn init(config_json: &str) -> Result<String, AgentRunnerError> {
    with_runtime(|rt| rt.configure(config_json))
}

pub fn set_context_policy(policy_json: &str) -> Result<bool, AgentRunnerError> {
    with_runtime(|rt| rt.set_context_policy(policy_json))
}

pub fn prepare_user_turn(request_json: &str) -> Result<String, AgentRunnerError> {
    with_runtime(|rt| rt.prepare_user_turn(request_json))
}

pub fn append_llm_chunk(
    session_id: &str,
    chunk: &str,
    correlation_id: Option<&str>,
) -> Result<bool, AgentRunnerError> {
    with_runtime(|rt| rt.append_llm_chunk(session_id, chunk, correlation_id.map(|v| v.to_string())))
}

pub fn commit_llm_response(
    prepared_turn_json: &str,
    llm_response_json: &str,
) -> Result<String, AgentRunnerError> {
    with_runtime(|rt| rt.commit_llm_response(prepared_turn_json, llm_response_json))
}

pub fn commit_llm_stream(prepared_turn_json: &str) -> Result<String, AgentRunnerError> {
    with_runtime(|rt| rt.commit_llm_stream(prepared_turn_json))
}

pub fn process_llm_response_for_session(
    session_id: &str,
    llm_response_json: &str,
) -> Result<String, AgentRunnerError> {
    with_runtime(|rt| rt.process_llm_response(session_id, llm_response_json))
}

pub fn process_tool_result_for_session(
    session_id: &str,
    tool_result_json: &str,
) -> Result<String, AgentRunnerError> {
    with_runtime(|rt| rt.process_tool_result(session_id, tool_result_json))
}

pub fn drain_events(session_id: &str) -> Result<String, AgentRunnerError> {
    with_runtime(|rt| rt.drain_events(session_id))
}

pub fn get_telemetry_snapshot(session_id: &str) -> Result<String, AgentRunnerError> {
    with_runtime(|rt| rt.telemetry_snapshot(session_id))
}

pub fn get_slo_snapshot(session_id: &str) -> Result<String, AgentRunnerError> {
    with_runtime(|rt| rt.slo_snapshot(session_id))
}

pub fn get_state(session_id: &str) -> Result<String, AgentRunnerError> {
    with_runtime(|rt| {
        let Some(state) = rt.sessions.get(session_id) else {
            if rt.archived_sessions.contains_key(session_id) {
                wasm_log(
                    session_id,
                    LogLevel::Warn,
                    "get_state called on archived session",
                );
                return Err(AgentRunnerError::SessionArchived(format!(
                    "Session '{session_id}' is archived in host storage"
                )));
            }
            wasm_log(session_id, LogLevel::Error, "get_state: session not found");
            return Err(AgentRunnerError::SessionNotFound(session_id.to_string()));
        };
        state.state.to_json().map_err(AgentRunnerError::from)
    })
}

pub fn reset_session(session_id: &str) -> Result<bool, AgentRunnerError> {
    with_runtime(|rt| {
        let removed = rt.sessions.remove(session_id).is_some();
        rt.archived_sessions.remove(session_id);
        rt.pending_events.remove(session_id);
        rt.pending_event_seq.remove(session_id);
        Ok(removed)
    })
}

/// Register MCP tool definitions so WASM can validate LLM-driven tool calls.
///
/// `tools_json` must be a JSON array of `ToolDefinition` objects.  The call
/// replaces the entire registry; pass an empty array to clear it.  Returns the
/// number of tools registered.
pub fn register_tools(tools_json: &str) -> Result<u32, AgentRunnerError> {
    with_runtime(|rt| rt.register_tools(tools_json))
}

/// Returns a formatted tool-list block suitable for injection into a system
/// prompt, or an empty string when no tools have been registered.
pub fn get_tools_prompt() -> Result<String, AgentRunnerError> {
    with_runtime(|rt| rt.get_tools_prompt())
}

/// Trigger idle-timeout sweep manually (useful for deterministic tests and host schedulers).
pub fn sweep_idle_sessions(now_unix_ms: Option<i64>) -> Result<u32, AgentRunnerError> {
    with_runtime(|rt| rt.sweep_sessions(now_unix_ms))
}
