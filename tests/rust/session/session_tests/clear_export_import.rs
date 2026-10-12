#[test]
fn test_clear_session() {
    let manager = SessionManager::new();
    let session_id = manager.create_session("user-123", "gpt-4").unwrap();

    manager
        .add_message(&session_id, Message::user("Hello!"))
        .unwrap();
    manager
        .add_message(&session_id, Message::assistant("Hi!"))
        .unwrap();

    let history = manager.get_chat_history(&session_id).unwrap();
    assert_eq!(history.len(), 2);

    manager.clear_session(&session_id).unwrap();
    let history = manager.get_chat_history(&session_id).unwrap();
    assert_eq!(history.len(), 0);
}


#[test]
fn test_export_import_json() {
    let manager = SessionManager::new();
    let session_id = manager.create_session("user-123", "gpt-4").unwrap();

    manager
        .add_message(&session_id, Message::user("Test"))
        .unwrap();
    manager
        .add_message(&session_id, Message::assistant("Response"))
        .unwrap();

    let session = manager.get_session(&session_id).unwrap().unwrap();
    let export = SessionExport::from_session(session);
    let json_data = export.to_json().unwrap();

    let imported = SessionExport::from_json(&json_data).unwrap();
    assert_eq!(imported.session.user_id, "user-123");
    assert_eq!(imported.session.model, "gpt-4");
    assert_eq!(imported.session.messages.len(), 2);
}
