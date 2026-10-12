use super::*;

#[test]
fn caller_context_default() {
    let ctx = CallerContext::new();
    assert!(ctx.correlation_id.is_none());
    assert!(ctx.user_id.is_none());
}

#[test]
fn caller_context_builder() {
    let ctx = CallerContext::new()
        .with_correlation_id("corr-1")
        .with_user_id("user-1")
        .with_tenant_id("tenant-1")
        .with_source("cli")
        .with_metadata("key", "value");

    assert_eq!(ctx.correlation_id.as_deref(), Some("corr-1"));
    assert_eq!(ctx.user_id.as_deref(), Some("user-1"));
    assert_eq!(ctx.tenant_id.as_deref(), Some("tenant-1"));
    assert_eq!(ctx.source.as_deref(), Some("cli"));
    assert_eq!(ctx.metadata.as_ref().unwrap().get("key").unwrap(), "value");
}

#[test]
fn caller_context_json_roundtrip() {
    let ctx = CallerContext::new()
        .with_correlation_id("corr-1")
        .with_user_id("user-1");
    let json = ctx.to_json().unwrap();
    let restored = CallerContext::from_json(&json).unwrap();
    assert_eq!(ctx, restored);
}

#[test]
fn telemetry_event_builder() {
    let event = TelemetryEvent::new("tool_call", Some("corr-1".into()), Some("s1".into()))
        .with_attribute("tool", serde_json::json!("fs.read"));

    assert_eq!(event.event_type, "tool_call");
    assert_eq!(event.correlation_id.as_deref(), Some("corr-1"));
    assert_eq!(event.session_id.as_deref(), Some("s1"));
    assert_eq!(
        event.attributes.get("tool").unwrap(),
        &serde_json::json!("fs.read")
    );
}

#[test]
fn telemetry_event_metric_attributes() {
    let event = TelemetryEvent::new("agent_step", Some("corr-1".into()), Some("s1".into()));
    let attrs = event.metric_attributes();
    assert_eq!(attrs.get("event_type").unwrap(), "agent_step");
    assert_eq!(attrs.get("correlation_id").unwrap(), "corr-1");
    assert_eq!(attrs.get("session_id").unwrap(), "s1");
}

#[test]
fn telemetry_event_json_roundtrip() {
    let event = TelemetryEvent::new("test", None, None);
    let json = event.to_json().unwrap();
    let restored: TelemetryEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event.event_type, restored.event_type);
}
