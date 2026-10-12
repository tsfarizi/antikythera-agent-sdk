//! Security configuration tests

use antikythera_core::security::config::{
    RateLimitConfig, SecretMetadata, SecretsConfig, SecurityConfig, ValidationConfig,
};

#[test]
fn test_security_config_default() {
    let config = SecurityConfig::default();
    assert!(config.validation.max_input_size_bytes > 0);
    assert!(config.rate_limit.enabled);
    assert!(config.secrets.enabled);
}

#[test]
fn test_validation_config_default() {
    let config = ValidationConfig::default();
    assert_eq!(config.max_input_size_bytes, 10 * 1024 * 1024);
    assert_eq!(config.max_message_length, 100_000);
    assert!(config.sanitize_html);
    assert!(config.validate_json_schema);
}

#[test]
fn test_rate_limit_config_default() {
    let config = RateLimitConfig::default();
    assert!(config.enabled);
    assert_eq!(config.requests_per_minute, 60);
    assert_eq!(config.requests_per_hour, 1000);
    assert_eq!(config.requests_per_day, 10_000);
}

#[test]
fn test_secrets_config_default() {
    let config = SecretsConfig::default();
    assert!(config.enabled);
    assert!(config.encrypt_at_rest);
    assert_eq!(config.encryption_algorithm, "AES256-GCM");
    assert_eq!(config.key_derivation_function, "Argon2");
}

#[test]
fn test_secret_metadata_creation() {
    let metadata = SecretMetadata::new("test-id".to_string(), 1);
    assert_eq!(metadata.id, "test-id");
    assert_eq!(metadata.version, 1);
    assert!(metadata.active);
}

#[test]
fn test_secret_metadata_expiration() {
    let mut metadata = SecretMetadata::new("test-id".to_string(), 1);
    metadata.expires_at = 0;

    assert!(metadata.is_expired());
}

#[test]
/// Should need rotation with 1 hour max age.
///
fn test_secret_metadata_needs_rotation() {
    let mut metadata = SecretMetadata::new("test-id".to_string(), 1);
    metadata.last_rotated_at = 0;

    assert!(metadata.needs_rotation(1));
}
