#[test]
fn validator_creation_uses_default_config() {
    let validator = InputValidator::from_config().unwrap();
    assert_eq!(validator.config().max_input_size_bytes, 10 * 1024 * 1024);
}

#[test]
fn validate_size_within_limit() {
    let validator = InputValidator::from_config().unwrap();
    assert!(matches!(validator.validate_size("small"), ValidationResult::Valid));
}

#[test]
fn validate_size_exceeds_limit() {
    let validator = InputValidator::from_config().unwrap();
    let large = "x".repeat(11 * 1024 * 1024);
    assert!(matches!(validator.validate_size(&large), ValidationResult::Invalid(_)));
}

#[test]
fn validate_message_length_within_limit() {
    let validator = InputValidator::from_config().unwrap();
    assert!(matches!(validator.validate_message_length(&"a".repeat(1000)), ValidationResult::Valid));
}

#[test]
fn validate_message_length_exceeds_limit() {
    let validator = InputValidator::from_config().unwrap();
    assert!(matches!(validator.validate_message_length(&"a".repeat(200_000)), ValidationResult::Invalid(_)));
}
