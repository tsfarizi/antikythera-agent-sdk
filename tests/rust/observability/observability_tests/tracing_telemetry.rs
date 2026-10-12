#[test]
fn in_memory_tracing_hook_records_started_and_ended_spans() {
    let hook = InMemoryTracingHook::new();
    let span = TraceSpanContext::new("trace-1", "span-1", "tool_call")
        .with_correlation_id("corr-99")
        .with_parent("root-0")
        .with_attribute("tool", "search");

    hook.on_span_start(span.clone());
    hook.on_span_end(span.clone(), TraceStatus::Ok);

    let started = hook.started_spans();
    let ended = hook.ended_spans();
    assert_eq!(started.len(), 1);
    assert_eq!(ended.len(), 1);
    assert_eq!(started[0], span);
    assert_eq!(ended[0].1, TraceStatus::Ok);
}

#[test]
fn in_memory_telemetry_hook_as_observability() {
    let hook = InMemoryTelemetryHook::new();

    let event1 = TelemetryEvent::new("llm_request", None, Some("s1".to_string()));
    hook.record_event(event1);

    let event2 = TelemetryEvent::new("tool_call", None, Some("s1".to_string()));
    hook.record_event(event2);

    let snapshot = hook.snapshot();
    assert_eq!(snapshot.len(), 2);

    let llm_events = hook.events_by_type("llm_request");
    assert_eq!(llm_events.len(), 1);
}
