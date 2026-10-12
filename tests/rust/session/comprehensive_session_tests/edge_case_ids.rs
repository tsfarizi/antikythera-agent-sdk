

#[test]
/// ERROR HANDLING & EDGE CASES.
///
fn test_duplicate_session_creation_with_same_id() {
    let manager = SessionManager::new();

    let id1 = manager.create_session_with_id("id-123", "user-1", "gpt-4").unwrap();
    let id2 = manager.create_session_with_id("id-123", "user-2", "gpt-3").unwrap();

    assert_eq!(id1, id2);

    let session = manager.get_session("id-123").unwrap().unwrap();
    assert_eq!(session.user_id, "user-2");
    assert_eq!(session.model, "gpt-3");
}

#[test]
/// Some operations with empty session ID.
///
fn test_empty_session_id_handling() {
    let manager = SessionManager::new();

    let result = manager.add_message("", Message::user("test"));
    assert!(result.is_err());

    let session = manager.get_session("").unwrap();
    assert!(session.is_none());
}

#[test]
fn test_very_long_session_id() {
    let manager = SessionManager::new();
    let long_id = "s".repeat(100_000);

    let id = manager.create_session_with_id(&long_id, "user", "gpt-4").unwrap();
    assert_eq!(id, long_id);
    assert!(manager.has_session(&long_id).unwrap());
}

#[test]
fn test_unicode_session_id() {
    let manager = SessionManager::new();
    let unicode_id = "session-\u{1f680}-\u{65e5}\u{672c}\u{8a9e}-\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629}";

    let id = manager.create_session_with_id(unicode_id, "user", "gpt-4").unwrap();
    assert_eq!(id, unicode_id);
    assert!(manager.has_session(unicode_id).unwrap());
}
