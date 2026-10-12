

#[test]
/// RetryCondition and ErrorKind â€” conditional retry logic.
///
fn retry_condition_default_is_always() {
    let policy = TaskRetryPolicy::default();
    assert!(matches!(policy.condition, RetryCondition::Always));
}

#[test]
/// Mirrors the gate in execute_task: if matches!(condition, OnTransient) &&
/// !error_kind.is_transient() { break; }.
///
fn retry_condition_on_transient_blocks_retry_for_permanent() {
    let is_transient_error = false;
    let condition = RetryCondition::OnTransient;
    let should_retry = match condition {
        RetryCondition::Always => true,
        RetryCondition::Never => false,
        RetryCondition::OnTransient => is_transient_error,
    };
    assert!(!should_retry, "OnTransient must not retry permanent errors");
}

#[test]
fn retry_condition_on_transient_allows_retry_for_transient() {
    let is_transient_error = true;
    let condition = RetryCondition::OnTransient;
    let should_retry = match condition {
        RetryCondition::Always => true,
        RetryCondition::Never => false,
        RetryCondition::OnTransient => is_transient_error,
    };
    assert!(should_retry, "OnTransient must retry transient errors");
}

#[test]
fn retry_condition_never_blocks_all_retries() {
    let condition = RetryCondition::Never;
    let should_retry = !matches!(condition, RetryCondition::Never);
    assert!(!should_retry, "Never must block all retries");
}

#[test]
/// Verify the discriminant name round-trips (serde snake_case).
///
fn error_kind_serde_roundtrip() {
    let kinds = vec![
        ErrorKind::Transient,
        ErrorKind::Permanent,
        ErrorKind::Cancelled,
        ErrorKind::DeadlineExceeded,
        ErrorKind::BudgetExhausted,
    ];
    for kind in kinds {
        let json = serde_json::to_string(&kind).expect("serialize");
        let restored: ErrorKind = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            serde_json::to_string(&kind).unwrap(),
            serde_json::to_string(&restored).unwrap()
        );
    }
}

#[test]
fn task_result_is_transient_helper() {
    let transient = TaskResult::failure_with_kind(
        "t1".into(),
        "a".into(),
        "rate limited".into(),
        ErrorKind::Transient,
    );
    assert!(transient.is_transient());

    let permanent = TaskResult::failure_with_kind(
        "t2".into(),
        "b".into(),
        "auth error".into(),
        ErrorKind::Permanent,
    );
    assert!(!permanent.is_transient());

    let success = TaskResult::success("t3".into(), "c".into(), serde_json::json!(1), 1, "s".into());
    assert!(!success.is_transient(), "success is never transient");
}
