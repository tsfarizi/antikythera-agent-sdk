

#[test]
fn pipeline_result_all_success_computes_totals() {
    let results = vec![
        TaskResult::success(
            "t1".into(),
            "a".into(),
            serde_json::json!("first"),
            3,
            "s".into(),
        ),
        TaskResult::success(
            "t2".into(),
            "b".into(),
            serde_json::json!("second"),
            4,
            "s".into(),
        ),
    ];

    let pipeline = PipelineResult::from_results(results);

    assert!(pipeline.success);
    assert_eq!(pipeline.total_steps, 7);
    assert_eq!(pipeline.final_output, serde_json::json!("second"));
    assert!(pipeline.error.is_none());
}

#[test]
fn pipeline_result_with_failure_reports_first_error() {
    let results = vec![
        TaskResult::success("t1".into(), "a".into(), Value::Null, 2, "s".into()),
        TaskResult::failure("t2".into(), "b".into(), "tool error".to_string()),
        TaskResult::success("t3".into(), "c".into(), Value::Null, 1, "s".into()),
    ];

    let pipeline = PipelineResult::from_results(results);

    assert!(!pipeline.success);
    assert_eq!(pipeline.error.as_deref(), Some("tool error"));
}

#[test]
/// Vacuously true â€” no failures.
///
fn pipeline_result_empty_has_null_output() {
    let pipeline = PipelineResult::from_results(vec![]);
    assert!(pipeline.final_output.is_null());
    assert!(pipeline.success);
    assert_eq!(pipeline.total_steps, 0);
}
