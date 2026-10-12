

#[test]
fn task_retry_policy_default_is_zero_retries() {
    let policy = TaskRetryPolicy::default();
    assert_eq!(policy.max_retries, 0);
    assert_eq!(policy.backoff_ms, 0);
}

#[test]
fn task_retry_policy_serde_roundtrip() {
    let policy = TaskRetryPolicy {
        max_retries: 5,
        backoff_ms: 1_000,
        ..TaskRetryPolicy::default()
    };
    let json = serde_json::to_string(&policy).expect("serialize");
    let restored: TaskRetryPolicy = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(restored.max_retries, 5);
    assert_eq!(restored.backoff_ms, 1_000);
}
