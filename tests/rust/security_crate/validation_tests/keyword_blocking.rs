#[test]
fn blocked_keywords_detected() {
    let validator = InputValidator::from_config().unwrap();
    assert!(matches!(validator.check_blocked_keywords("<script>alert('xss')</script>"), ValidationResult::Invalid(_)));
    assert!(matches!(validator.check_blocked_keywords("javascript:void(0)"), ValidationResult::Invalid(_)));
    assert!(matches!(validator.check_blocked_keywords("eval(malicious)"), ValidationResult::Invalid(_)));
}

#[test]
fn safe_keywords_pass() {
    let validator = InputValidator::from_config().unwrap();
    assert!(matches!(validator.check_blocked_keywords("normal text"), ValidationResult::Valid));
    assert!(matches!(validator.check_blocked_keywords("This is safe"), ValidationResult::Valid));
}
