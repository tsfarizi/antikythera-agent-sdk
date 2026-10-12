#[test]
fn test_create_session() {
    let manager = SessionManager::new();

    let session_id = manager.create_session("user-123", "gpt-4").unwrap();
    assert!(!session_id.is_empty());
    assert!(manager.has_session(&session_id).unwrap());
}


#[test]
fn test_add_message() {
    let manager = SessionManager::new();
    let session_id = manager.create_session("user-123", "gpt-4").unwrap();

    manager
        .add_message(&session_id, Message::user("Hello!"))
        .unwrap();
    manager
        .add_message(&session_id, Message::assistant("Hi there!"))
        .unwrap();

    let history = manager.get_chat_history(&session_id).unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].content, "Hello!");
    assert_eq!(history[1].content, "Hi there!");
}
