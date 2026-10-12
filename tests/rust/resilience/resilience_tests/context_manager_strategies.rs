fn make_message(role: MessageRole, content: &str) -> ChatMessage {
    ChatMessage::new(role, content)
}

#[test]
/// System message should be preserved.
///
fn context_manager_preserves_system_messages() {
    let manager = RuntimeContextManager::new(ContextPolicy::default());
    let messages = vec![
        make_message(MessageRole::System, "You are helpful"),
        make_message(MessageRole::User, "Hello"),
        make_message(MessageRole::Assistant, "Hi there"),
    ];

    let result = manager
        .apply_policy(&messages)
        .expect("apply_policy failed");

    assert!(result.iter().any(|m| m.role.as_str() == "system"));
}

#[test]
/// Should have system message + at most 5 user messages.
///
fn context_manager_respects_max_history_messages() {
    let policy = ContextPolicy::new().with_max_history_messages(5);
    let manager = RuntimeContextManager::new(policy);

    let mut messages = (0..15)
        .map(|i| make_message(MessageRole::User, &format!("msg {}", i)))
        .collect::<Vec<_>>();
    messages.insert(0, make_message(MessageRole::System, "sys"));

    let result = manager
        .apply_policy(&messages)
        .expect("apply_policy failed");

    assert!(result.iter().filter(|m| m.role.as_str() == "user").count() <= 5);
}

#[test]
/// Should keep the last 3 messages.
///
fn context_manager_keep_newest_discards_oldest() {
    let policy = ContextPolicy::new()
        .with_max_history_messages(3)
        .with_truncation_strategy(TruncationStrategy::KeepNewest);
    let manager = RuntimeContextManager::new(policy);

    let messages = vec![
        make_message(MessageRole::User, "msg 0"),
        make_message(MessageRole::User, "msg 1"),
        make_message(MessageRole::User, "msg 2"),
        make_message(MessageRole::User, "msg 3"),
        make_message(MessageRole::User, "msg 4"),
    ];

    let result = manager
        .apply_policy(&messages)
        .expect("apply_policy failed");

    assert_eq!(result.len(), 3);
    assert_eq!(result[0].content(), "msg 2");
    assert_eq!(result[2].content(), "msg 4");
}

#[test]
/// Should have ~3 from head and ~3 from tail.
///
fn context_manager_keep_balanced_retains_head_and_tail() {
    let policy = ContextPolicy::new()
        .with_max_history_messages(6)
        .with_truncation_strategy(TruncationStrategy::KeepBalanced { head_ratio: 0.5 });
    let manager = RuntimeContextManager::new(policy);

    let messages = (0..20)
        .map(|i| make_message(MessageRole::User, &format!("msg {}", i)))
        .collect::<Vec<_>>();

    let result = manager
        .apply_policy(&messages)
        .expect("apply_policy failed");

    assert_eq!(result.len(), 6);
    assert_eq!(result[0].content(), "msg 0");
    assert_eq!(result[5].content(), "msg 19");
}

#[test]
/// Token count should be below budget.
///
fn context_manager_respects_token_budget() {
    let policy = ContextPolicy::new()
        .with_max_history_messages(100)
        .with_token_budget(100);
    let manager = RuntimeContextManager::new(policy);

    let messages = (0..50)
        .map(|_| make_message(MessageRole::User, &"x".repeat(20)))
        .collect::<Vec<_>>();

    let result = manager
        .apply_policy(&messages)
        .expect("apply_policy failed");

    let tokens = result
        .iter()
        .map(|m| m.content().len())
        .sum::<usize>()
        .div_ceil(4);
    assert!(tokens <= 100);
}

#[test]
fn context_manager_cloneable() {
    let manager1 = RuntimeContextManager::new(ContextPolicy::default());
    let manager2 = manager1.clone();

    let messages = vec![make_message(MessageRole::User, "test")];
    let result1 = manager1
        .apply_policy(&messages)
        .expect("apply_policy failed");
    let result2 = manager2
        .apply_policy(&messages)
        .expect("apply_policy failed");

    assert_eq!(result1.len(), result2.len());
}
