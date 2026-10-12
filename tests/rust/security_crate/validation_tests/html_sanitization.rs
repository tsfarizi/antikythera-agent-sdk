#[test]
fn sanitize_html_removes_script_tags() {
    let validator = InputValidator::from_config().unwrap();
    let html = "<script>alert('xss')</script><div onclick=\"alert('click')\">content</div>";
    let sanitized = validator.sanitize_html(html);
    assert!(!sanitized.contains("<script>"));
    assert!(!sanitized.contains("onclick="));
}

#[test]
fn sanitize_html_preserves_safe_content() {
    let validator = InputValidator::from_config().unwrap();
    let html = "<div>Safe content</div>";
    let sanitized = validator.sanitize_html(html);
    assert!(sanitized.contains("Safe content"));
}
