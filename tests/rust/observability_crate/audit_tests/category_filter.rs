#[test]
fn filter_by_category() {
    let trail = AuditTrail::new();
    trail.append(AuditRecord::new(AuditCategory::PolicyDecision, "allow:model", true, None));
    trail.append(AuditRecord::new(AuditCategory::ToolExecution, "deny:tool", false, None));
    assert_eq!(trail.by_category(AuditCategory::PolicyDecision).len(), 1);
    assert_eq!(trail.by_category(AuditCategory::ToolExecution).len(), 1);
}
