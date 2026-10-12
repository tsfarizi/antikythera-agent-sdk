//! Split into 5 parts for consistent test organization.
//!

/// Config loading tests - testing AppConfig::load behavior. The application stores all
/// configuration as a single TOML file (app.toml). Tests verify: file-not-found error, self-heal
/// on corrupt data, and correct field values on a valid TOML config.
///
use antikythera_core::config::ModelConfig;
use antikythera_core::config::toml_config::{TomlAppConfig, config_to_toml};
use antikythera_core::config::{AppConfig, ConfigError};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

/// Serialize a `TomlAppConfig` to a temp file and return the path.
fn write_toml_config(dir: &Path, config: &TomlAppConfig) -> std::path::PathBuf {
    let path = dir.join("app.toml");
    let data = config_to_toml(config).expect("Failed to serialize TomlAppConfig");
    fs::write(&path, data.as_bytes()).expect("Failed to write app.toml");
    path
}

/// A minimal valid `TomlAppConfig` for testing.
fn minimal_toml_config() -> TomlAppConfig {
    TomlAppConfig {
        model: ModelConfig {
            default_provider: "test-provider".to_string(),
            model: "test-model".to_string(),
        },
        ..Default::default()
    }
}

include!("loading_tests/config_not_found.rs");
include!("loading_tests/self_heal_corrupt_data.rs");
include!("loading_tests/prompt_template_loading.rs");
include!("loading_tests/toml_roundtrip.rs");
include!("loading_tests/actual_config_loading.rs");
