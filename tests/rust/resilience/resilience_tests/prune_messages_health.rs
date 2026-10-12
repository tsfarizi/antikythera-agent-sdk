#[test]
/// At least min_history_messages non-system messages.
///
/// Newest message (last in original) must be in the output.
///
fn prune_messages_with_tight_budget_keeps_newest_messages() {
    let mut messages = Vec::new();
    messages.push(ChatMessage::new(MessageRole::System, "Be helpful."));
    for i in 0..8 {
        let role = if i % 2 == 0 {
            MessageRole::User
        } else {
            MessageRole::Assistant
        };
        messages.push(ChatMessage::new(role, format!("turn {i}")));
    }

    let policy = ContextWindowPolicy {
        max_tokens: 80,
        reserve_for_response: 20,
        min_history_messages: 2,
    };

    let pruned = prune_messages(&messages, &policy);

    assert!(pruned.iter().any(|m| m.role == MessageRole::System));
    let non_system: Vec<_> = pruned
        .iter()
        .filter(|m| m.role != MessageRole::System)
        .collect();
    assert!(non_system.len() >= policy.min_history_messages);
    let last_original = messages.last().unwrap();
    assert!(
        pruned
            .iter()
            .any(|m| m.content() == last_original.content())
    );
}



#[test]
fn health_tracker_aggregates_multiple_components() {
    let mut tracker = HealthTracker::new();
    for _ in 0..10 {
        tracker.record_success("llm-primary", 200);
    }
    tracker.record_success("tool-server", 50);
    tracker.record_failure("tool-server", "timeout");
    tracker.record_success("tool-server", 60);
    tracker.record_success("tool-server", 55);

    let primary = tracker.health_of("llm-primary").unwrap();
    assert_eq!(primary.status, HealthStatus::Healthy);

    let tool = tracker.health_of("tool-server").unwrap();
    assert_ne!(tool.status, HealthStatus::Healthy);

    assert_ne!(tracker.overall_status(), HealthStatus::Healthy);
}
