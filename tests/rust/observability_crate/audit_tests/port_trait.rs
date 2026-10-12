#[tokio::test]
async fn port_trait_record_event_works() {
    let trail = AuditTrail::new();
    antikythera_ports::AuditSink::record_event(
        &trail,
        "policy_decision",
        "allow:model:gpt-4",
        true,
        vec![("env".into(), "test".into())],
    ).await;
    let records = trail.snapshot();
    assert_eq!(records.len(), 1);
    assert!(records[0].allowed);
}
