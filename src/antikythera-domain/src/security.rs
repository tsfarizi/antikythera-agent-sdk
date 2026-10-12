//! Security Configuration Types
//!
//! All security parameters are configurable and accessible via FFI.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Security configuration for the entire application
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SecurityConfig {
    /// Input validation settings
    pub validation: ValidationConfig,
    /// Rate limiting settings
    pub rate_limit: RateLimitConfig,
    /// Secrets management settings
    pub secrets: SecretsConfig,
}

/// Input validation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    /// Maximum input size in bytes
    pub max_input_size_bytes: u64,
    /// Maximum message length in characters
    pub max_message_length: usize,
    /// Maximum number of concurrent tool calls
    pub max_concurrent_tool_calls: u32,
    /// Allowed URL patterns (regex patterns)
    pub allowed_url_patterns: Vec<String>,
    /// Blocked URL patterns (regex patterns)
    pub blocked_url_patterns: Vec<String>,
    /// Enable HTML sanitization
    pub sanitize_html: bool,
    /// Enable JSON schema validation
    pub validate_json_schema: bool,
    /// Maximum nesting depth for JSON structures
    pub max_json_nesting_depth: u32,
    /// Maximum array length in JSON
    pub max_json_array_length: u32,
    /// Blocked keywords in input
    pub blocked_keywords: Vec<String>,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_input_size_bytes: 10 * 1024 * 1024,
            max_message_length: 100_000,
            max_concurrent_tool_calls: 10,
            allowed_url_patterns: vec![r"^https?://[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}.*$".to_string()],
            blocked_url_patterns: vec![r"^file://.*$".to_string(), r"^data:.*$".to_string()],
            sanitize_html: true,
            validate_json_schema: true,
            max_json_nesting_depth: 10,
            max_json_array_length: 1000,
            blocked_keywords: vec![
                "<script".to_string(),
                "javascript:".to_string(),
                "eval(".to_string(),
            ],
        }
    }
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitConfig {
    /// Enable rate limiting
    pub enabled: bool,
    /// Maximum requests per minute per session
    pub requests_per_minute: u32,
    /// Maximum requests per hour per session
    pub requests_per_hour: u32,
    /// Maximum requests per day per session
    pub requests_per_day: u32,
    /// Maximum concurrent sessions per user
    pub max_concurrent_sessions: u32,
    /// Rate limit window size in seconds
    pub window_size_secs: u32,
    /// Burst allowance (number of requests allowed above limit)
    pub burst_allowance: u32,
    /// Rate limit cleanup interval in seconds
    pub cleanup_interval_secs: u32,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            requests_per_minute: 60,
            requests_per_hour: 1000,
            requests_per_day: 10_000,
            max_concurrent_sessions: 5,
            window_size_secs: 60,
            burst_allowance: 10,
            cleanup_interval_secs: 300,
        }
    }
}

/// Secrets management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    /// Enable secrets management
    pub enabled: bool,
    /// Enable encryption at rest
    pub encrypt_at_rest: bool,
    /// Encryption algorithm (e.g., "AES256-GCM")
    pub encryption_algorithm: String,
    /// Key derivation function (e.g., "PBKDF2", "Argon2")
    pub key_derivation_function: String,
    /// Key derivation iterations
    pub key_derivation_iterations: u32,
    /// Enable automatic secret rotation
    pub auto_rotate: bool,
    /// Secret rotation interval in hours
    pub rotation_interval_hours: u32,
    /// Maximum secret age in hours before forced rotation
    pub max_secret_age_hours: u32,
    /// Secret storage backend (e.g., "memory", "file", "system")
    pub storage_backend: String,
    /// Custom storage path (for file backend)
    pub storage_path: Option<String>,
    /// Enable secret versioning
    pub enable_versioning: bool,
    /// Maximum number of secret versions to keep
    pub max_versions: u32,
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            encrypt_at_rest: true,
            encryption_algorithm: "AES256-GCM".to_string(),
            key_derivation_function: "Argon2".to_string(),
            key_derivation_iterations: 100_000,
            auto_rotate: false,
            rotation_interval_hours: 720,
            max_secret_age_hours: 2160,
            storage_backend: "memory".to_string(),
            storage_path: None,
            enable_versioning: true,
            max_versions: 5,
        }
    }
}

/// Secret metadata for tracking and rotation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretMetadata {
    /// Secret identifier
    pub id: String,
    /// Secret version
    pub version: u32,
    /// Creation timestamp (Unix epoch)
    pub created_at: u64,
    /// Last rotation timestamp (Unix epoch)
    pub last_rotated_at: u64,
    /// Expiry timestamp (Unix epoch)
    pub expires_at: u64,
    /// Whether the secret is active
    pub active: bool,
    /// Custom metadata
    pub metadata: HashMap<String, String>,
}

impl SecretMetadata {
    pub fn new(id: String, version: u32) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id,
            version,
            created_at: now,
            last_rotated_at: now,
            expires_at: now + (90 * 24 * 60 * 60),
            active: true,
            metadata: HashMap::new(),
        }
    }

    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now >= self.expires_at
    }

    pub fn needs_rotation(&self, max_age_hours: u32) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let max_age_secs = max_age_hours as u64 * 3600;
        (now - self.last_rotated_at) >= max_age_secs
    }
}

#[cfg(test)]
#[path = "../../../tests/rust/unit/antikythera-domain/security.tests.rs"]
mod tests;
