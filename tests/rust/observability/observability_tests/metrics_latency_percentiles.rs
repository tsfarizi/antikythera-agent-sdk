#[test]
fn in_memory_metrics_exporter_collects_counter_records() {
    let exporter = InMemoryMetricsExporter::new();
    exporter.export_counter("tool.calls", 1.0, Default::default());
    exporter.export_counter("tool.calls", 2.0, Default::default());

    let snapshot = exporter.snapshot();
    assert_eq!(snapshot.len(), 2);
    assert!(snapshot.iter().all(|r| r.kind == MetricKind::Counter));
}

#[test]
fn metrics_exporter_clear_resets_records() {
    let exporter = InMemoryMetricsExporter::new();
    exporter.export_counter("x", 1.0, Default::default());
    exporter.clear();
    assert!(exporter.snapshot().is_empty());
}

#[test]
fn latency_tracker_summary_reports_percentiles() {
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
fn latency_tracker_ignores_negative_and_nan_values() {
    let mut tracker = LatencyTracker::new();
    tracker.record_ms(-10.0);
    tracker.record_ms(f64::NAN);
    tracker.record_ms(50.0);

    assert_eq!(tracker.count(), 1);
}

#[test]
fn percentile_returns_zero_for_empty_samples() {
    assert_eq!(percentile(&[], 0.95), 0.0);
}
