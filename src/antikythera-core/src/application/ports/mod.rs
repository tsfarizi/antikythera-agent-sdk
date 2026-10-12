//! Port traits — the boundaries between Clean Architecture rings.
//!
//! In Clean Architecture, the application layer defines ports (interfaces)
//! that infrastructure implements. Here we re-export the traits that
//! application code depends on, making the dependency direction explicit.

pub mod id_generator;
pub mod logging;
pub mod observability;
pub mod security;
pub mod session_store;
pub mod tool_server;

/// Re-export infrastructure traits as application ports (dependency inversion). The trait is
/// defined in infrastructure::model::traits, but application depends on it only through this port
/// boundary.
///
pub use crate::infrastructure::model::traits::{ModelClient, ModelProvider};
pub use observability::{AuditSink, MetricsExporter, TracingHook};
pub use security::{InputValidator, RateLimiter, SecretStore};
