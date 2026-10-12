use super::*;

#[test]
fn clean_string_unchanged() {
    assert_eq!(sanitize_for_toml("hello world"), "hello world");
}

#[test]
fn newlines_replaced_with_space() {
    assert_eq!(sanitize_for_toml("hello\nworld"), "hello world");
    assert_eq!(sanitize_for_toml("a\r\nb"), "a b");
}

#[test]
fn backslashes_escaped() {
    assert_eq!(sanitize_for_toml("a\\b"), "a\\\\b");
}

#[test]
fn quotes_escaped() {
    assert_eq!(sanitize_for_toml(r#"say "hi""#), r#"say \"hi\""#);
}

#[test]
fn needs_sanitization_detects_unsafe_chars() {
    assert!(needs_sanitization("has\nnewline"));
    assert!(needs_sanitization("has\"quote"));
    assert!(needs_sanitization("has\\backslash"));
    assert!(!needs_sanitization("safe string"));
}
