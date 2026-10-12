//! Core streaming types and events

use serde::{Deserialize, Serialize};

/// Streaming mode requested by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StreamingMode {
    Token,
    Event,
    #[default]
    Mixed,
}

/// Intermediate event emitted by the agent pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    Token {
        content: String,
    },
    Tool {
        tool_name: String,
        phase: ToolEventPhase,
    },
    State {
        state: String,
        detail: Option<String>,
    },
    Completed,
    /// A streaming chunk of tool-execution output.
    ToolResult {
        tool_name: String,
        chunk: String,
        is_final: bool,
    },
    /// A streaming chunk of context-management summarisation output.
    Summary {
        chunk: String,
        is_final: bool,
        original_message_count: usize,
    },
}

/// Tool event phase for structured tool events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEventPhase {
    Started,
    Finished,
}
