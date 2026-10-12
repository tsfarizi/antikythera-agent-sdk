#[test]
fn clear_resets_records() {
    let exporter = InMemoryMetricsExporter::new();
    exporter.export_counter("x", 1.0, Default::default());
    exporter.clear();
    assert!(exporter.snapshot().is_empty());
}

#[test]
fn snapshot_returns_cloned_records() {
    let exporter = InMemoryMetricsExporter::new();
    exporter.export_counter("a", 1.0, Default::default());
    let snap1 = exporter.snapshot();
    let snap2 = exporter.snapshot();
    assert_eq!(snap1.len(), snap2.len());
}
