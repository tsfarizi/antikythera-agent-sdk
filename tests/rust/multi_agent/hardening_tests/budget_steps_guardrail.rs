

#[test]
/// Budget_steps guardrail â€” verifies the min() semantics used in orchestrator.
///
/// Mirrors the computation in execute_task: budgeted_max_steps = task.budget_steps.map(|b|
/// b.min(max_steps)).unwrap_or(max_steps).
///
fn budget_steps_guardrail_caps_at_max_steps() {
    let max_steps = 10usize;

    let task_with_lower_budget = AgentTask::new("test").with_budget_steps(6);
    let budgeted = task_with_lower_budget
        .budget_steps
        .map(|b| b.min(max_steps))
        .unwrap_or(max_steps);
    assert_eq!(budgeted, 6, "budget_steps < max_steps â†’ budget_steps wins");

    let task_with_higher_budget = AgentTask::new("test").with_budget_steps(15);
    let budgeted = task_with_higher_budget
        .budget_steps
        .map(|b| b.min(max_steps))
        .unwrap_or(max_steps);
    assert_eq!(
        budgeted, 10,
        "budget_steps > max_steps â†’ max_steps wins (guardrail)"
    );

    let task_without_budget = AgentTask::new("test");
    let budgeted = task_without_budget
        .budget_steps
        .map(|b| b.min(max_steps))
        .unwrap_or(max_steps);
    assert_eq!(budgeted, 10, "no budget_steps â†’ max_steps used");
}
