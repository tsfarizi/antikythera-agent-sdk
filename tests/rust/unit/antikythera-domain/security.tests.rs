use super::*;

#[test]
fn security_config_default_has_sensible_values() {
    let cfg = SecurityConfig::default();
    assert!(cfg.validation.sanitize_html);
    assert!(cfg.validation.validate_json_schema);
    assert!(cfg.validation.max_input_size_bytes > 0);
    assert!(cfg.rate_limit.enabled);
    assert!(cfg.rate_limit.requests_per_minute > 0);
    assert!(cfg.secrets.enabled);
}

#[test]
fn security_config_serialization_roundtrip() {
    let cfg = SecurityConfig::default();
    let json = serde_json::to_string(&cfg).unwrap();
    let restored: SecurityConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(
        restored.validation.max_input_size_bytes,
        cfg.validation.max_input_size_bytes
    );
    assert_eq!(
        restored.rate_limit.requests_per_minute,
        cfg.rate_limit.requests_per_minute
    );
}

#[test]
fn secret_metadata_new_has_correct_defaults() {
    let meta = SecretMetadata::new("test-key".to_string(), 1);
    assert_eq!(meta.id, "test-key");
    assert_eq!(meta.version, 1);
    assert!(meta.active);
    assert!(meta.expires_at > meta.created_at);
}

#[test]
fn secret_metadata_not_expired_by_default() {
    let meta = SecretMetadata::new("k".to_string(), 1);
    assert!(!meta.is_expired());
}

#[test]
fn secret_metadata_needs_rotation_with_zero_max_age() {
    let meta = SecretMetadata::new("k".to_string(), 1);
    assert!(meta.needs_rotation(0));
}
