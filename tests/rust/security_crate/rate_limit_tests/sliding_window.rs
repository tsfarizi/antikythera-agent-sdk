#[test]
fn requests_within_limit_succeed() {
    let limiter = RateLimiter::from_config();
    for _ in 0..10 {
        assert!(limiter.check("s1").is_ok());
    }
}

#[test]
fn requests_exceeding_minute_limit_fail() {
    let config = RateLimitConfig {
        enabled: true,
        requests_per_minute: 3,
        requests_per_hour: 100,
        requests_per_day: 1000,
        max_concurrent_sessions: 10,
        window_size_secs: 60,
        burst_allowance: 0,
        cleanup_interval_secs: 300,
    };
    let limiter = RateLimiter::new(config);
    for _ in 0..3 {
        assert!(limiter.check("s2").is_ok());
    }
    assert!(matches!(limiter.check("s2"), Err(RateLimitError::LimitExceeded { .. })));
}

#[test]
fn disabled_limiter_allows_unlimited() {
    let config = RateLimitConfig { enabled: false, ..Default::default() };
    let limiter = RateLimiter::new(config);
    for _ in 0..1000 {
        assert!(limiter.check("s3").is_ok());
    }
}

#[test]
/// 2 base + 2 burst = 4 total.
///
fn burst_allowance_permits_extra_requests() {
    let config = RateLimitConfig {
        enabled: true,
        requests_per_minute: 2,
        requests_per_hour: 100,
        requests_per_day: 1000,
        max_concurrent_sessions: 10,
        window_size_secs: 60,
        burst_allowance: 2,
        cleanup_interval_secs: 300,
    };
    let limiter = RateLimiter::new(config);
    for _ in 0..4 {
        assert!(limiter.check("s4").is_ok());
    }
    assert!(limiter.check("s4").is_err());
}
