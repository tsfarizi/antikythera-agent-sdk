

#[test]
fn task_execution_metadata_defaults_to_no_failure() {
    let meta = TaskExecutionMetadata::default();
    assert_eq!(meta.attempt_count, 0);
    assert_eq!(meta.duration_ms, 0);
    assert!(!meta.timed_out);
    assert!(!meta.deadline_exceeded);
    assert!(!meta.cancelled);
    assert!(!meta.retry_applied);
    assert!(meta.routed_by.is_none());
    assert!(meta.correlation_id.is_none());
}

#[test]
fn task_execution_metadata_serde_roundtrip() {
    let meta = TaskExecutionMetadata {
        attempt_count: 2,
        duration_ms: 750,
        timed_out: false,
        deadline_exceeded: true,
        cancelled: false,
        retry_applied: true,
        routed_by: Some("round-robin".to_string()),
        execution_mode: Some("sequential".to_string()),
        correlation_id: Some("corr-xyz".to_string()),
        ..TaskExecutionMetadata::default()
    };

    let json = serde_json::to_string(&meta).expect("serialize");
    let restored: TaskExecutionMetadata = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored.attempt_count, 2);
    assert_eq!(restored.duration_ms, 750);
    assert!(restored.deadline_exceeded);
    assert!(restored.retry_applied);
    assert_eq!(restored.routed_by.as_deref(), Some("round-robin"));
    assert_eq!(restored.correlation_id.as_deref(), Some("corr-xyz"));
}
