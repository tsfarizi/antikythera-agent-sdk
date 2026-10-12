use super::*;

#[test]
fn test_new_with_string_literal() {
    let ctx = SessionContext::new("test-session");
    assert_eq!(ctx.session_id(), "test-session");
}

#[test]
fn test_new_with_string() {
    let ctx = SessionContext::new(String::from("dynamic-session"));
    assert_eq!(ctx.session_id(), "dynamic-session");
}

#[test]
/// Default should produce a non-empty session ID (from global fallback).
///
fn test_default_uses_global_fallback() {
    let ctx = SessionContext::default();
    assert!(!ctx.session_id().is_empty());
}

#[test]
fn test_or_active_returns_stored_id() {
    let ctx = SessionContext::new("my-session");
    let result = ctx.or_active();
    assert_eq!(result, "my-session");
}

#[test]
/// Empty string should trigger fallback to global.
///
fn test_or_active_empty_falls_back() {
    let ctx = SessionContext::new("");
    let result = ctx.or_active();
    assert!(!result.is_empty());
}

#[test]
fn test_into_session_id() {
    let ctx = SessionContext::new("consume-me");
    let owned = ctx.into_session_id();
    assert_eq!(owned, "consume-me");
}

#[test]
fn test_clone_independence() {
    let original = SessionContext::new("original");
    let cloned = original.clone();
    assert_eq!(original.session_id(), cloned.session_id());
}
