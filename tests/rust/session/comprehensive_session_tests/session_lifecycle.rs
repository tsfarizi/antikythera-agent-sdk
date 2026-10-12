

#[test]
fn test_session_creation() {
    let session = Session::new("user-123", "gpt-4");

    assert_eq!(session.user_id, "user-123");
    assert_eq!(session.model, "gpt-4");
    assert_eq!(session.messages.len(), 0);
    assert!(!session.id.is_empty());
}

#[test]
fn test_session_with_id() {
    let mut session = Session::new("user-123", "gpt-4");
    session.id = "custom-id-123".to_string();

    assert_eq!(session.id, "custom-id-123");
}

#[test]
fn test_session_with_title() {
    let mut session = Session::new("user-123", "gpt-4");
    session.title = Some("Weather Discussion".to_string());

    assert_eq!(session.title, Some("Weather Discussion".to_string()));
}

#[test]
fn test_session_add_message() {
    let mut session = Session::new("user-123", "gpt-4");

    session.add_message(Message::user("Hello"));
    assert_eq!(session.messages.len(), 1);

    session.add_message(Message::assistant("Hi there!"));
    assert_eq!(session.messages.len(), 2);
}

#[test]
fn test_session_message_ordering() {
    let mut session = Session::new("user-123", "gpt-4");

    let msg1 = Message::user("First");
    let msg2 = Message::assistant("Second");
    let msg3 = Message::user("Third");

    session.add_message(msg1.clone());
    session.add_message(msg2.clone());
    session.add_message(msg3.clone());

    assert_eq!(session.messages[0].content, "First");
    assert_eq!(session.messages[1].content, "Second");
    assert_eq!(session.messages[2].content, "Third");
}

#[test]
fn test_session_empty_user_id() {
    let session = Session::new("", "gpt-4");
    assert_eq!(session.user_id, "");
}

#[test]
fn test_session_unicode_model_name() {
    let session = Session::new("user", "GPT-4_\u{65e5}\u{672c}\u{8a9e}_\u{1f680}");
    assert_eq!(session.model, "GPT-4_\u{65e5}\u{672c}\u{8a9e}_\u{1f680}");
}
