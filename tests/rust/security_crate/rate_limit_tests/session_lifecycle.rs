#[test]
fn get_usage_reports_counts() {
    let limiter = RateLimiter::from_config();
    limiter.check("s5").unwrap();
    limiter.check("s5").unwrap();
    let usage = limiter.get_usage("s5").unwrap();
    assert_eq!(usage.requests_per_minute, 2);
}

#[test]
fn reset_session_clears_counters() {
    let limiter = RateLimiter::from_config();
    limiter.check("s6").unwrap();
    limiter.reset_session("s6");
    let usage = limiter.get_usage("s6").unwrap();
    assert_eq!(usage.requests_per_minute, 0);
}

#[test]
fn remove_session_decreases_count() {
    let limiter = RateLimiter::from_config();
    limiter.check("s7").unwrap();
    assert_eq!(limiter.active_session_count(), 1);
    limiter.remove_session("s7");
    assert_eq!(limiter.active_session_count(), 0);
}
