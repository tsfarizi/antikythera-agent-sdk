#[test]
fn policy_audit_event_serialization() {
    let event = PolicyAuditEvent::new(
        Some("corr-123".to_string()),
        Some("sess-456".to_string()),
        PolicyEventType::ContextPolicyApplied,
        "context_policy",
        "allow",
        "context window within limits",
    )
    .with_resource("agent-001");

    let json = event.to_json().unwrap();
    assert!(json.contains("context_policy"));
    assert!(json.contains("corr-123"));
}

#[test]
fn in_memory_audit_sink() {
    let sink = InMemoryAuditSink::new();

    let event1 = PolicyAuditEvent::new(
        None,
        Some("s1".to_string()),
        PolicyEventType::ToolAccessGranted,
        "tool_policy",
        "allow",
        "tool authorized",
    );
    sink.record_event(event1);

    let event2 = PolicyAuditEvent::new(
        None,
        Some("s1".to_string()),
        PolicyEventType::ToolAccessDenied,
        "tool_policy",
        "deny",
        "tool not in allowlist",
    );
    sink.record_event(event2);

    let snapshot = sink.snapshot();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot[0].event_type, PolicyEventType::ToolAccessGranted);
    assert_eq!(snapshot[1].event_type, PolicyEventType::ToolAccessDenied);
}
