//! Agent Finite State Machine
//!
//! Typed FSM that enforces valid state transitions in the agent loop.
//! Each variant corresponds to a concrete phase of execution;
//! illegal transitions are rejected at the call site rather than silently
//! corrupting runtime state.

use std::fmt;

use antikythera_macros::FsmComplete;
use serde::{Deserialize, Serialize};

/// Typed FSM states for the agent loop.
///
/// DESIGN NOTE: This enum is mirrored in antikythera-sdk/src/wasm_agent/types.rs
/// as `AgentFsmState`. The transition matrix MUST be kept in sync.
/// See: tests/sdk/wasm_agent/fsm_parity_tests.rs for parity verification.
///
/// Valid flow:
/// ```text
/// Idle → UserTurnPrepared → LlmStreaming → LlmCommitted
///   → ToolRequested → ToolResultProcessed → LlmStreaming (loop)
///   → ToolRequested → ToolResultProcessed → Final
///   → LlmCommitted → Final
///   → LlmCommitted → Idle (retry)
///   → ToolResultProcessed → Idle (retry)
///   → Final → Idle (new turn)
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, FsmComplete)]
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
pub enum AgentFsmState {
    Idle,
    UserTurnPrepared,
    LlmStreaming,
    LlmCommitted,
    ToolRequested,
    ToolResultProcessed,
    Final,
}

impl AgentFsmState {
    /// Returns the initial state of a fresh agent loop.
    pub fn initial() -> Self {
        Self::Idle
    }

    /// Attempt a state transition. Returns `Err` if the transition is not
    /// permitted by the FSM definition.
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

    /// Check whether a transition from `self` to `next` is valid without
    /// mutating state.
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

/// Display implementation: lowercase snake_case for log messages.
impl fmt::Display for AgentFsmState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    /// State the FSM was in when the transition was attempted.
    pub from: AgentFsmState,
    /// State the caller tried to transition to.
    pub to: AgentFsmState,
}

impl fmt::Display for FsmTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid FSM transition: ")?;
        fmt::Display::fmt(&self.from, f)?;
        f.write_str(" → ")?;
        fmt::Display::fmt(&self.to, f)
    }
}

impl std::error::Error for FsmTransitionError {}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-domain/fsm.tests.rs"]
mod tests;
