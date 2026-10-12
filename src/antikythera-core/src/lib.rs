//! # Antikythera Core
//!
//! Pure agent management system — provider-agnostic, transport-agnostic,
//! format-agnostic.
//!
//! ## Architecture
//!
//! This crate defines the **domain model**, **application logic**, and **port
//! traits** for an autonomous AI agent system. Concrete implementations of
//! transport, model providers, and security live in peripheral crates
//! (`antikythera-sdk`).
//!
//! ### Layer Structure
//! - `domain/` — Canonical entity definitions (Message, ToolCall, AgentTask, etc.)
//! - `application/` — Agent runner, orchestration, hooks, streaming, resilience
//! - `config/` — Format-agnostic configuration schema
//! - `infrastructure/` — Model provider traits (host-delegated only)
//! - `security/` — Security port traits and implementations
//! - `logging/` — Structured logging system

pub mod application;
pub mod config;
pub mod constants;
pub mod domain;
pub mod infrastructure;

/// Security module for input validation, rate limiting, and secrets management
pub mod security;

/// Unified logging system for all core operations
pub mod logging;

pub use application::agent::events::DomainEvent;
pub use application::agent::{Agent, AgentOptions, AgentOutcome, ToolDescriptor};

/// Re-export agent client types for backward compatibility.
///
pub use application::agent::client::{
    ChatRequest, ChatResult, ClientConfig, ClientConfigSnapshot, McpError, PreparedChatTurn,
};

pub use config::AppConfig;

/// Re-export WASM boundary bridge trait for ergonomic `antikythera_core::WasmBridge`.
///
pub use infrastructure::wasm_bridge::WasmBridge;

/// Crate version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
