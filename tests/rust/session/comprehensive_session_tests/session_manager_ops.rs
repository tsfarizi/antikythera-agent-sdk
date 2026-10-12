

#[test]
/// SESSION MANAGER - BASIC OPERATIONS.
///
fn test_session_manager_create_session() {
    let manager = SessionManager::new();
    let session_id = manager.create_session("user-123", "gpt-4").unwrap();

    assert!(!session_id.is_empty());
    assert!(manager.has_session(&session_id).unwrap());
}

#[test]
fn test_session_manager_create_with_custom_id() {
    let manager = SessionManager::new();
    let custom_id = "my-custom-session-id";
    let session_id = manager.create_session_with_id(custom_id, "user-123", "gpt-4").unwrap();

    assert_eq!(session_id, custom_id);
    assert!(manager.has_session(custom_id).unwrap());
}

#[test]
fn test_session_manager_get_session() {
    let manager = SessionManager::new();
    let id = manager.create_session("user-123", "gpt-4").unwrap();

    let session = manager.get_session(&id).unwrap().unwrap();
    assert_eq!(session.user_id, "user-123");
    assert_eq!(session.model, "gpt-4");
}

#[test]
fn test_session_manager_get_nonexistent_session() {
    let manager = SessionManager::new();
    let session = manager.get_session("nonexistent").unwrap();

    assert!(session.is_none());
}

#[test]
fn test_session_manager_add_message() {
    let manager = SessionManager::new();
    let id = manager.create_session("user-123", "gpt-4").unwrap();

    let msg = Message::user("Hello");
    let result = manager.add_message(&id, msg);

    assert!(result.is_ok());

    let session = manager.get_session(&id).unwrap().unwrap();
    assert_eq!(session.messages.len(), 1);
}

#[test]
fn test_session_manager_add_message_to_nonexistent() {
    let manager = SessionManager::new();
    let msg = Message::user("Hello");

    let result = manager.add_message("nonexistent", msg);
    assert!(result.is_err());
}

#[test]
fn test_session_manager_list_sessions() {
    let manager = SessionManager::new();

    manager.create_session("user-1", "gpt-4").unwrap();
    manager.create_session("user-2", "gpt-3").unwrap();
    manager.create_session("user-3", "claude").unwrap();

    let summaries = manager.list_sessions().unwrap();
    assert_eq!(summaries.len(), 3);
}

#[test]
fn test_session_manager_session_count() {
    let manager = SessionManager::new();
    assert_eq!(manager.session_count().unwrap(), 0);

    manager.create_session("user-1", "gpt-4").unwrap();
    assert_eq!(manager.session_count().unwrap(), 1);

    manager.create_session("user-2", "gpt-3").unwrap();
    assert_eq!(manager.session_count().unwrap(), 2);
}
