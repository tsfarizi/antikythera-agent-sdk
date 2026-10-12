

#[test]
/// Both managers should see the same session.
///
fn test_manager_clone_shares_data() {
    let manager1 = SessionManager::new();
    let id = manager1.create_session("user", "gpt-4").unwrap();

    let manager2 = manager1.clone();

    assert!(manager1.has_session(&id).unwrap());
    assert!(manager2.has_session(&id).unwrap());
}

#[test]
/// Session1 should still be empty.
///
fn test_session_clone_independence() {
    let session1 = Session::new("user", "gpt-4");
    let mut session2 = session1.clone();

    session2.add_message(Message::user("test"));

    assert_eq!(session1.messages.len(), 0);
    assert_eq!(session2.messages.len(), 1);
}
