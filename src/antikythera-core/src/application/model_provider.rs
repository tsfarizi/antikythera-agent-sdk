//! Application-facing model provider port.
//!
//! This keeps application modules decoupled from concrete infrastructure
//! module paths while preserving the existing trait contract.

/// Port re-export: application depends on this trait via the ports boundary.
///
pub use crate::application::ports::ModelProvider;
