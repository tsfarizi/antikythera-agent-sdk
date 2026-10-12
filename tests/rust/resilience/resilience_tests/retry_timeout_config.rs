use std::time::Duration;

#[test]
fn delay_for_attempt_zero_equals_initial_delay() {
    let policy = RetryPolicy::default();
    assert_eq!(policy.delay_for_attempt(0), Duration::from_millis(200));
}

#[test]
fn delay_doubles_each_attempt_with_factor_two() {
    let policy = RetryPolicy::default();
    assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(400));
    assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(800));
}

#[test]
/// 5000 * 3 = 15000 > 7000 → should be capped at 7000.
///
fn delay_is_capped_at_max_delay() {
    let policy = RetryPolicy {
        initial_delay_ms: 5_000,
        max_delay_ms: 7_000,
        backoff_factor: 3.0,
        ..Default::default()
    };
    assert_eq!(policy.delay_for_attempt(1), Duration::from_millis(7_000));
}

#[test]
fn no_retry_policy_has_exactly_one_attempt() {
    assert_eq!(RetryPolicy::no_retry().max_attempts, 1);
}

#[test]
fn aggressive_policy_has_five_attempts() {
    assert_eq!(RetryPolicy::aggressive().max_attempts, 5);
}

#[test]
fn timeout_policy_duration_helpers_match_ms_fields() {
    let policy = TimeoutPolicy {
        llm_timeout_ms: 5_000,
        tool_timeout_ms: 2_000,
    };
    assert_eq!(policy.llm_duration(), Duration::from_secs(5));
    assert_eq!(policy.tool_duration(), Duration::from_secs(2));
}

#[test]
fn resilience_config_roundtrips_json() {
    let config = ResilienceConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let parsed: ResilienceConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.retry.max_attempts, config.retry.max_attempts);
    assert_eq!(parsed.timeout.llm_timeout_ms, config.timeout.llm_timeout_ms);
}
