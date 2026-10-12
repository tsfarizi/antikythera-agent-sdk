//! Multi-agent orchestration.
//!
//! This module provides production-ready multi-agent scheduling, routing, and
//! pipeline execution on top of the existing single-agent [`Agent`] runner.
//!
//! # Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │          MultiAgentOrchestrator<P>            │
//! │  ┌─────────────┐  ┌──────────┐  ┌─────────┐  │
//! │  │AgentRegistry│  │TaskSched │  │Router   │  │
//! │  └─────────────┘  └──────────┘  └─────────┘  │
//! │         ↓               ↓            ↓        │
//! │    AgentProfile    ExecutionMode  AgentRouter  │
//! └──────────────────────────────────────────────┘
//!                   ↓
//!              Agent<P>::run(...)
//! ```
//!
//! # Quick start
//!
//! ```rust,compile_fail
//! use std::sync::Arc;
//! use antikythera_core::application::agent::multi_agent::{
//!     orchestrator::MultiAgentOrchestrator,
//!     registry::AgentProfile,
//!     task::AgentTask,
//!     execution::ExecutionMode,
//! };
//!
//! // MultiAgentOrchestrator::new expects Arc<McpClient<P>> (concrete generic),
//! // not a trait object. The McpClient is provided by antikythera-sdk.
//! # async fn run(client: Arc<antikythera_core::application::tooling::interface::ServerManager>) {
//! # let orch = MultiAgentOrchestrator::new(client, ExecutionMode::Auto);
//! # }
//! ```
//!
//! [`Agent`]: crate::application::agent::runner::Agent

pub mod budget;
pub mod cancellation;
pub mod execution;
pub mod guardrails;
pub mod orchestrator;
pub mod registry;
pub mod router;
pub mod scheduler;
pub mod session;
pub mod task;

/// Convenient re-exports (maintain backwards compatibility).
///
pub use budget::{BudgetSnapshot, OrchestratorBudget};
pub use cancellation::{CancellationSnapshot, CancellationToken};
pub use execution::ExecutionMode;
pub use guardrails::{
    BudgetGuardrail, CancellationGuardrail, GuardrailChain, GuardrailContext, GuardrailRejection,
    GuardrailStage, RateLimitGuardrail, TaskGuardrail, TimeoutGuardrail,
};
pub use orchestrator::MultiAgentOrchestrator;
pub use registry::{
    AgentProfile, AgentRegistry, AgentRole, ContextId, MemoryConfig, SyncMemoryProvider,
};
pub use router::{AgentRouter, DirectRouter, FirstAvailableRouter, RoleRouter, RoundRobinRouter};
pub use scheduler::TaskScheduler;
pub use session::{ManagedSession, OrchestratorSessionManager, SessionError};
pub use task::{
    AgentTask, ErrorKind, PipelineResult, RetryCondition, RoutingDecision, TaskExecutionMetadata,
    TaskResult, TaskRetryPolicy,
};
