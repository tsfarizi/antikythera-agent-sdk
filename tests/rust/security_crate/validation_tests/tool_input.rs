#[test]
fn valid_tool_input_passes() {
    let validator = InputValidator::from_config().unwrap();
    let input = r#"{"param": "value", "url": "https://api.example.com"}"#;
    assert!(matches!(validator.validate_tool_input("test_tool", input), ValidationResult::Valid));
}

#[test]
fn malicious_tool_input_rejected() {
    let validator = InputValidator::from_config().unwrap();
    let input = r#"{"param": "<script>alert('xss')</script>"}"#;
    assert!(matches!(validator.validate_tool_input("test_tool", input), ValidationResult::Invalid(_)));
}
