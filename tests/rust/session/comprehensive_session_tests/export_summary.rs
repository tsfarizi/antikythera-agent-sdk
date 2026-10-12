

#[test]
fn test_session_summary_creation() {
    let manager = SessionManager::new();
    let id = manager.create_session("user-123", "gpt-4").unwrap();

    let session = manager.get_session(&id).unwrap().unwrap();
    let summary = SessionSummary::from(&session);

    assert_eq!(summary.id, session.id);
    assert_eq!(summary.user_id, "user-123");
    assert_eq!(summary.model, "gpt-4");
}

#[test]
fn test_session_export_creation() {
    let manager = SessionManager::new();
    let id = manager.create_session("user-123", "gpt-4").unwrap();

    let _ = manager.add_message(&id, Message::user("Hello"));
    let _ = manager.add_message(&id, Message::assistant("Hi!"));

    let session = manager.get_session(&id).unwrap().unwrap();
    let export = SessionExport::from_session(session);

    assert_eq!(export.session.messages.len(), 2);
}

#[test]
fn test_session_export_with_unicode() {
    let mut session = Session::new("user", "gpt-4");
    session.add_message(Message::user("\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}"));
    session.add_message(Message::assistant("\u{1f31f} Bonjour"));

    let export = SessionExport::from_session(session);
    assert_eq!(export.session.messages.len(), 2);
}
