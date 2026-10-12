

#[test]
fn agent_task_builder_sets_all_fields() {
    let policy = TaskRetryPolicy {
        max_retries: 3,
        backoff_ms: 250,
        ..TaskRetryPolicy::default()
    };

    let task = AgentTask::new("analyse this code")
        .for_agent("code-reviewer")
        .with_session("sess-42")
        .with_max_steps(15)
        .with_timeout_ms(5_000)
        .with_retry_policy(policy.clone())
        .with_budget_steps(12)
        .with_correlation_id("corr-abc")
        .with_metadata("priority", "high");

    assert_eq!(task.input, "analyse this code");
    assert_eq!(task.agent_id.as_deref(), Some("code-reviewer"));
    assert_eq!(task.session_id.as_deref(), Some("sess-42"));
    assert_eq!(task.max_steps, Some(15));
    assert_eq!(task.timeout_ms, Some(5_000));
    assert_eq!(task.budget_steps, Some(12));
    assert_eq!(task.correlation_id.as_deref(), Some("corr-abc"));
    assert_eq!(task.metadata["priority"], Value::String("high".to_string()));

    let rp = task.retry_policy.unwrap();
    assert_eq!(rp.max_retries, 3);
    assert_eq!(rp.backoff_ms, 250);
}

#[test]
fn agent_task_auto_generates_unique_ids() {
    let t1 = AgentTask::new("task 1");
    let t2 = AgentTask::new("task 2");
    assert_ne!(t1.task_id, t2.task_id, "auto-generated IDs must be unique");
}

#[test]
fn agent_task_serde_roundtrip() {
    let task = AgentTask::new("do something")
        .for_agent("planner")
        .with_max_steps(8)
        .with_budget_steps(6)
        .with_retry_policy(TaskRetryPolicy {
            max_retries: 2,
            backoff_ms: 100,
            ..TaskRetryPolicy::default()
        });

    let json = serde_json::to_string(&task).expect("serialize");
    let restored: AgentTask = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored.task_id, task.task_id);
    assert_eq!(restored.agent_id, task.agent_id);
    assert_eq!(restored.max_steps, task.max_steps);
    assert_eq!(restored.budget_steps, task.budget_steps);
    let rp = restored.retry_policy.unwrap();
    assert_eq!(rp.max_retries, 2);
    assert_eq!(rp.backoff_ms, 100);
}
