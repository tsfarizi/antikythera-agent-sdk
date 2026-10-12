//! # Agent Module
//!
//! This module provides an autonomous AI agent that can use tools and execute
//! multi-step tasks. The agent implements a feedback loop with JSON retry logic
//! for robust LLM interaction.
//!
//! ## Key Types
//!
//! - [`Agent`] - The main agent executor
//! - [`AgentOptions`] - Configuration options for agent behavior
//! - [`AgentOutcome`] - Result of agent execution
//! - [`ToolContext`] - Context passed to tools during execution
//! - [`AgentError`] - Errors that can occur during agent execution
//!
//! ## Agent Loop
//!
//! The agent operates in a loop:
//! 1. Send messages to LLM
//! 2. Parse JSON response (with retry on parse failure)
//! 3. If tool call requested, execute tool and continue
//! 4. If final response, return to user

pub mod client;
mod context;
mod directive;
mod errors;
pub mod events;
mod memory;
mod models;
mod response_embedder;
mod runner;
mod runtime;
mod tool_result_parser;

#[cfg(not(target_arch = "wasm32"))]
/// Multi-agent is only available on native targets (requires Send futures).
///
pub mod multi_agent;

pub use context::{ServerGuidance, ToolContext, ToolDescriptor};
pub use errors::{AgentError, ToolError};
pub use memory::{
    AgentStateSnapshot, MemoryError, MemoryProvider, STATE_SCHEMA_VERSION, StateMetadata,
};
pub use models::{AgentOptions, AgentOutcome, AgentStep};
pub use runner::Agent;
