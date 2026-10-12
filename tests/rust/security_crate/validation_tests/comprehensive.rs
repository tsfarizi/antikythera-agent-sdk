#[test]
fn comprehensive_validation_passes_clean_input() {
    let validator = InputValidator::from_config().unwrap();
    assert!(validator.validate("Normal message").is_ok());
}

#[test]
fn comprehensive_validation_fails_large_input() {
    let validator = InputValidator::from_config().unwrap();
    let large = "x".repeat(200_000);
    assert!(validator.validate(&large).is_err());
}

#[test]
fn comprehensive_validation_fails_blocked_keyword() {
    let validator = InputValidator::from_config().unwrap();
    assert!(validator.validate("Check out this <script>alert('xss')</script>").is_err());
}

#[test]
fn concurrent_calls_within_limit() {
    let validator = InputValidator::from_config().unwrap();
    assert!(matches!(validator.validate_concurrent_calls(3), ValidationResult::Valid));
}

#[test]
fn concurrent_calls_exceeds_limit() {
    let config = ValidationConfig { max_concurrent_tool_calls: 5, ..Default::default() };
    let validator = InputValidator::new(config).unwrap();
    assert!(matches!(validator.validate_concurrent_calls(5), ValidationResult::Invalid(_)));
}
