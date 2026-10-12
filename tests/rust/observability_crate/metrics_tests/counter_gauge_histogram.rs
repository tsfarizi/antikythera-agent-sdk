#[test]
fn export_counter_collects_records() {
    let exporter = InMemoryMetricsExporter::new();
    exporter.export_counter("tool.calls", 1.0, Default::default());
    exporter.export_counter("tool.calls", 2.0, Default::default());
    let snapshot = exporter.snapshot();
    assert_eq!(snapshot.len(), 2);
    assert!(snapshot.iter().all(|r| r.kind == MetricKind::Counter));
}

#[test]
fn export_gauge_collects_records() {
    let exporter = InMemoryMetricsExporter::new();
    exporter.export_gauge("memory.usage", 512.0, Default::default());
    let snapshot = exporter.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].kind, MetricKind::Gauge);
}

#[test]
fn export_histogram_collects_records() {
    let exporter = InMemoryMetricsExporter::new();
    exporter.export_histogram("latency", 120.0, Default::default());
    let snapshot = exporter.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].kind, MetricKind::Histogram);
}
