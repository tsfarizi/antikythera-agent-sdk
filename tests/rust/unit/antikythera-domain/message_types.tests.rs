use super::*;

#[test]
fn message_role_as_str() {
    assert_eq!(MessageRole::User.as_str(), "user");
    assert_eq!(MessageRole::Assistant.as_str(), "assistant");
    assert_eq!(MessageRole::System.as_str(), "system");
    assert_eq!(MessageRole::ToolResult.as_str(), "tool_result");
}

#[test]
fn message_role_serialization_roundtrip() {
    let json = serde_json::to_string(&MessageRole::User).unwrap();
    let restored: MessageRole = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, MessageRole::User);
}

#[test]
fn message_part_text_as_text() {
    let p = MessagePart::text("hello");
    assert_eq!(p.as_text(), Some("hello"));
}

#[test]
fn message_part_image_as_text_returns_none() {
    let p = MessagePart::image("image/png", "data");
    assert!(p.as_text().is_none());
}

#[test]
fn message_serialization_roundtrip() {
    let msg = Message::user("test content");
    let json = serde_json::to_string(&msg).unwrap();
    let restored: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.role, MessageRole::User);
    assert_eq!(restored.content, "test content");
    assert_eq!(restored.parts.len(), 1);
}

#[test]
fn message_user_factory() {
    let msg = Message::user("hi");
    assert_eq!(msg.role, MessageRole::User);
    assert_eq!(msg.content, "hi");
    assert!(msg.tool_name.is_none());
}

#[test]
fn message_to_json_and_from_json() {
    let msg = Message::assistant("reply");
    let json = msg.to_json().unwrap();
    let restored = Message::from_json(&json).unwrap();
    assert_eq!(restored.role, MessageRole::Assistant);
    assert_eq!(restored.content, "reply");
}
