#[test]
fn update_config_takes_effect() {
    let mut limiter = RateLimiter::from_config();
    assert!(limiter.config().enabled);
    let new_config = RateLimitConfig { enabled: false, ..Default::default() };
    limiter.update_config(new_config);
    assert!(!limiter.config().enabled);
}
