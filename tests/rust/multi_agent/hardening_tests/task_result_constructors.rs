

#[test]
fn task_result_success_constructor_marks_success() {
    let result = TaskResult::success(
        "task-1".to_string(),
        "agent-a".to_string(),
        serde_json::json!({"answer": 42}),
        5,
        "sess-1".to_string(),
    );

    assert!(result.success);
    assert!(result.error.is_none());
    assert_eq!(result.steps_used, 5);
    assert_eq!(result.output["answer"], 42);
}

#[test]
fn task_result_failure_constructor_marks_failure() {
    let result = TaskResult::failure(
        "task-2".to_string(),
        "agent-b".to_string(),
        "LLM timeout".to_string(),
    );

    assert!(!result.success);
    assert_eq!(result.error.as_deref(), Some("LLM timeout"));
    assert_eq!(result.steps_used, 0);
    assert!(result.output.is_null());
}
