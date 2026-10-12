//! WASM boundary types for the agent runtime.
//!
//! These types intentionally mirror `antikythera_core::domain::entities` but are
//! defined separately because:
//! 1. WASM components cannot share Rust enum discriminants across the FFI boundary
//! 2. The WASM types include additional fields (e.g., `step_id`, `session_id`,
//!    `context_policy`) needed for the streaming protocol that native agents don't
//!    require
//! 3. Binary compatibility (Postcard serialization) requires stable layouts
//!    that the core domain types may evolve independently of
//!
//! Conversion between core and WASM types is provided via `From` implementations
//! when the `sdk-core` feature is enabled.

use antikythera_core::WasmBridge;
use antikythera_macros::{FsmComplete, WasmBridge};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[path = "prompt_variables.rs"]
pub mod prompt_variables;
#[path = "stream_types.rs"]
pub mod stream_types;
#[path = "tool_registry.rs"]
pub mod tool_registry;

pub use prompt_variables::*;
pub use stream_types::*;
pub use tool_registry::*;

/// Action the agent wants to take
#[derive(Debug, Clone, Serialize, Deserialize, WasmBridge)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AgentAction {
    /// Call a tool
    CallTool {
        tool: String,
        input: serde_json::Value,
    },
    /// Final response to user
    Final { response: serde_json::Value },
    /// Retry with error
    Retry { error: String },
}

/// Typed FSM states for the agent loop (WASM mirror of core::domain::fsm::AgentFsmState).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, FsmComplete)]
#[serde(rename_all = "snake_case")]
#[fsm_transitions(
    Idle => [UserTurnPrepared],
    UserTurnPrepared => [LlmStreaming],
    LlmStreaming => [LlmCommitted],
    LlmCommitted => [ToolRequested, Final, Idle],
    ToolRequested => [ToolResultProcessed],
    ToolResultProcessed => [LlmStreaming, Final, Idle],
    Final => [Idle]
)]
/// Agent FSM States (WASM mirror of core::domain::fsm::AgentFsmState) DESIGN NOTE: This enum is
/// intentionally duplicated from core::domain::fsm because WASM components cannot share Rust enum
/// discriminants across the FFI boundary. The transition matrix MUST be kept in sync with the
/// core copy. See: tests/sdk/wasm_agent/fsm_parity_tests.rs for parity verification.
///
pub enum AgentFsmState {
    #[default]
    Idle,
    UserTurnPrepared,
    LlmStreaming,
    LlmCommitted,
    ToolRequested,
    ToolResultProcessed,
    Final,
}

impl AgentFsmState {
    pub fn initial() -> Self {
        Self::Idle
    }

    pub fn transition_to(&mut self, next: AgentFsmState) -> Result<(), FsmTransitionError> {
        if self.can_transition_to(&next) {
            *self = next;
            Ok(())
        } else {
            Err(FsmTransitionError {
                from: *self,
                to: next,
            })
        }
    }

    pub fn can_transition_to(&self, next: &AgentFsmState) -> bool {
        (self == &Self::Idle && next == &Self::UserTurnPrepared)
            || (self == &Self::UserTurnPrepared && next == &Self::LlmStreaming)
            || (self == &Self::LlmStreaming && next == &Self::LlmCommitted)
            || (self == &Self::LlmCommitted && next == &Self::ToolRequested)
            || (self == &Self::LlmCommitted && next == &Self::Final)
            || (self == &Self::LlmCommitted && next == &Self::Idle)
            || (self == &Self::ToolRequested && next == &Self::ToolResultProcessed)
            || (self == &Self::ToolResultProcessed && next == &Self::LlmStreaming)
            || (self == &Self::ToolResultProcessed && next == &Self::Final)
            || (self == &Self::ToolResultProcessed && next == &Self::Idle)
            || (self == &Self::Final && next == &Self::Idle)
    }
}

impl std::fmt::Display for AgentFsmState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => f.write_str("idle"),
            Self::UserTurnPrepared => f.write_str("user_turn_prepared"),
            Self::LlmStreaming => f.write_str("llm_streaming"),
            Self::LlmCommitted => f.write_str("llm_committed"),
            Self::ToolRequested => f.write_str("tool_requested"),
            Self::ToolResultProcessed => f.write_str("tool_result_processed"),
            Self::Final => f.write_str("final"),
        }
    }
}

/// Error returned when an invalid FSM transition is attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FsmTransitionError {
    pub from: AgentFsmState,
    pub to: AgentFsmState,
}

impl std::fmt::Display for FsmTransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid FSM transition: ")?;
        std::fmt::Display::fmt(&self.from, f)?;
        f.write_str(" → ")?;
        std::fmt::Display::fmt(&self.to, f)
    }
}

impl std::error::Error for FsmTransitionError {}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TruncationStrategy {
    #[default]
    KeepNewest,
    KeepBalanced,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextPolicy {
    pub max_history_messages: usize,
    pub summarize_after_messages: usize,
    pub summary_max_chars: usize,
    #[serde(default)]
    pub truncation_strategy: TruncationStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextSummary {
    pub version: u64,
    pub text: String,
    pub source_messages: usize,
}

/// Agent session state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentState {
    /// Session ID
    pub session_id: String,
    /// Current step number
    pub current_step: u32,
    /// Message history (user + assistant + tool results)
    pub message_history: Vec<AgentMessage>,
    /// Tool call results
    pub tool_results: HashMap<String, serde_json::Value>,
    /// Agent configuration
    pub config: AgentConfig,
    /// Rolling summary for long context
    #[serde(default)]
    pub rolling_summary: Option<ContextSummary>,
    /// Current FSM state
    #[serde(default)]
    pub fsm_state: AgentFsmState,
}

impl AgentState {
    /// Create new session
    pub fn new(config: AgentConfig) -> Self {
        Self {
            session_id: config.session_id.clone(),
            current_step: 0,
            message_history: Vec::new(),
            tool_results: HashMap::new(),
            config,
            rolling_summary: None,
            fsm_state: AgentFsmState::initial(),
        }
    }

    /// Add message to history
    pub fn add_message(&mut self, message: AgentMessage) {
        self.message_history.push(message);
    }

    /// Record tool result
    pub fn record_tool_result(&mut self, tool_name: String, result: serde_json::Value) {
        self.tool_results.insert(tool_name, result);
        self.current_step += 1;
    }

    /// Check if max steps exceeded
    pub fn is_max_steps_exceeded(&self) -> bool {
        self.current_step >= self.config.max_steps
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("Serialize error: {}", e))
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Deserialize error: {}", e))
    }
}

/// Message in conversation (for WASM agent)
#[derive(Debug, Clone, Serialize, Deserialize, WasmBridge)]
pub struct AgentMessage {
    /// Role (user, assistant, system, tool)
    pub role: String,
    /// Message content
    pub content: String,
    /// Optional tool call info
    pub tool_call: Option<ToolCall>,
    /// Optional tool result
    pub tool_result: Option<ToolResult>,
}

/// Agent behavior config (matches WIT agent-config)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Maximum steps
    pub max_steps: u32,
    /// Verbose logging
    pub verbose: bool,
    /// Auto-execute tools
    pub auto_execute_tools: bool,
    /// Consult the host-supplied `runtime-hooks` interface at the pipeline
    /// points (A1a: after the composed `logic-hooks` provider passes through).
    /// When `false`, runtime hooks are never called (pre-runtime-hooks
    /// behavior). Defaults to `true`; the serde default keeps state snapshots
    /// produced before this flag existed deserializable.
    #[serde(default = "default_true")]
    pub runtime_hooks_enabled: bool,
    /// Session timeout (seconds)
    pub session_timeout_secs: u32,
    /// Session ID
    pub session_id: String,
    /// Default context policy
    #[serde(default)]
    pub context_policy: ContextPolicy,
}

fn default_true() -> bool {
    true
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_steps: 10,
            verbose: false,
            auto_execute_tools: true,
            runtime_hooks_enabled: true,
            session_timeout_secs: 300,
            session_id: format!("session-{}", antikythera_log::wasm_compat::now_unix_ms()),
            context_policy: ContextPolicy {
                max_history_messages: 24,
                summarize_after_messages: 12,
                summary_max_chars: 1200,
                truncation_strategy: TruncationStrategy::KeepNewest,
            },
        }
    }
}

#[cfg(feature = "component")]
/// Conversions from core domain types.
///
impl From<antikythera_core::domain::entities::AgentAction> for AgentAction {
    fn from(core_action: antikythera_core::domain::entities::AgentAction) -> Self {
        match core_action {
            antikythera_core::domain::entities::AgentAction::CallTool(tool_call) => {
                let input =
                    serde_json::to_value(&tool_call.arguments).unwrap_or(serde_json::Value::Null);
                AgentAction::CallTool {
                    tool: tool_call.name,
                    input,
                }
            }
            antikythera_core::domain::entities::AgentAction::FinalResponse(response) => {
                AgentAction::Final {
                    response: serde_json::Value::String(response),
                }
            }
            antikythera_core::domain::entities::AgentAction::Error(_) => AgentAction::Retry {
                error: "Agent error".to_string(),
            },
        }
    }
}

#[cfg(feature = "component")]
impl From<&antikythera_core::config::schema::AgentConfig> for AgentConfig {
    fn from(core_config: &antikythera_core::config::schema::AgentConfig) -> Self {
        Self {
            max_steps: core_config.max_steps,
            verbose: core_config.verbose,
            auto_execute_tools: core_config.auto_execute_tools,
            runtime_hooks_enabled: true,
            session_timeout_secs: core_config.session_timeout_secs,
            session_id: format!("session-{}", antikythera_log::wasm_compat::now_unix_ms()),
            context_policy: ContextPolicy::default(),
        }
    }
}
