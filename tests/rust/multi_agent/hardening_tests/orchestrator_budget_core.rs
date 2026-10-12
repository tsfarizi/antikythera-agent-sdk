

#[test]
/// OrchestratorBudget â€” step and task guardrails.
///
fn budget_step_limit_exhaustion() {
    use antikythera_core::application::agent::multi_agent::budget::OrchestratorBudget;
    let budget = OrchestratorBudget::new().with_max_total_steps(5);
    assert!(!budget.is_step_budget_exhausted());
    budget.record_steps(3);
    assert!(!budget.is_step_budget_exhausted());
    budget.record_steps(2);
    assert!(budget.is_step_budget_exhausted());
}

#[test]
fn budget_task_limit_exhaustion() {
    use antikythera_core::application::agent::multi_agent::budget::OrchestratorBudget;
    let budget = OrchestratorBudget::new().with_max_total_tasks(2);
    assert!(!budget.is_task_budget_exhausted());
    budget.record_task_dispatch();
    assert!(!budget.is_task_budget_exhausted());
    budget.record_task_dispatch();
    assert!(budget.is_task_budget_exhausted());
}
