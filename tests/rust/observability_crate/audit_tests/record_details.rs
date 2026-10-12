#[test]
fn record_with_details() {
    let trail = AuditTrail::new();
    let record = AuditRecord::new(AuditCategory::PolicyDecision, "check", true, Some("c1".into()))
        .with_detail("model", "gpt-4");
    trail.append(record);
    let records = trail.snapshot();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].details.get("model").unwrap(), "gpt-4");
}

#[test]
fn clear_removes_all_records() {
    let trail = AuditTrail::new();
    trail.append(AuditRecord::new(AuditCategory::PolicyDecision, "x", true, None));
    trail.clear();
    assert!(trail.snapshot().is_empty());
}
