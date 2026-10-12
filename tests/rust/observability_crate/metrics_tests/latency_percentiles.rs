#[test]
fn latency_tracker_summary() {
    let mut tracker = LatencyTracker::new();
    tracker.record_ms(100.0);
    tracker.record_ms(200.0);
    tracker.record_ms(300.0);
    let summary = tracker.summary();
    assert_eq!(summary.count, 3);
    assert_eq!(summary.min_ms, 100.0);
    assert_eq!(summary.max_ms, 300.0);
    assert_eq!(summary.p50_ms, 200.0);
}

#[test]
fn latency_tracker_ignores_negative_and_nan() {
    let mut tracker = LatencyTracker::new();
    tracker.record_ms(-10.0);
    tracker.record_ms(f64::NAN);
    tracker.record_ms(50.0);
    assert_eq!(tracker.count(), 1);
}

#[test]
fn percentile_returns_zero_for_empty() {
    assert_eq!(percentile(&[], 0.95), 0.0);
}
