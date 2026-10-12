//! # Configuration Module
//!
//! Re-exports from `antikythera-config` crate.
//! The wizard submodule remains in core for CLI integration.

#[cfg(not(target_arch = "wasm32"))]
pub mod wizard;

/// Re-export everything from the extracted config crate.
///
pub use antikythera_config::app::{
    AgentConfig, AppConfig, DocServerConfig, ModelConfig, ModelInfo, PromptsConfig, ProviderConfig,
    RestServerConfig,
};
pub use antikythera_config::error::ConfigError;
pub use antikythera_config::loader;
pub use antikythera_config::schema;
pub use antikythera_config::serializer;
pub use antikythera_config::server::{self, ServerConfig, TransportType};
pub use antikythera_config::toml_config;
pub use antikythera_config::tool;
pub use antikythera_config::{CONFIG_PATH, ENV_PATH};
