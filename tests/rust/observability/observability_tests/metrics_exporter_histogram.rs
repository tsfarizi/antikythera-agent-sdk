#[test]
fn in_memory_metric_exporter_records_histogram_metrics() {
    let exporter = InMemoryMetricsExporter::new();
    let mut attributes = HashMap::new();
    attributes.insert("component".to_string(), "agent".to_string());

    exporter.export_histogram("latency.ms", 153.0, attributes);

    let snapshot = exporter.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].name, "latency.ms");
}
