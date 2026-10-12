#[test]
fn truncation_strategy_default_is_keep_newest() {
    assert_eq!(
        TruncationStrategy::default(),
        TruncationStrategy::KeepNewest
    );
}

#[test]
fn context_policy_default_has_sensible_values() {
    let policy = ContextPolicy::default();
    assert_eq!(policy.max_history_messages, 50);
    assert_eq!(policy.truncation_strategy, TruncationStrategy::KeepNewest);
    assert_eq!(policy.min_system_messages, 1);
    assert_eq!(policy.token_budget, None);
}

#[test]
fn context_policy_fluent_builder_sets_values() {
    let policy = ContextPolicy::new()
        .with_max_history_messages(100)
        .with_token_budget(8000)
        .with_min_system_messages(2)
        .with_truncation_strategy(TruncationStrategy::KeepBalanced { head_ratio: 0.4 });

    assert_eq!(policy.max_history_messages, 100);
    assert_eq!(policy.token_budget, Some(8000));
    assert_eq!(policy.min_system_messages, 2);
    assert!(matches!(
        policy.truncation_strategy,
        TruncationStrategy::KeepBalanced { head_ratio: 0.4 }
    ));
}

#[test]
fn context_policy_serialization_roundtrip() {
    let policy = ContextPolicy::new()
        .with_max_history_messages(75)
        .with_truncation_strategy(TruncationStrategy::KeepBalanced { head_ratio: 0.35 })
        .with_token_budget(6000);

    let json = serde_json::to_string(&policy).expect("serialization failed");
    let deserialized: ContextPolicy =
        serde_json::from_str(&json).expect("deserialization failed");

    assert_eq!(deserialized.max_history_messages, 75);
    assert_eq!(deserialized.token_budget, Some(6000));
    assert!(matches!(
        deserialized.truncation_strategy,
        TruncationStrategy::KeepBalanced { head_ratio: 0.35 }
    ));
}
