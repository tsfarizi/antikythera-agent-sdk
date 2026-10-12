#[test]
fn tracing_hook_tracks_start_and_end_lifecycle() {
    let hook = InMemoryTracingHook::new();
    let span = TraceSpanContext::new("trace-a", "span-a", "model_request")
        .with_parent("root-span")
        .with_correlation_id("corr-a");

    hook.on_span_start(span.clone());
    hook.on_span_end(span.clone(), TraceStatus::Ok);

    assert_eq!(hook.started_spans().len(), 1);
    assert_eq!(hook.ended_spans().len(), 1);
    assert_eq!(hook.ended_spans()[0].1, TraceStatus::Ok);
}
