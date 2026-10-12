#[test]
fn test_sanitize_removes_newlines() {
    let input = "Line 1\nLine 2\r\nLine 3";
    let result = sanitize_for_toml(input);
    assert_eq!(result, "Line 1 Line 2 Line 3");
}

#[test]
fn test_sanitize_escapes_quotes() {
    let input = r#"He said "hello""#;
    let result = sanitize_for_toml(input);
    assert_eq!(result, r#"He said \"hello\""#);
}

#[test]
fn test_sanitize_removes_emoji() {
    let input = "⚠️ Warning: Important message";
    let result = sanitize_for_toml(input);
    assert!(result.contains("Warning"));
    assert!(!result.contains("⚠"));
}

#[test]
fn test_sanitize_collapses_whitespace() {
    let input = "Multiple   spaces   here";
    let result = sanitize_for_toml(input);
    assert_eq!(result, "Multiple spaces here");
}

#[test]
fn test_sanitize_complex_description() {
    let input =
        "Membuat Surat SKTM.\n\n⚠️ INSTRUKSI:\n1. WAJIB tanyakan data\n2. DILARANG dummy";
    let result = sanitize_for_toml(input);
    assert!(!result.contains('\n'));
    assert!(!result.contains("⚠"));
    assert!(result.contains("INSTRUKSI"));
}

#[test]
fn test_needs_sanitization() {
    assert!(needs_sanitization("Has\nnewline"));
    assert!(needs_sanitization("Has⚠️emoji"));
    assert!(!needs_sanitization("Normal text"));
}

#[test]
fn test_clean_text_unchanged() {
    let input = "This is clean text without special chars";
    let result = sanitize_for_toml(input);
    assert_eq!(result, input);
}
