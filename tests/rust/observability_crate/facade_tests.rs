//! Observability crate integration tests — facade.

use antikythera_observability::ObservabilityFacade;

#[test]
fn facade_creates_with_defaults() {
    let facade = ObservabilityFacade::from_config().unwrap();
    assert!(facade.metrics().is_some());
    assert!(facade.tracer().is_some());
    assert!(facade.audit().is_some());
}

#[tokio::test]
async fn facade_metrics_port_trait_works() {
    let facade = ObservabilityFacade::from_config().unwrap();
    let metrics = facade.metrics().unwrap();
    antikythera_ports::MetricsExporter::record_metric(metrics, "test", "counter", 1.0, vec![])
        .await;
}
