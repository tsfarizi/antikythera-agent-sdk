use super::*;
use crate::message_types::Message;

#[test]
fn session_new_initializes_correctly() {
    let s = Session::new("user-1", "gpt-4");
    assert!(!s.id.is_empty());
    assert_eq!(s.user_id, "user-1");
    assert_eq!(s.model, "gpt-4");
    assert!(s.title.is_none());
    assert!(s.messages.is_empty());
    assert_eq!(s.tokens_used, 0);
    assert_eq!(s.total_steps, 0);
    assert!(s.tools_used.is_empty());
}

#[test]
fn session_add_message_increments_count_and_updates_timestamp() {
    let mut s = Session::new("u", "m");
    let before = s.updated_at.clone();
    s.add_message(Message::user("hello"));
    assert_eq!(s.messages.len(), 1);
    assert!(s.updated_at >= before);
}

#[test]
fn session_clear_messages_resets_state() {
    let mut s = Session::new("u", "m");
    s.add_message(Message::user("a"));
    s.add_message(Message::assistant("b"));
    s.record_tool("search", 1);
    s.clear_messages();
    assert!(s.messages.is_empty());
    assert_eq!(s.total_steps, 0);
    assert!(s.tools_used.is_empty());
}

#[test]
fn session_record_tool_deduplicates() {
    let mut s = Session::new("u", "m");
    s.record_tool("search", 1);
    s.record_tool("search", 2);
    assert_eq!(s.tools_used.len(), 1);
    assert_eq!(s.total_steps, 2);
}

#[test]
fn session_summary_from_ref() {
    let mut s = Session::new("u", "m");
    s.set_title("My Session");
    s.add_message(Message::user("hi"));
    let summary = SessionSummary::from(&s);
    assert_eq!(summary.id, s.id);
    assert_eq!(summary.message_count, 1);
    assert_eq!(summary.title.as_deref(), Some("My Session"));
}

#[test]
fn session_serialization_roundtrip() {
    let mut s = Session::new("u", "m");
    s.add_message(Message::user("hello"));
    let json = serde_json::to_string(&s).unwrap();
    let restored: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.id, s.id);
    assert_eq!(restored.messages.len(), 1);
}
