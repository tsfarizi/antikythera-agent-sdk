//! TOML serialization helpers for [`crate::schema::AppConfig`].
//!
//! The canonical struct definitions live in [`crate::schema`].  This module
//! provides the thin serialize/deserialize/load/save functions that operate
//! on the TOML text file (`app.toml`).

pub use super::schema::AppConfig;
use std::path::Path;

/// Configuration file path (project root)
pub const CONFIG_PATH: &str = "app.toml";

/// Environment file path (project root)
pub const ENV_PATH: &str = ".env";

/// Serialize configuration to TOML string.
pub fn config_to_toml(config: &AppConfig) -> Result<String, String> {
    toml::to_string(config).map_err(|e| format!("Failed to serialize config: {}", e))
}

/// Deserialize configuration from TOML string.
pub fn config_from_toml(data: &str) -> Result<AppConfig, String> {
    toml::from_str(data).map_err(|e| format!("Failed to deserialize config: {}", e))
}

/// Load configuration from file.
pub fn load_config(path: Option<&Path>) -> Result<AppConfig, String> {
    let config_path = path.unwrap_or(Path::new(CONFIG_PATH));

    if !config_path.exists() {
        return Err(format!("Config file not found: {}", config_path.display()));
    }

    let data = std::fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read config file: {}", e))?;

    config_from_toml(&data)
}

/// Save configuration to file.
pub fn save_config(config: &AppConfig, path: Option<&Path>) -> Result<(), String> {
    let config_path = path.unwrap_or(Path::new(CONFIG_PATH));

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    let data = config_to_toml(config)?;

    std::fs::write(config_path, data).map_err(|e| format!("Failed to write config file: {}", e))?;

    Ok(())
}

/// Backwards-compatible alias.  Prefer [`AppConfig`] directly.
pub type TomlAppConfig = AppConfig;
