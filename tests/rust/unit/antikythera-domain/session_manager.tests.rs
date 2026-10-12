use super::*;

#[test]
fn create_and_get_session() {
    let mgr = SessionManager::new();
    let id = mgr.create_session("user-1", "gpt-4").unwrap();
    let session = mgr.get_session(&id).unwrap().expect("session should exist");
    assert_eq!(session.user_id, "user-1");
    assert_eq!(session.model, "gpt-4");
}

#[test]
fn delete_session_removes_it() {
    let mgr = SessionManager::new();
    let id = mgr.create_session("u", "m").unwrap();
    assert!(mgr.has_session(&id).unwrap());
    mgr.delete_session(&id).unwrap();
    assert!(!mgr.has_session(&id).unwrap());
}

#[test]
fn delete_nonexistent_session_returns_error() {
    let mgr = SessionManager::new();
    let result = mgr.delete_session("nope");
    assert!(matches!(
        result,
        Err(SessionManagerError::SessionNotFound(_))
    ));
}

#[test]
fn add_message_to_session() {
    let mgr = SessionManager::new();
    let id = mgr.create_session("u", "m").unwrap();
    mgr.add_message(&id, Message::user("hi")).unwrap();
    let history = mgr.get_chat_history(&id).unwrap();
    assert_eq!(history.len(), 1);
}

#[test]
fn add_message_to_nonexistent_session_returns_error() {
    let mgr = SessionManager::new();
    let result = mgr.add_message("nope", Message::user("hi"));
    assert!(matches!(
        result,
        Err(SessionManagerError::SessionNotFound(_))
    ));
}

#[test]
fn session_count_tracks_creates_and_deletes() {
    let mgr = SessionManager::new();
    assert_eq!(mgr.session_count().unwrap(), 0);
    let id1 = mgr.create_session("u", "m").unwrap();
    let _id2 = mgr.create_session("u", "m").unwrap();
    assert_eq!(mgr.session_count().unwrap(), 2);
    mgr.delete_session(&id1).unwrap();
    assert_eq!(mgr.session_count().unwrap(), 1);
}

#[test]
fn create_session_with_custom_id() {
    let mgr = SessionManager::new();
    let id = mgr.create_session_with_id("custom-id", "u", "m").unwrap();
    assert_eq!(id, "custom-id");
    let session = mgr.get_session("custom-id").unwrap().unwrap();
    assert_eq!(session.id, "custom-id");
}

#[test]
fn import_session_and_list() {
    let mgr = SessionManager::new();
    let mut session = Session::new("u", "m");
    session.set_title("Imported");
    mgr.import_session(session.clone()).unwrap();
    let list = mgr.list_sessions().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].title.as_deref(), Some("Imported"));
}
