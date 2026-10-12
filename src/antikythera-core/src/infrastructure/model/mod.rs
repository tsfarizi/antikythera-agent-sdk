//! Model infrastructure module
//!
//! # Architecture — WASM boundary
//!
//! This module defines the LLM provider *contract* (traits + types) that is
//! safe to compile into any target including `wasm32-wasip2` (WASM component).
//!
//! The framework no longer ships built-in HTTP model clients as an active
//! runtime path. All model dispatch is delegated to the embedding host through
//! a host-provided transport or through the two-phase prepare/complete flow.
//!
//! # Structure
//! - `types`   — Request, Response, Error types (always compiled)
//! - `traits`  — `ModelProvider`, `ModelClient` traits (always compiled)
//! - `host`    — host-delegating `ModelClient` implementation
//! - `provider` — `DynamicModelProvider` routing layer (always compiled;
//!   `from_configs` remains only as a compatibility shim that now
//!   returns an unsupported-operation error)

pub mod host;
pub mod provider;
pub mod traits;
pub mod types;

pub use host::{HostModelClient, HostModelResponse, HostModelTransport};
pub use provider::DynamicModelProvider;
pub use traits::ModelProvider;
pub use types::{ModelError, ModelParams, ModelRequest, ModelResponse};
