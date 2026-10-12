#[test]
fn allowed_urls_pass() {
    let validator = InputValidator::from_config().unwrap();
    for url in &["https://example.com", "https://api.example.com/v1/endpoint"] {
        assert!(matches!(validator.validate_url(url), ValidationResult::Valid), "URL {} should be valid", url);
    }
}

#[test]
fn blocked_urls_rejected() {
    let validator = InputValidator::from_config().unwrap();
    for url in &["file:///etc/passwd", "data:text/html,<script>alert('xss')</script>"] {
        assert!(matches!(validator.validate_url(url), ValidationResult::Invalid(_)), "URL {} should be blocked", url);
    }
}
