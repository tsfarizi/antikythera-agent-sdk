//! Streaming response collectors

use super::buffer::AgentEventStream;
use super::request::StreamingRequest;
use super::types::{AgentEvent, StreamingMode, ToolEventPhase};
use crate::logging::{SessionContext, StreamingLogger};
use serde::{Deserialize, Serialize};

/// Snapshot returned by in-memory streaming responders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StreamingSnapshot {
    pub mode: StreamingMode,
    pub tokens: Vec<String>,
    pub events: Vec<AgentEvent>,
    pub final_response: Option<String>,
}

/// Provider-facing abstraction for streaming responses.
pub trait StreamingResponse: Send {
    fn request(&self) -> &StreamingRequest;
    fn push_token(&mut self, token: String);
    fn push_event(&mut self, event: AgentEvent);
    fn set_final_response(&mut self, response: String);
    fn snapshot(&self) -> StreamingSnapshot;
}

/// In-memory streaming collector used by tests and host adapters.
#[derive(Debug, Clone)]
pub struct InMemoryStreamingResponse {
    request: StreamingRequest,
    tokens: Vec<String>,
    events: AgentEventStream,
    final_response: Option<String>,
}

impl InMemoryStreamingResponse {
    /// Build an in-memory stream collector from a request.
    pub fn new(request: StreamingRequest) -> Self {
        Self {
            events: AgentEventStream::with_max_buffered_events(request.max_buffered_events),
            request,
            tokens: Vec::new(),
            final_response: None,
        }
    }
}

impl StreamingResponse for InMemoryStreamingResponse {
    fn request(&self) -> &StreamingRequest {
        &self.request
    }

    fn push_token(&mut self, token: String) {
        let ctx = SessionContext::default();
        let log = StreamingLogger::from_context(&ctx);
        if self.request.wants_tokens() {
            self.tokens.push(token.clone());
        }
        if self.request.wants_events() {
            self.events.push(AgentEvent::Token {
                content: token.clone(),
            });
        }
        if self.request.wants_tokens() || self.request.wants_events() {
            log.token_emitted(ctx.session_id(), token.len());
        }
    }

    /// Phase 2 filtering: honour include_tool_results / include_summaries flags.
    ///
    fn push_event(&mut self, event: AgentEvent) {
        if !self.request.wants_events() {
            return;
        }
        if let Some(p2) = self.request.phase2.as_ref() {
            match &event {
                AgentEvent::ToolResult { .. } if !p2.include_tool_results => return,
                AgentEvent::Summary { .. } if !p2.include_summaries => return,
                _ => {}
            }
        }
        let ctx = SessionContext::default();
        let log = StreamingLogger::from_context(&ctx);
        if let AgentEvent::Tool { tool_name, phase } = &event {
            let phase_str = match phase {
                ToolEventPhase::Started => "started",
                ToolEventPhase::Finished => "finished",
            };
            log.tool_event(ctx.session_id(), tool_name, phase_str);
        }
        self.events.push(event);
    }

    fn set_final_response(&mut self, response: String) {
        if self.request.include_final_response {
            StreamingLogger::from_context(&SessionContext::default())
                .debug(format!("Final response set | len={}", response.len()));
            self.final_response = Some(response);
        }
    }

    fn snapshot(&self) -> StreamingSnapshot {
        StreamingLogger::from_context(&SessionContext::default()).debug(format!(
            "Streaming snapshot taken | tokens={} events={} has_final={}",
            self.tokens.len(),
            self.events.len(),
            self.final_response.is_some()
        ));
        StreamingSnapshot {
            mode: self.request.mode,
            tokens: self.tokens.clone(),
            events: self.events.events.iter().cloned().collect(),
            final_response: self.final_response.clone(),
        }
    }
}
