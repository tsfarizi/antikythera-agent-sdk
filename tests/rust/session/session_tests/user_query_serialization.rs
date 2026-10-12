#[test]
fn test_get_sessions_by_user() {
    let manager = SessionManager::new();

    manager.create_session("alice", "gpt-4").unwrap();
    manager.create_session("alice", "gpt-3.5").unwrap();
    manager.create_session("bob", "gpt-4").unwrap();

    let alice_sessions = manager.get_sessions_by_user("alice").unwrap();
    assert_eq!(alice_sessions.len(), 2);

    let bob_sessions = manager.get_sessions_by_user("bob").unwrap();
    assert_eq!(bob_sessions.len(), 1);
}


#[test]
fn test_message_serialization() {
    let msg = Message::user("Test message").with_metadata(r#"{"key": "value"}"#);
    let json = msg.to_json().unwrap();
    let restored = Message::from_json(&json).unwrap();

    assert_eq!(restored.content, "Test message");
    assert_eq!(restored.metadata, Some(r#"{"key": "value"}"#.to_string()));
}


#[test]
fn test_session_summary() {
    let manager = SessionManager::new();
    let session_id = manager.create_session("user-123", "gpt-4").unwrap();

    manager
        .add_message(&session_id, Message::user("Hello"))
        .unwrap();
    manager.record_tool(&session_id, "get_weather", 1).unwrap();

    let summary = manager
        .get_session(&session_id)
        .unwrap()
        .map(|s| SessionSummary::from(&s));
    let summary = summary.unwrap();

    assert_eq!(summary.message_count, 1);
    assert_eq!(summary.total_steps, 1);
    assert_eq!(summary.tools_used, vec!["get_weather"]);
}
