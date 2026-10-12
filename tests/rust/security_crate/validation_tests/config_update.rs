#[test]
fn update_config_changes_limits() {
    let mut validator = InputValidator::from_config().unwrap();
    let new_config = ValidationConfig {
        max_input_size_bytes: 5 * 1024 * 1024,
        max_message_length: 50_000,
        ..Default::default()
    };
    validator.update_config(new_config).unwrap();
    assert_eq!(validator.config().max_input_size_bytes, 5 * 1024 * 1024);
    assert_eq!(validator.config().max_message_length, 50_000);
}

#[test]
fn invalid_regex_pattern_rejected() {
    let config = ValidationConfig {
        allowed_url_patterns: vec!["[invalid".to_string()],
        ..Default::default()
    };
    assert!(InputValidator::new(config).is_err());
}
