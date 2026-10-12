#[test]
fn caller_context_builder() {
    let ctx = CallerContext::new()
        .with_correlation_id("corr-123")
        .with_user_id("user-456")
        .with_tenant_id("tenant-789")
        .with_source("native-cli");

    assert_eq!(ctx.correlation_id, Some("corr-123".to_string()));
    assert_eq!(ctx.user_id, Some("user-456".to_string()));
    assert_eq!(ctx.tenant_id, Some("tenant-789".to_string()));
    assert_eq!(ctx.source, Some("native-cli".to_string()));
}

#[test]
fn telemetry_event_serialization() {
    let event = TelemetryEvent::new(
        "agent_step",
        Some("corr-123".to_string()),
        Some("sess-456".to_string()),
    )
    .with_attribute("agent_id".to_string(), serde_json::json!("agent-001"))
    .with_attribute("step_count".to_string(), serde_json::json!(5));

    let json = event.to_json().unwrap();
    assert!(json.contains("agent_step"));
    assert!(json.contains("corr-123"));
}

#[test]
fn caller_context_ensure_correlation_id_sets_value_once() {
    let mut ctx = CallerContext::new();
    let first = ctx.ensure_correlation_id();
    let second = ctx.ensure_correlation_id();

    assert_eq!(first, second);
    assert_eq!(ctx.correlation_id, Some(first));
}

#[test]
fn telemetry_event_metric_attributes_contains_core_fields() {
    let event = TelemetryEvent::new(
        "tool_call",
        Some("corr-1".to_string()),
        Some("sess-1".to_string()),
    );
    let attrs = event.metric_attributes();

    assert_eq!(attrs.get("event_type"), Some(&"tool_call".to_string()));
    assert_eq!(attrs.get("correlation_id"), Some(&"corr-1".to_string()));
}
