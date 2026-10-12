//! Observability crate integration tests — telemetry types.

use antikythera_observability::{CallerContext, TelemetryEvent};

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
fn telemetry_event_metric_attributes() {
    let event = TelemetryEvent::new("tool_call", Some("corr-1".into()), Some("s1".into()));
    let attrs = event.metric_attributes();
    assert_eq!(attrs.get("event_type").unwrap(), "tool_call");
    assert_eq!(attrs.get("correlation_id").unwrap(), "corr-1");
}

#[test]
fn telemetry_event_json_roundtrip() {
    let event = TelemetryEvent::new("test", None, None);
    let json = event.to_json().unwrap();
    let restored: TelemetryEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event.event_type, restored.event_type);
}
