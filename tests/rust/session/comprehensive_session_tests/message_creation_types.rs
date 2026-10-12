

#[test]
fn test_message_user_creation() {
    let msg = Message::user("Hello, world!");

    assert_eq!(msg.role, MessageRole::User);
    assert_eq!(msg.content, "Hello, world!");
    assert_eq!(msg.tool_name, None);
    assert_eq!(msg.step, None);
}

#[test]
fn test_message_assistant_creation() {
    let msg = Message::assistant("I'm here to help!");

    assert_eq!(msg.role, MessageRole::Assistant);
    assert_eq!(msg.content, "I'm here to help!");
}

#[test]
fn test_message_system_creation() {
    let msg = Message::system("System initialized");

    assert_eq!(msg.role, MessageRole::System);
    assert_eq!(msg.content, "System initialized");
}

#[test]
fn test_message_tool_result_creation() {
    let args = serde_json::json!({ "city": "NYC" });
    let msg = Message::tool_result("get_weather", "72 deg F, sunny", Some(args.clone()), 1);

    assert_eq!(msg.role, MessageRole::ToolResult);
    assert_eq!(msg.content, "72 deg F, sunny");
    assert_eq!(msg.tool_name, Some("get_weather".to_string()));
    assert_eq!(msg.step, Some(1));
    assert!(msg.tool_args.is_some());
}

#[test]
fn test_message_with_metadata() {
    let msg = Message::user("test").with_metadata(r#"{"priority": "high"}"#);

    assert_eq!(msg.metadata, Some(r#"{"priority": "high"}"#.to_string()));
}

#[test]
fn test_message_empty_content() {
    let msg = Message::user("");
    assert_eq!(msg.content, "");
}

#[test]
fn test_message_very_long_content() {
    let long_content = "x".repeat(1_000_000);
    let msg = Message::user(&long_content);
    assert_eq!(msg.content, long_content);
}

#[test]
fn test_message_unicode_content() {
    let unicode_msgs = vec![
        "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\u{4e16}\u{754c}",
        "\u{4f60}\u{597d}\u{4e16}\u{754c}",
        "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627} \u{0628}\u{0627}\u{0644}\u{0639}\u{0627}\u{0644}\u{0645}",
        "\u{1f680} Rust \u{1f980}",
        "Mixed: English + \u{65e5}\u{672c}\u{8a9e} + \u{1f600}",
    ];

    for content in unicode_msgs {
        let msg = Message::user(content);
        assert_eq!(msg.content, content);
    }
}

#[test]
fn test_message_special_characters() {
    let special_contents = vec![
        "Quote: \"hello\"",
        "Newline: hello\nworld",
        "Tab: hello\tworld",
        "Backslash: \\path\\to\\file",
        "JSON: {\"key\": \"value\"}",
        "SQL: DROP TABLE users;",
    ];

    for content in special_contents {
        let msg = Message::user(content);
        assert_eq!(msg.content, content);
    }
}

#[test]
fn test_message_role_str_conversion() {
    assert_eq!(MessageRole::User.as_str(), "user");
    assert_eq!(MessageRole::Assistant.as_str(), "assistant");
    assert_eq!(MessageRole::System.as_str(), "system");
    assert_eq!(MessageRole::ToolResult.as_str(), "tool_result");
}

#[test]
fn test_message_tool_result_without_args() {
    let msg = Message::tool_result("get_time", "2024-04-20", None, 42);

    assert_eq!(msg.role, MessageRole::ToolResult);
    assert_eq!(msg.tool_name, Some("get_time".to_string()));
    assert_eq!(msg.tool_args, None);
    assert_eq!(msg.step, Some(42));
}
