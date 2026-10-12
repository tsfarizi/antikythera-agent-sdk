#[test]
fn audit_trail_captures_policy_and_tool_events() {
    let trail = AuditTrail::new();
    trail.append(AuditRecord::new(
        AuditCategory::PolicyDecision,
        "allow:model:gpt-4",
        true,
        Some("corr-123".to_string()),
    ));
    trail.append(AuditRecord::new(
        AuditCategory::ToolExecution,
        "deny:tool:filesystem.write",
        false,
        Some("corr-123".to_string()),
    ));

    assert_eq!(trail.by_category(AuditCategory::PolicyDecision).len(), 1);
    assert_eq!(trail.by_category(AuditCategory::ToolExecution).len(), 1);
}
