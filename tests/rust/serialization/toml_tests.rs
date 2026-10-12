//! Split into 6 parts for consistent test organization.
//!

/// Serialization tests - testing config TOML serialization Tests for converting AppConfig back to
/// TOML format.
///
use antikythera_core::config::AppConfig;
use antikythera_core::domain::sanitize::{needs_sanitization, sanitize_for_toml};

include!("toml_tests/toml_required_fields.rs");
include!("toml_tests/toml_prompt_template.rs");
include!("toml_tests/toml_system_prompt.rs");
include!("toml_tests/sanitize_toml_values.rs");
