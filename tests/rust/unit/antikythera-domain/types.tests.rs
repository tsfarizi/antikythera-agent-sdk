use super::*;

#[test]
fn chat_message_new_text_only() {
    let msg = ChatMessage::new(MessageRole::User, "hello");
    assert_eq!(msg.role, MessageRole::User);
    assert_eq!(msg.content(), "hello");
    assert!(!msg.has_attachments());
}

#[test]
fn chat_message_with_text_parts_concatenated() {
    let msg = ChatMessage::with_parts(
        MessageRole::Assistant,
        vec![MessagePart::text("a"), MessagePart::text("b")],
    );
    assert_eq!(msg.content(), "ab");
}

#[test]
fn chat_message_has_attachments_with_image() {
    let msg = ChatMessage::with_parts(
        MessageRole::User,
        vec![
            MessagePart::text("see"),
            MessagePart::image("image/png", "base64data"),
        ],
    );
    assert!(msg.has_attachments());
}

#[test]
fn chat_message_serialization_roundtrip() {
    let msg = ChatMessage::new(MessageRole::User, "test");
    let json = serde_json::to_string(&msg).unwrap();
    let restored: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.role, MessageRole::User);
    assert_eq!(restored.content(), "test");
}
