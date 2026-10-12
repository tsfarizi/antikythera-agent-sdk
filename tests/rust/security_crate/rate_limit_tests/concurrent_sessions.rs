#[test]
fn concurrent_session_limit_enforced() {
    let config = RateLimitConfig {
        enabled: true,
        requests_per_minute: 100,
        requests_per_hour: 1000,
        requests_per_day: 10000,
        max_concurrent_sessions: 2,
        window_size_secs: 60,
        burst_allowance: 0,
        cleanup_interval_secs: 300,
    };
    let limiter = RateLimiter::new(config);
    limiter.check("s8").unwrap();
    limiter.check("s9").unwrap();
    assert!(matches!(limiter.check("s10"), Err(RateLimitError::TooManyConcurrentSessions { .. })));
}

#[test]
/// Same session should not be blocked.
///
fn existing_session_not_blocked_by_concurrent_limit() {
    let config = RateLimitConfig {
        enabled: true,
        max_concurrent_sessions: 1,
        requests_per_minute: 100,
        requests_per_hour: 1000,
        requests_per_day: 10000,
        window_size_secs: 60,
        burst_allowance: 0,
        cleanup_interval_secs: 300,
    };
    let limiter = RateLimiter::new(config);
    limiter.check("s11").unwrap();
    assert!(limiter.check("s11").is_ok());
}
