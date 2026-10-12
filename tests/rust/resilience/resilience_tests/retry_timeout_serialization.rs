#[test]
fn retry_policy_json_roundtrip_preserves_all_fields() {
    let policy = RetryPolicy {
        max_attempts: 7,
        initial_delay_ms: 500,
        max_delay_ms: 20_000,
        backoff_factor: 1.5,
    };
    let json = serde_json::to_string(&policy).unwrap();
    let parsed: RetryPolicy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.max_attempts, 7);
    assert_eq!(parsed.initial_delay_ms, 500);
    assert_eq!(parsed.max_delay_ms, 20_000);
    assert!((parsed.backoff_factor - 1.5).abs() < 1e-9);
}


#[test]
fn timeout_policy_durations_match_millisecond_fields() {
    let policy = TimeoutPolicy {
        llm_timeout_ms: 45_000,
        tool_timeout_ms: 8_000,
    };
    assert_eq!(policy.llm_duration().as_secs(), 45);
    assert_eq!(policy.tool_duration().as_secs(), 8);
}


#[tokio::test]
async fn with_retry_if_succeeds_after_transient_network_errors() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    let attempts = Arc::new(AtomicU32::new(0));
    let a = Arc::clone(&attempts);
    let policy = RetryPolicy {
        max_attempts: 5,
        initial_delay_ms: 1,
        max_delay_ms: 5,
        backoff_factor: 1.0,
    };

    let result: Result<String, String> = with_retry_if(
        &policy,
        || {
            let a = Arc::clone(&a);
            async move {
                let n = a.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err("network error".to_string())
                } else {
                    Ok("success".to_string())
                }
            }
        },
        |e| e.contains("network"),
    )
    .await;

    assert_eq!(result.unwrap(), "success");
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}
