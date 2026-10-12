#[test]
fn latency_tracker_computes_sla_percentiles() {
    let mut tracker = LatencyTracker::new();
    for value in [100.0, 120.0, 200.0, 220.0, 300.0] {
        tracker.record_ms(value);
    }

    let summary = tracker.summary();
    assert_eq!(summary.count, 5);
    assert_eq!(summary.p50_ms, 200.0);
    assert_eq!(summary.p95_ms, 300.0);
    assert_eq!(summary.p99_ms, 300.0);
}
