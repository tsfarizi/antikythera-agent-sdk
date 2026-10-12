#[test]
fn telemetry_hook_records_external_event() {
    let sink = InMemoryTelemetryHook::new();
    let middleware =
        HostHookMiddleware::new(HookRegistry::new().with_telemetry_hook(Arc::new(sink.clone())));
    let context = HookContext::new(
        CallerContext::new().with_user_id("user-a"),
        HookOperation::AgentRun,
    )
    .with_correlation_id("corr-a")
    .with_session_id("sess-a");

    let event = TelemetryEvent {
        event_type: "agent_finished".to_string(),
        correlation_id: context.correlation_id.clone(),
        session_id: context.session_id.clone(),
        timestamp_ms: 1,
        attributes: HashMap::new(),
    };

    middleware
        .emit_event(&context, event)
        .expect("emit event should succeed");

    assert_eq!(sink.snapshot().len(), 1);
    assert_eq!(sink.snapshot()[0].event_type, "agent_finished");
}
