#[test]
fn audit_trail_can_filter_by_category() {
    let trail = AuditTrail::new();
    trail.append(AuditRecord::new(
        AuditCategory::PolicyDecision,
        "allow_model",
        true,
        Some("corr-1".to_string()),
    ));
    trail.append(AuditRecord::new(
        AuditCategory::ToolExecution,
        "invoke_tool",
        true,
        Some("corr-1".to_string()),
    ));

    let policies = trail.by_category(AuditCategory::PolicyDecision);
    assert_eq!(policies.len(), 1);
    assert_eq!(policies[0].action, "allow_model");
}

#[test]
fn audit_record_with_detail_sets_fields() {
    let record = AuditRecord::new(AuditCategory::ToolExecution, "call_weather", true, None)
        .with_detail("tool", "weather");

    assert_eq!(record.details.get("tool"), Some(&"weather".to_string()));
}
