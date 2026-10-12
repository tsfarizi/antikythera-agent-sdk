#[tokio::test]
async fn port_trait_record_metric_works() {
    let exporter = InMemoryMetricsExporter::new();
    antikythera_ports::MetricsExporter::record_metric(
        &exporter,
        "test.metric",
        "counter",
        42.0,
        vec![("env".into(), "test".into())],
    ).await;
    let snapshot = exporter.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].name, "test.metric");
}

#[tokio::test]
async fn port_trait_flush_succeeds() {
    let exporter = InMemoryMetricsExporter::new();
    assert!(antikythera_ports::MetricsExporter::flush(&exporter).await.is_ok());
}
