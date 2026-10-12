//! # Application Module
//!
//! This module contains the core application logic for the MCP client.
//!
//! ## Submodules
//!
//! - [`client`] - The main MCP client for communicating with AI models
//! - [`agent`] - Autonomous agent that can use tools and execute multi-step tasks
//! - [`hooks`] - Host authentication, correlation, policy, and telemetry middleware
//! - [`streaming`] - Token/event streaming primitives and host adapters
//! - [`tooling`] - Tool server management and MCP server integration
//! - [`resilience`] - Retry, timeout, context management, and health tracking
//! - [`observability`] - Caller context, telemetry events, and tracing hooks

pub mod agent;
pub mod client;
pub mod config;
pub mod hooks;
pub mod model_provider;
pub mod observability;
pub mod ports;
pub mod prompt_composer;
pub mod resilience;
pub(super) mod session_store;
pub mod streaming;
pub mod tooling;

pub use hooks::{
    AuthHook, CorrelationHook, HookContext, HookError, HookOperation, HookRegistry,
    HostHookMiddleware, InMemoryTelemetryHook, PolicyDecision, PolicyDecisionHook,
    PolicyDecisionInput, PolicyTarget, TelemetryHook,
};
pub use model_provider::ModelProvider;
pub use observability::{
    AuditCategory, AuditRecord, AuditTrail, CallerContext, InMemoryMetricsExporter,
    InMemoryTracingHook, LatencySummary, LatencyTracker, MetricKind, MetricRecord, MetricsExporter,
    NoOpObservabilityHook, ObservabilityHook, TelemetryEvent, TraceSpanContext, TraceStatus,
    TracingHook,
};
pub use streaming::{
    AgentEvent, AgentEventStream, BufferPolicy, ClientInputStream, InMemoryStreamingResponse,
    StreamingBuffer, StreamingMode, StreamingPhase2Options, StreamingRequest, StreamingResponse,
    StreamingSnapshot, ToolEventPhase,
};
