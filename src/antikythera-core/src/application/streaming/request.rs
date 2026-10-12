//! Streaming request definitions

use super::types::StreamingMode;
use crate::logging::{SessionContext, StreamingLogger};
use serde::{Deserialize, Serialize};

/// Streaming request describing what kind of incremental output is wanted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamingRequest {
    #[serde(default)]
    pub mode: StreamingMode,
    #[serde(default = "default_true")]
    pub include_final_response: bool,
    #[serde(default)]
    pub max_buffered_events: Option<usize>,
    /// Optional Phase 2 options.  When `None` the stream behaves exactly as Phase 1.
    #[serde(default)]
    pub phase2: Option<StreamingPhase2Options>,
}

pub(crate) const fn default_true() -> bool {
    true
}

impl Default for StreamingRequest {
    fn default() -> Self {
        StreamingLogger::from_context(&SessionContext::default())
            .debug("StreamingRequest created with defaults");
        Self {
            mode: StreamingMode::Mixed,
            include_final_response: true,
            max_buffered_events: None,
            phase2: None,
        }
    }
}

impl StreamingRequest {
    /// Returns true when token chunks should be surfaced.
    pub fn wants_tokens(&self) -> bool {
        matches!(self.mode, StreamingMode::Token | StreamingMode::Mixed)
    }

    /// Returns true when structured agent events should be surfaced.
    pub fn wants_events(&self) -> bool {
        matches!(self.mode, StreamingMode::Event | StreamingMode::Mixed)
    }

    /// Returns a reference to the Phase 2 options if present.
    pub fn phase2_opts(&self) -> Option<&StreamingPhase2Options> {
        self.phase2.as_ref()
    }

    /// Returns `true` when Phase 2 features are active.
    pub fn is_phase2(&self) -> bool {
        self.phase2.is_some()
    }
}

/// Phase 2 streaming options.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamingPhase2Options {
    /// Flush policy for buffering events before delivery.
    #[serde(default)]
    pub buffer_policy: super::buffer::BufferPolicy,
    /// Forward [`super::types::AgentEvent::ToolResult`] chunks to the host.
    #[serde(default = "default_true")]
    pub include_tool_results: bool,
    /// Forward [`super::types::AgentEvent::Summary`] chunks from context management to the host.
    #[serde(default = "default_true")]
    pub include_summaries: bool,
}

impl Default for StreamingPhase2Options {
    fn default() -> Self {
        Self {
            buffer_policy: super::buffer::BufferPolicy::Unbuffered,
            include_tool_results: true,
            include_summaries: true,
        }
    }
}
