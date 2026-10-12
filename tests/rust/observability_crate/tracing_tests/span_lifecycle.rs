#[test]
fn span_start_records_in_started() {
    let hook = InMemoryTracingHook::new();
    let span = TraceSpanContext::new("trace-1", "span-1", "op");
    hook.on_span_start(span.clone());
    let started = hook.started_spans();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].name, "op");
}

#[test]
fn span_end_records_in_ended() {
    let hook = InMemoryTracingHook::new();
    let span = TraceSpanContext::new("trace-1", "span-1", "op");
    hook.on_span_end(span, TraceStatus::Ok);
    let ended = hook.ended_spans();
    assert_eq!(ended.len(), 1);
    assert_eq!(ended[0].1, TraceStatus::Ok);
}
