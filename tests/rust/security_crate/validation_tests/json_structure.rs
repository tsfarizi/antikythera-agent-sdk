#[test]
fn valid_json_passes() {
    let validator = InputValidator::from_config().unwrap();
    assert!(validator.validate_json(r#"{"key": "value"}"#).is_ok());
    assert!(validator.validate_json(r#"{"array": [1, 2, 3]}"#).is_ok());
}

#[test]
fn invalid_json_rejected() {
    let validator = InputValidator::from_config().unwrap();
    assert!(validator.validate_json(r#"not json"#).is_err());
    assert!(validator.validate_json(r#"{"key": "value""#).is_err());
}

#[test]
fn nesting_depth_exceeded() {
    let config = ValidationConfig { max_json_nesting_depth: 3, ..Default::default() };
    let validator = InputValidator::new(config).unwrap();
    assert!(validator.validate_json(r#"{"l1": {"l2": {"l3": "v"}}}"#).is_ok());
    assert!(validator.validate_json(r#"{"l1": {"l2": {"l3": {"l4": "v"}}}}"#).is_err());
}

#[test]
fn array_length_exceeded() {
    let config = ValidationConfig { max_json_array_length: 3, ..Default::default() };
    let validator = InputValidator::new(config).unwrap();
    assert!(validator.validate_json(r#"{"a": [1, 2]}"#).is_ok());
    assert!(validator.validate_json(r#"{"a": [1, 2, 3, 4]}"#).is_err());
}
