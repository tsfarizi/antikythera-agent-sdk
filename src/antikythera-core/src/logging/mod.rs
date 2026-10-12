//! Core Logging Module
//!
//! Centralized logging for antikythera-core.
//! All log entries automatically include the source module.
//!
//! ## Architecture
//!
//! This module provides **typed module loggers** that wrap the underlying
//! `antikythera_log::Logger` with automatic source tagging. Each subsystem
//! (agent, transport, provider, etc.) has its own logger type that ensures
//! every log entry is annotated with its origin.
//!
//! The global `LOGGERS` registry is owned by `antikythera-log` (in
//! `antikythera_log::session_logger`) so that any crate depending on
//! `antikythera-log` can share the same per-session buffers. This module
//! re-exports the registry helpers for backward compatibility and layers on
//! the typed module loggers below.
//!
//! ## Sub-modules
//!
//! - [`crate::logging::context`] -- [`crate::logging::SessionContext`] for propagating session IDs through
//!   the call stack without relying on global state.
//! - [`crate::logging::provider`] -- The concrete [`crate::application::ports::logging::LogProvider`] implementation
//!   ([`crate::logging::provider::AntikytheraLogProvider`]) that bridges port traits to
//!   `antikythera_log`.

pub mod context;
mod module_loggers;
pub mod provider;
mod registry;

pub use context::SessionContext;
pub use module_loggers::*;
pub use registry::*;
