fn make_msg(role: MessageRole, text: &str) -> ChatMessage {
    ChatMessage::new(role, text)
}


#[test]
fn estimate_text_is_non_zero_for_non_empty_input() {
    assert!(TokenEstimator::estimate_text("hello world") > 0);
}

#[test]
/// "hi" is 2 chars; 2/4 = 0, should return at least 1.
///
fn estimate_text_minimum_is_one_for_short_strings() {
    assert_eq!(TokenEstimator::estimate_text("hi"), 1);
}

#[test]
fn estimate_text_scales_with_length() {
    let short = TokenEstimator::estimate_text("hi");
    let long = TokenEstimator::estimate_text(&"a".repeat(1_000));
    assert!(long > short);
}

#[test]
/// Role overhead is 4 tokens.
///
fn estimate_message_includes_role_overhead() {
    let msg = make_msg(MessageRole::User, "hello");
    let content_tokens = TokenEstimator::estimate_text("hello");
    assert_eq!(TokenEstimator::estimate_message(&msg), content_tokens + 4);
}

#[test]
fn estimate_messages_sums_individual_estimates() {
    let msgs = vec![
        make_msg(MessageRole::User, "hello"),
        make_msg(MessageRole::Assistant, "world"),
    ];
    let total = TokenEstimator::estimate_messages(&msgs);
    let expected =
        TokenEstimator::estimate_message(&msgs[0]) + TokenEstimator::estimate_message(&msgs[1]);
    assert_eq!(total, expected);
}


#[test]
fn message_budget_subtracts_response_reservation() {
    let policy = ContextWindowPolicy {
        max_tokens: 8_192,
        reserve_for_response: 1_024,
        min_history_messages: 2,
    };
    assert_eq!(policy.message_budget(), 7_168);
}

#[test]
fn message_budget_does_not_underflow() {
    let policy = ContextWindowPolicy {
        max_tokens: 100,
        reserve_for_response: 200,
        min_history_messages: 1,
    };
    assert_eq!(policy.message_budget(), 0);
}


#[test]
fn prune_returns_all_messages_when_within_budget() {
    let msgs = vec![
        make_msg(MessageRole::User, "hi"),
        make_msg(MessageRole::Assistant, "hello"),
    ];
    let policy = ContextWindowPolicy {
        max_tokens: 10_000,
        reserve_for_response: 100,
        min_history_messages: 1,
    };
    let pruned = prune_messages(&msgs, &policy);
    assert_eq!(pruned.len(), msgs.len());
}

#[test]
/// At least min_history_messages are kept.
///
/// Fewer messages than the original.
///
/// The most recent message must be retained.
///
fn prune_removes_oldest_non_system_messages_first() {
    let mut msgs = Vec::new();
    for i in 0..10 {
        let role = if i % 2 == 0 {
            MessageRole::User
        } else {
            MessageRole::Assistant
        };
        msgs.push(make_msg(role, &format!("message number {i}")));
    }
    let policy = ContextWindowPolicy {
        max_tokens: 100,
        reserve_for_response: 10,
        min_history_messages: 2,
    };
    let pruned = prune_messages(&msgs, &policy);

    assert!(pruned.len() >= policy.min_history_messages);
    assert!(pruned.len() <= msgs.len());
    let last_original = msgs.last().unwrap();
    let last_pruned = pruned.last().unwrap();
    assert_eq!(last_pruned.content(), last_original.content());
}

#[test]
fn prune_always_retains_system_messages() {
    let msgs = vec![
        make_msg(MessageRole::System, "You are a helpful assistant."),
        make_msg(MessageRole::User, "question one"),
        make_msg(MessageRole::Assistant, "answer one"),
    ];
    let policy = ContextWindowPolicy {
        max_tokens: 20,
        reserve_for_response: 5,
        min_history_messages: 1,
    };
    let pruned = prune_messages(&msgs, &policy);
    let has_system = pruned.iter().any(|m| m.role == MessageRole::System);
    assert!(has_system, "System message must always be retained");
}

#[test]
/// Budget so tight nothing fits, but min_history_messages = 2.
///
fn prune_guarantees_min_history_messages() {
    let msgs = vec![
        make_msg(MessageRole::User, &"a".repeat(500)),
        make_msg(MessageRole::Assistant, &"b".repeat(500)),
        make_msg(MessageRole::User, &"c".repeat(500)),
    ];
    let policy = ContextWindowPolicy {
        max_tokens: 5,
        reserve_for_response: 1,
        min_history_messages: 2,
    };
    let pruned = prune_messages(&msgs, &policy);
    let non_system_count = pruned
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .count();
    assert!(non_system_count >= policy.min_history_messages);
}
