#[tokio::test]
async fn port_trait_start_span_returns_id() {
    let hook = InMemoryTracingHook::new();
    let span_id = antikythera_ports::TracingHook::start_span(
        &hook,
        "test-span",
        vec![("k".into(), "v".into())],
    ).await;
    assert!(!span_id.is_empty());
    assert_eq!(hook.started_spans().len(), 1);
}

#[tokio::test]
async fn port_trait_end_span_records_status() {
    let hook = InMemoryTracingHook::new();
    let span_id = antikythera_ports::TracingHook::start_span(&hook, "s", vec![]).await;
    antikythera_ports::TracingHook::end_span(&hook, &span_id, "ok").await;
    assert_eq!(hook.ended_spans().len(), 1);
}
