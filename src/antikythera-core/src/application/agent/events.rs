use crate::application::agent::AgentStep;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DomainEvent {
    AgentRunStarted {
        session_id: Option<String>,
        prompt: String,
    },
    AgentStepCompleted {
        step: AgentStep,
        remaining_steps: usize,
    },
    ToolInvoked {
        tool: String,
        input: Value,
        success: bool,
    },
    AgentRunCompleted {
        session_id: String,
        response: String,
        total_steps: usize,
    },
    SessionUpdated {
        session_id: String,
        message_count: usize,
    },
}

#[cfg(test)]
#[path = "../../../../../tests/rust/unit/antikythera-core/application/agent/events.tests.rs"]
mod tests;
